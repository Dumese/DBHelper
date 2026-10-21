//! Oracle 驱动（rust-oracle / ODPI-C）。
//!
//! Oracle 无纯 Rust 协议实现，rust-oracle 通过 ODPI-C 调用 OCI。
//! 因此：**编译期不需要 Oracle 客户端；运行期需要 Oracle Instant Client**（23ai 起支持 thin 模式）。
//! 所有同步调用统一经 `spawn_blocking` 隔离，避免阻塞 tokio 运行时。

use async_trait::async_trait;
use dbhelper_models::*;
use rel_common::*;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

pub struct OracleDriver {
    config: ConnectionConfig,
    password: String,
    conn: Mutex<Option<Arc<Mutex<oracle::Connection>>>>,
}

impl OracleDriver {
    pub fn new(config: ConnectionConfig) -> Self {
        Self { config, password: String::new(), conn: Mutex::new(None) }
    }

    pub fn set_password(&mut self, password: String) {
        self.password = password;
    }

    fn connect_string(&self) -> String {
        if self.config.host.contains('(') {
            // 已填写完整 TNS 描述符
            return self.config.host.clone();
        }
        let service = if self.config.database.is_empty() {
            "ORCL"
        } else {
            &self.config.database
        };
        format!("{}:{}/{}", self.config.host, self.config.port, service)
    }

    fn handle(&self) -> DriverResult<Arc<Mutex<oracle::Connection>>> {
        self.conn
            .lock()
            .ok()
            .and_then(|g| g.clone())
            .ok_or(DriverError::NotConnected)
    }
}

fn run_blocking<T, F>(f: F) -> impl std::future::Future<Output = DriverResult<T>>
where
    F: FnOnce() -> DriverResult<T> + Send + 'static,
    T: Send + 'static,
{
    async move {
        tokio::task::spawn_blocking(f)
            .await
            .map_err(|e| DriverError::Internal(e.to_string()))?
    }
}

fn decode_row(row: &oracle::Row, count: usize) -> Row {
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        // 按列类型逐次尝试：整数优先保精度，浮点排除 NaN/Inf，字符串兜日期/大数/文本。
        let value = row
            .get::<usize, Option<i64>>(i)
            .ok()
            .flatten()
            .map(Value::Int)
            .or_else(|| {
                row.get::<usize, Option<f64>>(i)
                    .ok()
                    .flatten()
                    .map(|f| if f.is_finite() { Value::Float(f) } else { Value::Null })
            })
            .or_else(|| row.get::<usize, Option<String>>(i).ok().flatten().map(Value::Text))
            .or_else(|| row.get::<usize, Option<bool>>(i).ok().flatten().map(Value::Bool))
            .unwrap_or(Value::Null);
        out.push(value);
    }
    out
}

#[async_trait]
impl RelationalDriver for OracleDriver {
    fn kind(&self) -> DbKind {
        DbKind::Oracle
    }

    fn config(&self) -> &ConnectionConfig {
        &self.config
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            multi_database: false,
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
        self.conn.lock().map(|g| g.is_some()).unwrap_or(false)
    }

    async fn connect(&self) -> DriverResult<()> {
        if self.is_connected() {
            return Ok(());
        }
        let user = self.config.username.clone();
        let pass = self.password.clone();
        let dsn = self.connect_string();
        let timeout = self.config.connect_timeout_ms;
        let conn = run_blocking(move || {
            let conn = oracle::Connection::connect(&user, &pass, &dsn)
                .map_err(|e| DriverError::Connect(format!("{e}（请确认已安装 Oracle Instant Client）")))?;
            let _ = conn.set_call_timeout(Some(std::time::Duration::from_millis(timeout)));
            Ok(Arc::new(Mutex::new(conn)))
        })
        .await?;
        *self.conn.lock().unwrap() = Some(conn);
        Ok(())
    }

    async fn close(&self) {
        *self.conn.lock().unwrap() = None;
    }

    async fn ping(&self) -> DriverResult<u64> {
        let conn = self.handle()?;
        let start = Instant::now();
        run_blocking(move || {
            let guard = conn.lock().map_err(|_| DriverError::Internal("连接锁中毒".into()))?;
            guard.query("SELECT 1 FROM DUAL", &[]).map(|_| ()).map_err(|e| DriverError::Query(e.to_string()))
        })
        .await?;
        Ok(start.elapsed().as_millis() as u64)
    }

    async fn server_info(&self) -> DriverResult<ServerInfo> {
        let conn = self.handle()?;
        let (version, db) = run_blocking(move || {
            let guard = conn.lock().map_err(|_| DriverError::Internal("连接锁中毒".into()))?;
            let mut rs = guard
                .query("SELECT banner FROM v$version WHERE ROWNUM = 1", &[])
                .map_err(|e| DriverError::Query(e.to_string()))?;
            let banner = rs
                .next()
                .and_then(|r| r.ok())
                .and_then(|r| r.get::<usize, String>(0).ok())
                .unwrap_or_else(|| "Oracle".into());
            let mut rs2 = guard
                .query("SELECT SYS_CONTEXT('USERENV','DB_NAME') FROM DUAL", &[])
                .map_err(|e| DriverError::Query(e.to_string()))?;
            let db = rs2
                .next()
                .and_then(|r| r.ok())
                .and_then(|r| r.get::<usize, String>(0).ok())
                .unwrap_or_default();
            Ok((banner, db))
        })
        .await?;
        Ok(ServerInfo {
            version,
            edition: "Oracle Database".into(),
            uptime_secs: None,
            current_database: db,
            extra: Vec::new(),
        })
    }

    async fn list_databases(&self) -> DriverResult<Vec<String>> {
        Ok(vec!["ORCL".into()])
    }

    async fn list_objects(
        &self,
        _database: Option<&str>,
        kinds: &[ObjectKind],
    ) -> DriverResult<Vec<SchemaObject>> {
        let conn = self.handle()?;
        let want_tables = kinds.is_empty() || kinds.contains(&ObjectKind::Table);
        let want_views = kinds.is_empty() || kinds.contains(&ObjectKind::View);
        run_blocking(move || {
            let guard = conn.lock().map_err(|_| DriverError::Internal("连接锁中毒".into()))?;
            let mut out = Vec::new();
            if want_tables {
                let mut rs = guard
                    .query(
                        "SELECT table_name, num_rows FROM user_tables ORDER BY table_name",
                        &[],
                    )
                    .map_err(|e| DriverError::Query(e.to_string()))?;
                while let Some(Ok(row)) = rs.next() {
                    out.push(SchemaObject {
                        name: row.get::<usize, String>(0).unwrap_or_default(),
                        kind: ObjectKind::Table,
                        schema: None,
                        row_estimate: row.get::<usize, Option<i64>>(1).ok().flatten().map(|v| v as u64),
                        comment: None,
                    });
                }
            }
            if want_views {
                let mut rs = guard
                    .query("SELECT view_name FROM user_views ORDER BY view_name", &[])
                    .map_err(|e| DriverError::Query(e.to_string()))?;
                while let Some(Ok(row)) = rs.next() {
                    out.push(SchemaObject {
                        name: row.get::<usize, String>(0).unwrap_or_default(),
                        kind: ObjectKind::View,
                        schema: None,
                        row_estimate: None,
                        comment: None,
                    });
                }
            }
            Ok(out)
        })
        .await
    }

    async fn describe_table(&self, _database: Option<&str>, table: &str) -> DriverResult<TableDef> {
        let conn = self.handle()?;
        let table_owned = table.to_string();
        let name_for_query = table.to_uppercase();
        let columns = run_blocking(move || {
            let guard = conn.lock().map_err(|_| DriverError::Internal("连接锁中毒".into()))?;
            let mut rs = guard
                .query(
                    "SELECT c.column_name, c.data_type, c.data_length, c.data_precision, c.data_scale, c.nullable, \
                        CASE WHEN pk.column_name IS NULL THEN 0 ELSE 1 END \
                     FROM user_tab_columns c LEFT JOIN ( \
                        SELECT cc.column_name FROM user_constraints uc \
                        JOIN user_cons_columns cc ON cc.constraint_name = uc.constraint_name \
                        WHERE uc.constraint_type = 'P' AND uc.table_name = :1) pk \
                     ON pk.column_name = c.column_name \
                     WHERE c.table_name = :1 ORDER BY c.column_id",
                    &[&name_for_query],
                )
                .map_err(|e| DriverError::Query(e.to_string()))?;
            let mut cols = Vec::new();
            let mut ordinal = 0i64;
            while let Some(Ok(row)) = rs.next() {
                let base: String = row.get(1).unwrap_or_default();
                let precision: Option<i64> = row.get(3).ok().flatten();
                let scale: Option<i64> = row.get(4).ok().flatten();
                let data_type = match (precision, scale) {
                    (Some(p), Some(s)) if base.contains("NUMBER") => format!("NUMBER({p},{s})"),
                    (Some(p), _) if base.contains("NUMBER") => format!("NUMBER({p})"),
                    _ => base,
                };
                cols.push(ColumnDef {
                    name: row.get(0).unwrap_or_default(),
                    data_type,
                    nullable: row.get::<usize, String>(5).map(|v| v == "Y").unwrap_or(true),
                    default: None,
                    is_pk: row.get::<usize, i64>(6).unwrap_or(0) == 1,
                    is_unique: false,
                    fk: None,
                    extra: String::new(),
                    comment: None,
                    ordinal,
                });
                ordinal += 1;
            }
            Ok(cols)
        })
        .await?;

        Ok(TableDef {
            schema: None,
            name: table_owned,
            columns,
            indexes: Vec::new(),
            row_estimate: None,
            comment: None,
        })
    }

    async fn query(&self, sql: &str, limit: u32, timeout: Duration) -> DriverResult<QueryOutcome> {
        let conn = self.handle()?;
        let sql = limit_sql(DbKind::Oracle, sql, limit);
        let sql_for_flag = sql.clone();
        let start = Instant::now();
        let fut = run_blocking(move || {
            let guard = conn.lock().map_err(|_| DriverError::Internal("连接锁中毒".into()))?;
            let mut rs = guard.query(&sql, &[]).map_err(|e| DriverError::Query(e.to_string()))?;
            let columns: Vec<ColumnMeta> = rs
                .column_info()
                .iter()
                .map(|c| ColumnMeta::new(c.name(), format!("{:?}", c.oracle_type())))
                .collect();
            let count = columns.len();
            let mut rows = Vec::new();
            let mut truncated = false;
            while let Some(item) = rs.next() {
                match item {
                    Ok(row) => {
                        if rows.len() >= limit as usize {
                            truncated = true;
                            break;
                        }
                        rows.push(decode_row(&row, count));
                    }
                    Err(e) => return Err(DriverError::Query(e.to_string())),
                }
            }
            Ok((columns, rows, truncated))
        });
        let (columns, rows, truncated) =
            tokio::time::timeout(timeout, fut)
                .await
                .map_err(|_| DriverError::Timeout(timeout.as_millis() as u64))??;
        Ok(QueryOutcome {
            columns,
            rows,
            rows_affected: 0,
            elapsed_ms: start.elapsed().as_millis() as u64,
            truncated,
            limit,
            notices: Vec::new(),
            read_only: is_read_only_sql(sql_for_flag.as_str()),
        })
    }

    async fn stream(
        &self,
        sql: String,
        limit: u32,
        timeout: Duration,
        tx: mpsc::Sender<StreamEvent>,
    ) -> DriverResult<()> {
        // OCI 为同步接口：以一次性查询 + 分批推送实现背压。
        let this: &OracleDriver = self;
        let outcome = this.query(&sql, limit, timeout).await;
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
                        rows_affected: 0,
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
        let conn = self.handle()?;
        let sql = sql.to_string();
        let start = Instant::now();
        let fut = run_blocking(move || {
            let guard = conn.lock().map_err(|_| DriverError::Internal("连接锁中毒".into()))?;
            let stmt = guard.execute(&sql, &[]).map_err(|e| DriverError::Query(e.to_string()))?;
            let affected = stmt.row_count().unwrap_or(0);
            Ok(affected)
        });
        let rows_affected = tokio::time::timeout(timeout, fut)
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
        let conn = self.handle()?;
        let sql = sql.to_string();
        let plan = run_blocking(move || {
            let guard = conn.lock().map_err(|_| DriverError::Internal("连接锁中毒".into()))?;
            let mut rs = guard
                .query(&format!("EXPLAIN PLAN FOR {sql}"), &[])
                .map_err(|e| DriverError::Query(e.to_string()))?;
            while rs.next().is_some() {}
            let mut rs2 = guard
                .query(
                    "SELECT PLAN_TABLE_OUTPUT FROM TABLE(DBMS_XPLAN.DISPLAY())",
                    &[],
                )
                .map_err(|e| DriverError::Query(e.to_string()))?;
            let mut lines = Vec::new();
            while let Some(Ok(row)) = rs2.next() {
                if let Ok(line) = row.get::<usize, String>(0) {
                    lines.push(Value::Text(line));
                }
            }
            Ok(lines)
        })
        .await?;
        Ok(QueryOutcome {
            columns: vec![ColumnMeta::new("PLAN_TABLE_OUTPUT", "VARCHAR2")],
            rows: plan.into_iter().map(|v| vec![v]).collect(),
            read_only: true,
            ..Default::default()
        })
    }

    async fn ddl(&self, database: Option<&str>, table: &str) -> DriverResult<String> {
        let def = self.describe_table(database, table).await?;
        self.create_table_ddl(&def, database).await
    }

    async fn estimate_affected(&self, sql: &str) -> DriverResult<Option<u64>> {
        let Some(count_sql) = estimate_sql(DbKind::Oracle, sql) else { return Ok(None) };
        let out = self.query(&count_sql, 1, Duration::from_secs(10)).await?;
        Ok(out.rows.first().and_then(|r| r.first()).map(|v| v.display().parse().unwrap_or(0)))
    }

    async fn count_rows(&self, _database: Option<&str>, table: &str) -> DriverResult<u64> {
        let out = self
            .query(&format!("SELECT COUNT(*) FROM {}", quote_ident(DbKind::Oracle, table)), 1, Duration::from_secs(15))
            .await?;
        Ok(out.rows.first().and_then(|r| r.first()).map(|v| v.display().parse().unwrap_or(0)).unwrap_or(0))
    }

    async fn sessions(&self) -> DriverResult<Vec<SessionInfo>> {
        let out = self
            .query(
                "SELECT sid, username, status, sql_id, LAST_CALL_ET FROM v$session WHERE type = 'USER'",
                200,
                Duration::from_secs(10),
            )
            .await?;
        Ok(out
            .rows
            .iter()
            .filter(|r| r.len() >= 5)
            .map(|r| SessionInfo {
                id: r[0].display(),
                user: r[1].display(),
                database: String::new(),
                state: r[2].display(),
                command: r[3].display(),
                elapsed_secs: r[4].display().parse().unwrap_or(0.0),
                sql: String::new(),
            })
            .collect())
    }

    async fn kill_session(&self, id: &str) -> DriverResult<()> {
        let sid: i64 = id.parse().map_err(|_| DriverError::Query("会话 ID 非法".into()))?;
        self.execute(&format!("ALTER SYSTEM KILL SESSION '{sid},1'"), Duration::from_secs(5))
            .await
            .map(|_| ())
    }

    async fn create_table_ddl(&self, def: &TableDef, _database: Option<&str>) -> DriverResult<String> {
        let mut cols = Vec::new();
        for c in &def.columns {
            let mut line = format!("  {} {}", quote_ident(DbKind::Oracle, &c.name), c.data_type);
            if !c.nullable {
                line.push_str(" NOT NULL");
            }
            cols.push(line);
        }
        let pks: Vec<String> = def
            .columns
            .iter()
            .filter(|c| c.is_pk)
            .map(|c| quote_ident(DbKind::Oracle, &c.name))
            .collect();
        if !pks.is_empty() {
            cols.push(format!("  PRIMARY KEY ({})", pks.join(", ")));
        }
        Ok(format!("CREATE TABLE {} (\n{}\n)", quote_ident(DbKind::Oracle, &def.name), cols.join(",\n")))
    }
}

// ---------------------------------------------------------------------------
// 写入（行内编辑 / CSV 导入）
// ---------------------------------------------------------------------------
//
// rust-oracle 0.6 未暴露稳定的参数化绑定（execute 的 binds 需要 OCI 侧类型构造，
// 各类型映射成本高且易错），因此这里渲染为**字面量**：
// - 标识符：`"name"`，内部 `"` → `""`（Oracle 标准转义）
// - 字符串：`'text'`，内部 `'` → `''`
// - 数值 / NULL / 布尔不产生可注入内容。

#[async_trait]
impl RowWriter for OracleDriver {
    async fn update_row(
        &self,
        table: &str,
        pk: &[(String, Value)],
        changes: &[(String, Value)],
    ) -> DriverResult<u64> {
        if changes.is_empty() {
            return Ok(0);
        }
        let mut sql = format!("UPDATE {} SET ", oracle_ident(table));
        for (i, (col, value)) in changes.iter().enumerate() {
            if i > 0 {
                sql.push_str(", ");
            }
            sql.push_str(&format!("{} = {}", oracle_ident(col), oracle_literal(value)));
        }
        sql.push_str(" WHERE ");
        for (i, (col, value)) in pk.iter().enumerate() {
            if i > 0 {
                sql.push_str(" AND ");
            }
            sql.push_str(&format!("{} = {}", oracle_ident(col), oracle_literal(value)));
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
            .map(|c| oracle_ident(c))
            .collect::<Vec<_>>()
            .join(", ");
        let mut sql = format!("INSERT INTO {} ({}) VALUES ", oracle_ident(table), cols);
        let values: Vec<String> = rows
            .iter()
            .map(|row| {
                let cells: Vec<String> = row.iter().map(oracle_literal).collect();
                format!("({})", cells.join(", "))
            })
            .collect();
        sql.push_str(&values.join(", "));
        self.execute(&sql, Duration::from_secs(60)).await.map(|o| o.rows_affected)
    }
}

/// Oracle 标识符转义：`"name"`，内部 `"` → `""`。
fn oracle_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// 渲染为 SQL 字面量。
fn oracle_literal(value: &Value) -> String {
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
            let mut s = String::from("HEXTORAW('");
            for byte in b {
                s.push_str(&format!("{byte:02X}"));
            }
            s.push_str("')");
            s
        }
        other => format!("'{}'", other.display().replace('\'', "''")),
    }
}
