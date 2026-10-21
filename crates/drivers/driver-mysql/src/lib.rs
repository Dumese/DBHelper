//! MySQL 驱动（sqlx / mysql feature）。

use async_trait::async_trait;
use dbhelper_models::*;
use futures::StreamExt;
use rel_common::*;
use sqlx::mysql::{MySqlConnectOptions, MySqlPoolOptions, MySqlRow, MySqlSslMode};
use sqlx::Row as _;
use sqlx::{Column, TypeInfo};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};

pub struct MysqlDriver {
    config: ConnectionConfig,
    pool: RwLock<Option<sqlx::MySqlPool>>,
    password: String,
}

impl MysqlDriver {
    pub fn new(config: ConnectionConfig) -> Self {
        Self { config, pool: RwLock::new(None), password: String::new() }
    }

    pub fn set_password(&mut self, password: String) {
        self.password = password;
    }

    async fn pool(&self) -> DriverResult<sqlx::MySqlPool> {
        self.pool.read().await.clone().ok_or(DriverError::NotConnected)
    }
}

fn ssl_mode(mode: SslMode) -> MySqlSslMode {
    match mode {
        SslMode::Disabled => MySqlSslMode::Disabled,
        SslMode::Preferred => MySqlSslMode::Preferred,
        SslMode::Required => MySqlSslMode::Required,
        SslMode::VerifyCa => MySqlSslMode::VerifyCa,
        SslMode::VerifyFull => MySqlSslMode::VerifyIdentity,
    }
}

pub fn decode_value(row: &MySqlRow, idx: usize) -> Value {
    let info = row.column(idx).type_info().name().to_ascii_uppercase();
    match info.as_str() {
        "TINYINT" | "SMALLINT" | "MEDIUMINT" | "INT" | "INTEGER" | "BIGINT" | "YEAR" => row
            .try_get::<Option<i64>, _>(idx)
            .ok()
            .flatten()
            .map(Value::Int)
            .unwrap_or(Value::Null),
        "TINYINT UNSIGNED" | "SMALLINT UNSIGNED" | "MEDIUMINT UNSIGNED" | "INT UNSIGNED" | "BIGINT UNSIGNED" => row
            .try_get::<Option<u64>, _>(idx)
            .ok()
            .flatten()
            .map(|v| Value::Int(v as i64))
            .unwrap_or(Value::Null),
        "FLOAT" | "DOUBLE" => row
            .try_get::<Option<f64>, _>(idx)
            .ok()
            .flatten()
            .map(Value::Float)
            .unwrap_or(Value::Null),
        "DECIMAL" | "NUMERIC" => row
            .try_get::<Option<sqlx::types::BigDecimal>, _>(idx)
            .ok()
            .flatten()
            .map(|v| Value::Decimal(v.to_string()))
            .unwrap_or(Value::Null),
        "BOOLEAN" | "BOOL" => row
            .try_get::<Option<bool>, _>(idx)
            .ok()
            .flatten()
            .map(Value::Bool)
            .unwrap_or(Value::Null),
        "DATE" => row
            .try_get::<Option<chrono::NaiveDate>, _>(idx)
            .ok()
            .flatten()
            .map(|v| Value::Timestamp(v.format("%Y-%m-%d").to_string()))
            .unwrap_or(Value::Null),
        "DATETIME" | "TIMESTAMP" => row
            .try_get::<Option<chrono::NaiveDateTime>, _>(idx)
            .ok()
            .flatten()
            .map(|v| Value::Timestamp(v.format("%Y-%m-%d %H:%M:%S").to_string()))
            .unwrap_or(Value::Null),
        "JSON" => row
            .try_get::<Option<serde_json::Value>, _>(idx)
            .ok()
            .flatten()
            .map(|v| Value::Json(v.to_string()))
            .unwrap_or(Value::Null),
        "BLOB" | "TINYBLOB" | "MEDIUMBLOB" | "LONGBLOB" | "BINARY" | "VARBINARY" | "BIT" => row
            .try_get::<Option<Vec<u8>>, _>(idx)
            .ok()
            .flatten()
            .map(Value::Bytes)
            .unwrap_or(Value::Null),
        "TIME" => row
            .try_get::<Option<chrono::NaiveTime>, _>(idx)
            .ok()
            .flatten()
            .map(|v| Value::Text(v.format("%H:%M:%S").to_string()))
            .unwrap_or(Value::Null),
        _ => row
            .try_get::<Option<String>, _>(idx)
            .ok()
            .flatten()
            .map(Value::Text)
            .unwrap_or(Value::Null),
    }
}

fn decode_row(row: &MySqlRow) -> Row {
    (0..row.len()).map(|i| decode_value(row, i)).collect()
}

fn columns_of(row: &MySqlRow) -> Vec<ColumnMeta> {
    row.columns()
        .iter()
        .map(|c| ColumnMeta::new(c.name(), c.type_info().name()))
        .collect()
}

#[async_trait]
impl RelationalDriver for MysqlDriver {
    fn kind(&self) -> DbKind {
        DbKind::Mysql
    }

    fn config(&self) -> &ConnectionConfig {
        &self.config
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            multi_database: true,
            schemas: false,
            sessions: true,
            kill_session: true,
            explain: true,
            ddl: true,
            procedures: true,
            triggers: true,
            events: true,
            backup: true,
        }
    }

    fn is_connected(&self) -> bool {
        self.pool.try_read().map(|p| p.is_some()).unwrap_or(false)
    }

    async fn connect(&self) -> DriverResult<()> {
        if self.is_connected() {
            return Ok(());
        }
        let mut opts = MySqlConnectOptions::new()
            .host(&self.config.host)
            .port(self.config.port)
            .username(&self.config.username)
            .ssl_mode(ssl_mode(self.config.ssl))
            .charset("utf8mb4");
        if !self.password.is_empty() {
            opts = opts.password(&self.password);
        }
        if !self.config.database.is_empty() {
            opts = opts.database(&self.config.database);
        }

        let pool = MySqlPoolOptions::new()
            .max_connections(4)
            .acquire_timeout(Duration::from_millis(self.config.connect_timeout_ms))
            .connect_with(opts)
            .await
            .map_err(|e| DriverError::Connect(e.to_string()))?;
        *self.pool.write().await = Some(pool);
        Ok(())
    }

    async fn close(&self) {
        if let Some(pool) = self.pool.write().await.take() {
            pool.close().await;
        }
    }

    async fn ping(&self) -> DriverResult<u64> {
        let pool = self.pool().await?;
        let start = Instant::now();
        sqlx::query("SELECT 1").execute(&pool).await.map_err(|e| DriverError::Query(e.to_string()))?;
        Ok(start.elapsed().as_millis() as u64)
    }

    async fn server_info(&self) -> DriverResult<ServerInfo> {
        let pool = self.pool().await?;
        let row: Option<(String, String)> = sqlx::query_as("SELECT VERSION(), @@version_comment")
            .fetch_optional(&pool)
            .await
            .map_err(|e| DriverError::Query(e.to_string()))?;
        let (version, edition) = row.unwrap_or(("unknown".into(), String::new()));
        let db: Option<String> = sqlx::query_scalar("SELECT DATABASE()").fetch_one(&pool).await.ok();
        Ok(ServerInfo {
            version: format!("MySQL {version}"),
            edition,
            uptime_secs: None,
            current_database: db.unwrap_or_else(|| self.config.database.clone()),
            extra: Vec::new(),
        })
    }

    async fn list_databases(&self) -> DriverResult<Vec<String>> {
        let pool = self.pool().await?;
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT schema_name FROM information_schema.schemata \
             WHERE schema_name NOT IN ('information_schema','mysql','performance_schema','sys') \
             ORDER BY schema_name",
        )
        .fetch_all(&pool)
        .await
        .map_err(|e| DriverError::Query(e.to_string()))?;
        Ok(rows.into_iter().map(|(n,)| n).collect())
    }

    async fn list_objects(
        &self,
        database: Option<&str>,
        kinds: &[ObjectKind],
    ) -> DriverResult<Vec<SchemaObject>> {
        let pool = self.pool().await?;
        let db = database.unwrap_or(&self.config.database).to_string();
        let mut out = Vec::new();

        if kinds.is_empty() || kinds.contains(&ObjectKind::Table) || kinds.contains(&ObjectKind::View) {
            let rows: Vec<(String, String, Option<u64>, Option<String>)> = sqlx::query_as(
                "SELECT table_name, table_type, table_rows, table_comment FROM information_schema.tables \
                 WHERE table_schema = ? ORDER BY table_type, table_name",
            )
            .bind(&db)
            .fetch_all(&pool)
            .await
            .map_err(|e| DriverError::Query(e.to_string()))?;
            for (name, ttype, rows, comment) in rows {
                out.push(SchemaObject {
                    name,
                    kind: if ttype.contains("VIEW") { ObjectKind::View } else { ObjectKind::Table },
                    schema: Some(db.clone()),
                    row_estimate: rows,
                    comment: comment.filter(|c| !c.is_empty()),
                });
            }
        }

        if kinds.is_empty() || kinds.contains(&ObjectKind::Procedure) || kinds.contains(&ObjectKind::Function) {
            let rows: Vec<(String, String)> = sqlx::query_as(
                "SELECT routine_name, routine_type FROM information_schema.routines \
                 WHERE routine_schema = ? ORDER BY routine_name",
            )
            .bind(&db)
            .fetch_all(&pool)
            .await
            .unwrap_or_default();
            for (name, rtype) in rows {
                out.push(SchemaObject {
                    name,
                    kind: if rtype.contains("FUNCTION") { ObjectKind::Function } else { ObjectKind::Procedure },
                    schema: Some(db.clone()),
                    row_estimate: None,
                    comment: None,
                });
            }
        }

        if kinds.is_empty() || kinds.contains(&ObjectKind::Trigger) {
            let rows: Vec<(String,)> = sqlx::query_as(
                "SELECT trigger_name FROM information_schema.triggers WHERE trigger_schema = ?",
            )
            .bind(&db)
            .fetch_all(&pool)
            .await
            .unwrap_or_default();
            for (name,) in rows {
                out.push(SchemaObject {
                    name,
                    kind: ObjectKind::Trigger,
                    schema: Some(db.clone()),
                    row_estimate: None,
                    comment: None,
                });
            }
        }

        Ok(out)
    }

    async fn describe_table(&self, database: Option<&str>, table: &str) -> DriverResult<TableDef> {
        let pool = self.pool().await?;
        let db = database.unwrap_or(&self.config.database).to_string();
        let cols: Vec<(String, String, String, Option<String>, String, Option<String>)> =
            sqlx::query_as(
                "SELECT column_name, column_type, is_nullable, column_default, column_key, column_comment \
                 FROM information_schema.columns WHERE table_schema = ? AND table_name = ? \
                 ORDER BY ordinal_position",
            )
            .bind(&db)
            .bind(table)
            .fetch_all(&pool)
            .await
            .map_err(|e| DriverError::Query(e.to_string()))?;

        let fks: Vec<(String, String, String, String)> = sqlx::query_as(
            "SELECT column_name, referenced_table_name, referenced_column_name, constraint_name \
             FROM information_schema.key_column_usage WHERE table_schema = ? AND table_name = ? \
             AND referenced_table_name IS NOT NULL",
        )
        .bind(&db)
        .bind(table)
        .fetch_all(&pool)
        .await
        .unwrap_or_default();

        let idx: Vec<(String, i64, String)> = sqlx::query_as(
            "SELECT index_name, non_unique, column_name FROM information_schema.statistics \
             WHERE table_schema = ? AND table_name = ? ORDER BY index_name, seq_in_index",
        )
        .bind(&db)
        .bind(table)
        .fetch_all(&pool)
        .await
        .unwrap_or_default();

        let mut columns = Vec::new();
        for (i, (name, ctype, nullable, default, key, comment)) in cols.into_iter().enumerate() {
            let fk = fks.iter().find(|(c, _, _, _)| c == &name).map(|(c, t, rc, cn)| ForeignKey {
                column: c.clone(),
                ref_table: t.clone(),
                ref_column: rc.clone(),
                constraint: cn.clone(),
            });
            columns.push(ColumnDef {
                name,
                data_type: ctype,
                nullable: nullable.eq_ignore_ascii_case("YES"),
                default,
                is_pk: key == "PRI",
                is_unique: key == "UNI",
                fk,
                extra: String::new(),
                comment: comment.filter(|c| !c.is_empty()),
                ordinal: i as i64,
            });
        }

        let mut indexes: Vec<IndexDef> = Vec::new();
        for (name, non_unique, column) in idx {
            match indexes.iter_mut().find(|i| i.name == name) {
                Some(existing) => existing.columns.push(column),
                None => indexes.push(IndexDef {
                    primary: name == "PRIMARY",
                    unique: non_unique == 0,
                    name,
                    columns: vec![column],
                }),
            }
        }

        let rows: Option<(u64,)> = sqlx::query_as("SELECT table_rows FROM information_schema.tables WHERE table_schema = ? AND table_name = ?")
            .bind(&db)
            .bind(table)
            .fetch_optional(&pool)
            .await
            .ok()
            .flatten();

        Ok(TableDef {
            schema: Some(db),
            name: table.to_string(),
            columns,
            indexes,
            row_estimate: rows.map(|r| r.0),
            comment: None,
        })
    }

    async fn query(&self, sql: &str, limit: u32, timeout: Duration) -> DriverResult<QueryOutcome> {
        let pool = self.pool().await?;
        let start = Instant::now();
        let fut = sqlx::query(sql).fetch_all(&pool);
        let rows = tokio::time::timeout(timeout, fut)
            .await
            .map_err(|_| DriverError::Timeout(timeout.as_millis() as u64))?
            .map_err(|e| DriverError::Query(e.to_string()))?;
        let columns = rows.first().map(columns_of).unwrap_or_default();
        let truncated = rows.len() > limit as usize;
        let rows: Vec<Row> = rows.iter().take(limit as usize).map(decode_row).collect();
        Ok(QueryOutcome {
            columns,
            rows,
            rows_affected: 0,
            elapsed_ms: start.elapsed().as_millis() as u64,
            truncated,
            limit,
            notices: Vec::new(),
            read_only: rel_common::is_read_only_sql(sql),
        })
    }

    async fn stream(
        &self,
        sql: String,
        limit: u32,
        timeout: Duration,
        tx: mpsc::Sender<StreamEvent>,
    ) -> DriverResult<()> {
        let pool = self.pool().await?;
        tokio::spawn(async move {
            let start = Instant::now();
            let work = async {
                let mut stream = sqlx::query(&sql).fetch(&pool);
                let mut sent = false;
                let mut batch: Vec<Row> = Vec::with_capacity(200);
                let mut total = 0usize;
                let mut truncated = false;
                while let Some(item) = stream.next().await {
                    match item {
                        Ok(row) => {
                            if !sent {
                                let _ = tx
                                    .send(StreamEvent::Columns {
                                        columns: columns_of(&row),
                                        read_only: rel_common::is_read_only_sql(&sql),
                                    })
                                    .await;
                                sent = true;
                            }
                            batch.push(decode_row(&row));
                            total += 1;
                            if batch.len() >= 200 {
                                if tx.send(StreamEvent::Batch(std::mem::take(&mut batch))).await.is_err() {
                                    return;
                                }
                            }
                            if total >= limit as usize {
                                truncated = true;
                                break;
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(StreamEvent::Failed(e.to_string())).await;
                            return;
                        }
                    }
                }
                if !batch.is_empty() {
                    let _ = tx.send(StreamEvent::Batch(batch)).await;
                }
                let _ = tx
                    .send(StreamEvent::Done {
                        elapsed_ms: start.elapsed().as_millis() as u64,
                        rows_affected: 0,
                        truncated,
                    })
                    .await;
            };
            if tokio::time::timeout(timeout, work).await.is_err() {
                let _ = tx
                    .send(StreamEvent::Failed(format!("执行超时（{} ms），已取消", timeout.as_millis())))
                    .await;
            }
        });
        Ok(())
    }

    async fn execute(&self, sql: &str, timeout: Duration) -> DriverResult<ExecOutcome> {
        let pool = self.pool().await?;
        let start = Instant::now();
        let fut = sqlx::query(sql).execute(&pool);
        let res = tokio::time::timeout(timeout, fut)
            .await
            .map_err(|_| DriverError::Timeout(timeout.as_millis() as u64))?
            .map_err(|e| DriverError::Query(e.to_string()))?;
        Ok(ExecOutcome {
            rows_affected: res.rows_affected(),
            elapsed_ms: start.elapsed().as_millis() as u64,
            last_insert_id: Some(res.last_insert_id() as i64),
            notices: Vec::new(),
        })
    }

    async fn explain(&self, sql: &str) -> DriverResult<QueryOutcome> {
        self.query(&format!("EXPLAIN {sql}"), 500, Duration::from_secs(15)).await
    }

    async fn ddl(&self, database: Option<&str>, table: &str) -> DriverResult<String> {
        let pool = self.pool().await?;
        let db = database.unwrap_or(&self.config.database);
        let sql = format!(
            "SHOW CREATE TABLE {}.{}",
            quote_ident(DbKind::Mysql, db),
            quote_ident(DbKind::Mysql, table)
        );
        let row: (String, String) = sqlx::query_as(&sql)
            .fetch_one(&pool)
            .await
            .map_err(|e| DriverError::Query(e.to_string()))?;
        Ok(format!("{};", row.1))
    }

    async fn estimate_affected(&self, sql: &str) -> DriverResult<Option<u64>> {
        let Some(count_sql) = estimate_sql(DbKind::Mysql, sql) else { return Ok(None) };
        let pool = self.pool().await?;
        let count: i64 = sqlx::query_scalar(&count_sql)
            .fetch_one(&pool)
            .await
            .map_err(|e| DriverError::Query(e.to_string()))?;
        Ok(Some(count as u64))
    }

    async fn count_rows(&self, _database: Option<&str>, table: &str) -> DriverResult<u64> {
        let pool = self.pool().await?;
        let count: i64 = sqlx::query_scalar(&format!(
            "SELECT COUNT(*) FROM {}",
            quote_ident(DbKind::Mysql, table)
        ))
        .fetch_one(&pool)
        .await
        .map_err(|e| DriverError::Query(e.to_string()))?;
        Ok(count as u64)
    }

    async fn sessions(&self) -> DriverResult<Vec<SessionInfo>> {
        let pool = self.pool().await?;
        let rows: Vec<(u64, String, Option<String>, String, String, i64, Option<String>)> =
            sqlx::query_as(
                "SELECT id, user, db, command, state, time, info FROM information_schema.processlist",
            )
            .fetch_all(&pool)
            .await
            .map_err(|e| DriverError::Query(e.to_string()))?;
        Ok(rows
            .into_iter()
            .map(|(id, user, db, command, state, time, info)| SessionInfo {
                id: id.to_string(),
                user,
                database: db.unwrap_or_default(),
                state,
                command,
                elapsed_secs: time as f64,
                sql: info.unwrap_or_default(),
            })
            .collect())
    }

    async fn kill_session(&self, id: &str) -> DriverResult<()> {
        let pool = self.pool().await?;
        let id: u64 = id.parse().map_err(|_| DriverError::Query("会话 ID 非法".into()))?;
        sqlx::query(&format!("KILL {id}"))
            .execute(&pool)
            .await
            .map_err(|e| DriverError::Query(e.to_string()))?;
        Ok(())
    }

    async fn create_table_ddl(&self, def: &TableDef, _database: Option<&str>) -> DriverResult<String> {
        let mut cols = Vec::new();
        for c in &def.columns {
            let mut line = format!("  {} {}", quote_ident(DbKind::Mysql, &c.name), c.data_type);
            if !c.nullable {
                line.push_str(" NOT NULL");
            }
            if let Some(d) = &c.default {
                line.push_str(&format!(" DEFAULT {d}"));
            }
            if c.is_pk && def.columns.iter().filter(|x| x.is_pk).count() == 1 {
                line.push_str(" PRIMARY KEY");
            }
            cols.push(line);
        }
        let pks: Vec<String> = def
            .columns
            .iter()
            .filter(|c| c.is_pk)
            .map(|c| quote_ident(DbKind::Mysql, &c.name))
            .collect();
        if pks.len() > 1 {
            cols.push(format!("  PRIMARY KEY ({})", pks.join(", ")));
        }
        for (col, fk) in def.foreign_keys() {
            cols.push(format!(
                "  CONSTRAINT fk_{}_{} FOREIGN KEY ({}) REFERENCES {} ({})",
                def.name,
                col.name,
                quote_ident(DbKind::Mysql, &col.name),
                quote_ident(DbKind::Mysql, &fk.ref_table),
                quote_ident(DbKind::Mysql, &fk.ref_column)
            ));
        }
        Ok(format!(
            "CREATE TABLE {} (\n{}\n) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;",
            quote_ident(DbKind::Mysql, &def.name),
            cols.join(",\n")
        ))
    }
}
