//! SQL Server 驱动（tiberius，纯 Rust TDS 实现，不依赖 ODBC）。
//!
//! 由于 sqlx 的 mssql 后端仍在实验阶段，这里直接使用 tiberius 以获得
//! 完整的数据类型与流式结果支持。

use async_trait::async_trait;
use dbhelper_models::*;
use rel_common::*;
use std::time::{Duration, Instant};
use tiberius::{AuthMethod, Client, ColumnData, Config, EncryptionLevel, Row as TRow};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Mutex};
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

pub struct MssqlDriver {
    config: ConnectionConfig,
    password: String,
    client: Mutex<Option<Client<Compat<TcpStream>>>>,
}

impl MssqlDriver {
    pub fn new(config: ConnectionConfig) -> Self {
        Self { config, password: String::new(), client: Mutex::new(None) }
    }

    pub fn set_password(&mut self, password: String) {
        self.password = password;
    }

}

fn encryption(mode: SslMode) -> EncryptionLevel {
    match mode {
        SslMode::Disabled => EncryptionLevel::NotSupported,
        _ => EncryptionLevel::Required,
    }
}

fn decode(value: ColumnData<'_>) -> Value {
    match value {
        ColumnData::U8(Some(v)) => Value::Int(v as i64),
        ColumnData::I16(Some(v)) => Value::Int(v as i64),
        ColumnData::I32(Some(v)) => Value::Int(v as i64),
        ColumnData::I64(Some(v)) => Value::Int(v),
        ColumnData::F32(Some(v)) => Value::Float(v as f64),
        ColumnData::F64(Some(v)) => Value::Float(v),
        ColumnData::Bit(Some(v)) => Value::Bool(v),
        ColumnData::String(Some(v)) => Value::Text(v.to_string()),
        ColumnData::Guid(Some(v)) => Value::Text(v.to_string()),
        ColumnData::Binary(Some(v)) => Value::Bytes(v.to_vec()),
        ColumnData::Numeric(Some(v)) => Value::Decimal(v.to_string()),
        ColumnData::Xml(Some(v)) => Value::Text(v.to_string()),
        _ => Value::Null,
    }
}

fn decode_rows(rows: Vec<TRow>) -> Vec<Row> {
    rows.into_iter()
        .map(|row| {
            let types: Vec<tiberius::ColumnType> =
                row.columns().iter().map(|c| c.column_type()).collect();
            let mut out = Vec::with_capacity(types.len());
            let mut iter = row.into_iter();
            for ty in types {
                let data = iter.next();
                out.push(decode_typed(ty, data));
            }
            out
        })
        .collect()
}

fn decode_typed(ty: tiberius::ColumnType, data: Option<ColumnData<'_>>) -> Value {
    use tiberius::ColumnType as CT;
    match data {
        Some(ColumnData::DateTime(Some(v))) => {
            Value::Timestamp(format_days(v.days() as i64 + 25_567, v.seconds_fragments() as i64 / 300))
        }
        Some(ColumnData::SmallDateTime(Some(v))) => {
            Value::Timestamp(format_days(v.days() as i64 + 25_567, v.seconds_fragments() as i64 / 300))
        }
        Some(ColumnData::Date(Some(v))) => Value::Timestamp(format_days(v.days() as i64 - 719_162, 0)),
        Some(ColumnData::Time(Some(v))) => Value::Text(format_time(v.increments() as i64 / 10_000_000)),
        Some(ColumnData::DateTime2(Some(v))) => {
            let d = v.date().days() as i64 - 719_162;
            Value::Timestamp(format_days(d, (v.time().increments() as i64) / 10_000_000))
        }
        Some(other) => decode(other),
        None => {
            let _ = ty;
            Value::Null
        }
    }
}

/// Howard Hinnant 的 civil_from_days：把自 1970-01-01 起的天数转为公历日期。
fn format_days(days_since_epoch: i64, seconds_of_day: i64) -> String {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    let (h, mi, se) = (seconds_of_day / 3600, (seconds_of_day % 3600) / 60, seconds_of_day % 60);
    format!("{year:04}-{m:02}-{d:02} {h:02}:{mi:02}:{se:02}")
}

fn format_time(seconds: i64) -> String {
    format!("{:02}:{:02}:{:02}", seconds / 3600, (seconds % 3600) / 60, seconds % 60)
}

fn columns_of(rows: &[TRow]) -> Vec<ColumnMeta> {
    rows.first()
        .map(|r| {
            r.columns()
                .iter()
                .map(|c| ColumnMeta::new(c.name(), format!("{:?}", c.column_type())))
                .collect()
        })
        .unwrap_or_default()
}

#[async_trait]
impl RelationalDriver for MssqlDriver {
    fn kind(&self) -> DbKind {
        DbKind::SqlServer
    }

    fn config(&self) -> &ConnectionConfig {
        &self.config
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            multi_database: true,
            schemas: true,
            sessions: true,
            kill_session: true,
            explain: true,
            ddl: true,
            procedures: true,
            triggers: true,
            events: false,
            backup: true,
        }
    }

    fn is_connected(&self) -> bool {
        self.client.try_lock().map(|g| g.is_some()).unwrap_or(true)
    }

    async fn connect(&self) -> DriverResult<()> {
        let mut cfg = Config::new();
        cfg.host(&self.config.host);
        cfg.port(self.config.port);
        if !self.config.database.is_empty() {
            cfg.database(&self.config.database);
        }
        cfg.authentication(AuthMethod::sql_server(&self.config.username, &self.password));
        cfg.encryption(encryption(self.config.ssl));
        // 仅「首选」模式做机会式加密且不校验对端身份；Required/VerifyCa/VerifyFull 均校验证书
        if matches!(self.config.ssl, SslMode::Preferred) {
            cfg.trust_cert();
        }
        cfg.application_name("DBHelper");

        let tcp = TcpStream::connect(cfg.get_addr())
            .await
            .map_err(|e| DriverError::Connect(e.to_string()))?;
        tcp.set_nodelay(true).ok();
        let client = Client::connect(cfg, tcp.compat_write())
            .await
            .map_err(|e| DriverError::Connect(e.to_string()))?;
        *self.client.lock().await = Some(client);
        Ok(())
    }

    async fn close(&self) {
        if let Some(client) = self.client.lock().await.take() {
            let _ = client.close().await;
        }
    }

    async fn ping(&self) -> DriverResult<u64> {
        let start = Instant::now();
        let mut guard = self.client.lock().await;
        let client = guard.as_mut().ok_or(DriverError::NotConnected)?;
        let stream = client
            .simple_query("SELECT 1")
            .await
            .map_err(|e| DriverError::Query(e.to_string()))?;
        stream
            .into_results()
            .await
            .map_err(|e| DriverError::Query(e.to_string()))?;
        Ok(start.elapsed().as_millis() as u64)
    }

    async fn server_info(&self) -> DriverResult<ServerInfo> {
        let rows = self
            .query("SELECT @@VERSION AS v, DB_NAME() AS db", 1, Duration::from_secs(10))
            .await?;
        let version = rows
            .rows
            .first()
            .and_then(|r| r.first())
            .map(|v| v.display())
            .unwrap_or_else(|| "SQL Server".into());
        Ok(ServerInfo {
            version: version.lines().next().unwrap_or("SQL Server").to_string(),
            edition: "Microsoft SQL Server".into(),
            uptime_secs: None,
            current_database: self.config.database.clone(),
            extra: Vec::new(),
        })
    }

    async fn list_databases(&self) -> DriverResult<Vec<String>> {
        let out = self
            .query("SELECT name FROM sys.databases WHERE database_id > 4 ORDER BY name", 200, Duration::from_secs(10))
            .await?;
        Ok(out.rows.iter().filter_map(|r| r.first().map(|v| v.display())).collect())
    }

    async fn list_objects(
        &self,
        _database: Option<&str>,
        kinds: &[ObjectKind],
    ) -> DriverResult<Vec<SchemaObject>> {
        let out = self
            .query(
                "SELECT s.name AS sch, o.name AS nm, o.type AS tp FROM sys.objects o \
                 JOIN sys.schemas s ON s.schema_id = o.schema_id \
                 WHERE o.type IN ('U','V','P','FN','IF','TF','TR') ORDER BY o.type, o.name",
                5000,
                Duration::from_secs(15),
            )
            .await?;
        let mut objects = Vec::new();
        for row in &out.rows {
            if row.len() < 3 {
                continue;
            }
            let schema = row[0].display();
            let name = row[1].display();
            let tp = row[2].display().trim().to_string();
            let kind = match tp.as_str() {
                "U" => ObjectKind::Table,
                "V" => ObjectKind::View,
                "P" => ObjectKind::Procedure,
                "FN" | "IF" | "TF" => ObjectKind::Function,
                "TR" => ObjectKind::Trigger,
                _ => continue,
            };
            if !kinds.is_empty() && !kinds.contains(&kind) {
                continue;
            }
            objects.push(SchemaObject {
                name,
                kind,
                schema: Some(schema),
                row_estimate: None,
                comment: None,
            });
        }
        Ok(objects)
    }

    async fn describe_table(&self, database: Option<&str>, table: &str) -> DriverResult<TableDef> {
        let schema = database.unwrap_or("dbo");
        let sql = format!(
            "SELECT c.name, t.name AS type_name, c.is_nullable, c.is_identity, \
                CASE WHEN pk.column_id IS NULL THEN 0 ELSE 1 END AS is_pk \
             FROM sys.columns c JOIN sys.types t ON t.user_type_id = c.user_type_id \
             JOIN sys.objects o ON o.object_id = c.object_id \
             LEFT JOIN (SELECT ic.column_id FROM sys.indexes i \
                        JOIN sys.index_columns ic ON ic.index_id = i.index_id AND ic.object_id = i.object_id \
                        WHERE i.is_primary_key = 1 AND i.object_id = OBJECT_ID('{schema}.{table}')) pk \
                ON pk.column_id = c.column_id \
             WHERE o.object_id = OBJECT_ID('{schema}.{table}') ORDER BY c.column_id"
        );
        let out = self.query(&sql, 500, Duration::from_secs(10)).await?;
        let mut columns = Vec::new();
        for (i, row) in out.rows.iter().enumerate() {
            if row.len() < 5 {
                continue;
            }
            columns.push(ColumnDef {
                name: row[0].display(),
                data_type: row[1].display(),
                nullable: matches!(row[2], Value::Bool(true)) || row[2].display() == "true",
                default: None,
                is_pk: matches!(row[4], Value::Bool(true)) || row[4].display() == "true",
                is_unique: false,
                fk: None,
                extra: if matches!(row[3], Value::Bool(true)) { "IDENTITY".into() } else { String::new() },
                comment: None,
                ordinal: i as i64,
            });
        }

        let fk_sql = format!(
            "SELECT c.name, OBJECT_NAME(fk.referenced_object_id), rc.name, fk.name \
             FROM sys.foreign_key_columns fkc \
             JOIN sys.objects fk ON fk.object_id = fkc.constraint_object_id \
             JOIN sys.columns c ON c.object_id = fkc.parent_object_id AND c.column_id = fkc.parent_column_id \
             JOIN sys.columns rc ON rc.object_id = fkc.referenced_object_id AND rc.column_id = fkc.referenced_column_id \
             WHERE fkc.parent_object_id = OBJECT_ID('{schema}.{table}')"
        );
        if let Ok(fks) = self.query(&fk_sql, 200, Duration::from_secs(10)).await {
            for row in &fks.rows {
                if row.len() < 4 {
                    continue;
                }
                let col = row[0].display();
                if let Some(def) = columns.iter_mut().find(|c| c.name == col) {
                    def.fk = Some(ForeignKey {
                        column: col,
                        ref_table: row[1].display(),
                        ref_column: row[2].display(),
                        constraint: row[3].display(),
                    });
                }
            }
        }

        Ok(TableDef {
            schema: Some(schema.to_string()),
            name: table.to_string(),
            columns,
            indexes: Vec::new(),
            row_estimate: None,
            comment: None,
        })
    }

    async fn query(&self, sql: &str, limit: u32, timeout: Duration) -> DriverResult<QueryOutcome> {
        let sql = limit_sql(DbKind::SqlServer, sql, limit);
        let start = Instant::now();
        let all = self.run_query(&sql, timeout).await?;
        let columns = columns_of(&all);
        let truncated = all.len() > limit as usize;
        let rows = decode_rows(all.into_iter().take(limit as usize).collect());
        Ok(QueryOutcome {
            columns,
            rows,
            rows_affected: 0,
            elapsed_ms: start.elapsed().as_millis() as u64,
            truncated,
            limit,
            notices: Vec::new(),
            read_only: is_read_only_sql(&sql),
        })
    }

    async fn stream(
        &self,
        sql: String,
        limit: u32,
        timeout: Duration,
        tx: mpsc::Sender<StreamEvent>,
    ) -> DriverResult<()> {
        // tiberius 的 Client 非 Clone：一次性取回后按批推送，保持有界通道背压语义。
        let outcome = self.query(&sql, limit, timeout).await;
        match outcome {
            Ok(out) => {
                let _ = tx
                    .send(StreamEvent::Columns { columns: out.columns.clone(), read_only: out.read_only })
                    .await;
                for chunk in out.rows.chunks(200) {
                    if tx.send(StreamEvent::Batch(chunk.to_vec())).await.is_err() {
                        return Ok(());
                    }
                }
                let _ = tx
                    .send(StreamEvent::Done {
                        elapsed_ms: out.elapsed_ms,
                        rows_affected: out.rows_affected,
                        truncated: out.truncated,
                    })
                    .await;
                Ok(())
            }
            Err(e) => {
                let _ = tx.send(StreamEvent::Failed(e.user_message())).await;
                Ok(())
            }
        }
    }

    async fn execute(&self, sql: &str, timeout: Duration) -> DriverResult<ExecOutcome> {
        let start = Instant::now();
        let mut guard = self.client.lock().await;
        let client = guard.as_mut().ok_or(DriverError::NotConnected)?;
        let affected = async {
            let stream = client
                .simple_query(sql)
                .await
                .map_err(|e| DriverError::Query(e.to_string()))?;
            let results = stream
                .into_results()
                .await
                .map_err(|e| DriverError::Query(e.to_string()))?;
            Ok::<u64, DriverError>(results.iter().map(|r| r.len() as u64).sum())
        };
        let rows_affected = tokio::time::timeout(timeout, affected)
            .await
            .map_err(|_| DriverError::Timeout(timeout.as_millis() as u64))??;
        Ok(ExecOutcome {
            rows_affected,
            elapsed_ms: start.elapsed().as_millis() as u64,
            last_insert_id: None,
            notices: Vec::new(),
        })
    }

    async fn explain(&self, sql: &str) -> DriverResult<QueryOutcome> {
        let sql = format!("SET SHOWPLAN_TEXT ON; {sql}");
        self.query(&sql, 200, Duration::from_secs(10)).await
    }

    async fn ddl(&self, database: Option<&str>, table: &str) -> DriverResult<String> {
        let def = self.describe_table(database, table).await?;
        self.create_table_ddl(&def, database).await
    }

    async fn estimate_affected(&self, sql: &str) -> DriverResult<Option<u64>> {
        let Some(count_sql) = estimate_sql(DbKind::SqlServer, sql) else { return Ok(None) };
        let out = self.query(&count_sql, 1, Duration::from_secs(10)).await?;
        Ok(out
            .rows
            .first()
            .and_then(|r| r.first())
            .map(|v| v.display().parse().unwrap_or(0)))
    }

    async fn count_rows(&self, _database: Option<&str>, table: &str) -> DriverResult<u64> {
        let out = self
            .query(&format!("SELECT COUNT(*) FROM {}", quote_ident(DbKind::SqlServer, table)), 1, Duration::from_secs(10))
            .await?;
        Ok(out
            .rows
            .first()
            .and_then(|r| r.first())
            .map(|v| v.display().parse().unwrap_or(0))
            .unwrap_or(0))
    }

    async fn sessions(&self) -> DriverResult<Vec<SessionInfo>> {
        let out = self
            .query(
                "SELECT s.session_id, s.login_name, DB_NAME(s.database_id), s.status, \
                    ISNULL(r.command,''), DATEDIFF(SECOND, r.start_time, GETDATE()), ISNULL(r.query_text,'') \
                 FROM sys.dm_exec_sessions s LEFT JOIN ( \
                    SELECT r.session_id, r.command, r.start_time, \
                        (SELECT TOP 1 text FROM sys.dm_exec_sql_text(r.sql_handle)) AS query_text \
                    FROM sys.dm_exec_requests r) r ON r.session_id = s.session_id \
                 WHERE s.is_user_process = 1",
                500,
                Duration::from_secs(10),
            )
            .await?;
        Ok(out
            .rows
            .iter()
            .filter(|r| r.len() >= 7)
            .map(|r| SessionInfo {
                id: r[0].display(),
                user: r[1].display(),
                database: r[2].display(),
                state: r[3].display(),
                command: r[4].display(),
                elapsed_secs: r[5].display().parse().unwrap_or(0.0),
                sql: r[6].display(),
            })
            .collect())
    }

    async fn kill_session(&self, id: &str) -> DriverResult<()> {
        let id: i32 = id.parse().map_err(|_| DriverError::Query("会话 ID 非法".into()))?;
        self.execute(&format!("KILL {id}"), Duration::from_secs(5)).await.map(|_| ())
    }

    async fn create_table_ddl(&self, def: &TableDef, _database: Option<&str>) -> DriverResult<String> {
        let mut cols = Vec::new();
        for c in &def.columns {
            let mut line = format!("  {} {}", quote_ident(DbKind::SqlServer, &c.name), c.data_type);
            if !c.nullable {
                line.push_str(" NOT NULL");
            }
            cols.push(line);
        }
        let pks: Vec<String> = def
            .columns
            .iter()
            .filter(|c| c.is_pk)
            .map(|c| quote_ident(DbKind::SqlServer, &c.name))
            .collect();
        if !pks.is_empty() {
            cols.push(format!("  PRIMARY KEY ({})", pks.join(", ")));
        }
        Ok(format!("CREATE TABLE {} (\n{}\n);", quote_ident(DbKind::SqlServer, &def.name), cols.join(",\n")))
    }
}

impl MssqlDriver {
    async fn run_query(&self, sql: &str, timeout: Duration) -> DriverResult<Vec<TRow>> {
        let mut guard = self.client.lock().await;
        let client = guard.as_mut().ok_or(DriverError::NotConnected)?;
        let fut = async {
            let stream = client
                .simple_query(sql)
                .await
                .map_err(|e| DriverError::Query(e.to_string()))?;
            stream
                .into_first_result()
                .await
                .map_err(|e| DriverError::Query(e.to_string()))
        };
        tokio::time::timeout(timeout, fut)
            .await
            .map_err(|_| DriverError::Timeout(timeout.as_millis() as u64))?
    }
}

// ---------------------------------------------------------------------------
// 写入（行内编辑 / CSV 导入）
// ---------------------------------------------------------------------------
//
// 注意：tiberius 0.12 未暴露参数化绑定接口（simple_query 只接受纯 SQL），
// 因此这里把值渲染为**字面量**。为避免注入，标识符与字符串都经过严格转义：
// - 标识符：`[` `]` 包裹，内部 `]` 转义为 `]]`
// - 字符串：`'` 包裹，内部 `'` 转义为 `''`（T-SQL 标准转义）
// - 其余类型（数值 / NULL / 布尔）不产生可注入内容。

#[async_trait]
impl RowWriter for MssqlDriver {
    async fn update_row(
        &self,
        table: &str,
        pk: &[(String, Value)],
        changes: &[(String, Value)],
    ) -> DriverResult<u64> {
        if changes.is_empty() {
            return Ok(0);
        }
        let mut sql = format!("UPDATE {} SET ", mssql_ident(table));
        for (i, (col, value)) in changes.iter().enumerate() {
            if i > 0 {
                sql.push_str(", ");
            }
            sql.push_str(&format!("{} = {}", mssql_ident(col), mssql_literal(value)));
        }
        sql.push_str(" WHERE ");
        for (i, (col, value)) in pk.iter().enumerate() {
            if i > 0 {
                sql.push_str(" AND ");
            }
            sql.push_str(&format!("{} = {}", mssql_ident(col), mssql_literal(value)));
        }
        self.execute(&sql, Duration::from_secs(30)).await.map(|o| o.rows_affected)
    }

    async fn insert_rows(
        &self,
        table: &str,
        columns: &[String],
        rows: &[Vec<Value>],
    ) -> DriverResult<u64> {
        if rows.is_empty() {
            return Ok(0);
        }
        let cols = columns
            .iter()
            .map(|c| mssql_ident(c))
            .collect::<Vec<_>>()
            .join(", ");
        let mut sql = format!("INSERT INTO {} ({}) VALUES ", mssql_ident(table), cols);
        let values: Vec<String> = rows
            .iter()
            .map(|row| {
                let cells: Vec<String> = row.iter().map(mssql_literal).collect();
                format!("({})", cells.join(", "))
            })
            .collect();
        sql.push_str(&values.join(", "));
        self.execute(&sql, Duration::from_secs(60)).await.map(|o| o.rows_affected)
    }
}

/// T-SQL 标识符转义：`[name]`，内部 `]` → `]]`。
fn mssql_ident(name: &str) -> String {
    format!("[{}]", name.replace(']', "]]"))
}

/// 渲染为 SQL 字面量。
fn mssql_literal(value: &Value) -> String {
    match value {
        Value::Null => "NULL".into(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => {
            if f.is_finite() {
                f.to_string()
            } else {
                "NULL".into()
            }
        }
        Value::Bool(b) => if *b { "1" } else { "0" }.into(),
        Value::Bytes(b) => {
            let mut s = String::from("0x");
            for byte in b {
                s.push_str(&format!("{byte:02X}"));
            }
            s
        }
        other => format!("'{}'", other.display().replace('\'', "''")),
    }
}
