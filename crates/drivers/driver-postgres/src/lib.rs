//! PostgreSQL 驱动（sqlx / postgres feature）。

use async_trait::async_trait;
use dbhelper_models::*;
use futures::StreamExt;
use rel_common::*;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions, PgRow, PgSslMode};
use sqlx::Row as _;
use sqlx::{Column, TypeInfo};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};

pub struct PostgresDriver {
    config: ConnectionConfig,
    pool: RwLock<Option<sqlx::PgPool>>,
    password: String,
}

impl PostgresDriver {
    pub fn new(config: ConnectionConfig) -> Self {
        Self { config, pool: RwLock::new(None), password: String::new() }
    }

    pub fn set_password(&mut self, password: String) {
        self.password = password;
    }

    async fn pool(&self) -> DriverResult<sqlx::PgPool> {
        self.pool.read().await.clone().ok_or(DriverError::NotConnected)
    }
}

fn ssl_mode(mode: SslMode) -> PgSslMode {
    match mode {
        SslMode::Disabled => PgSslMode::Disable,
        SslMode::Preferred => PgSslMode::Prefer,
        SslMode::Required => PgSslMode::Require,
        SslMode::VerifyCa => PgSslMode::VerifyCa,
        SslMode::VerifyFull => PgSslMode::VerifyFull,
    }
}

fn decode_value(row: &PgRow, idx: usize) -> Value {
    let info = row.column(idx).type_info().name().to_ascii_uppercase();
    match info.as_str() {
        "INT2" | "INT4" | "INT8" | "OID" => row
            .try_get::<Option<i64>, _>(idx)
            .ok()
            .flatten()
            .map(Value::Int)
            .unwrap_or_else(|| {
                row.try_get::<Option<i32>, _>(idx)
                    .ok()
                    .flatten()
                    .map(|v| Value::Int(v as i64))
                    .unwrap_or(Value::Null)
            }),
        "FLOAT4" | "FLOAT8" => row
            .try_get::<Option<f64>, _>(idx)
            .ok()
            .flatten()
            .map(Value::Float)
            .unwrap_or(Value::Null),
        "NUMERIC" | "MONEY" => row
            .try_get::<Option<sqlx::types::BigDecimal>, _>(idx)
            .ok()
            .flatten()
            .map(|v| Value::Decimal(v.to_string()))
            .unwrap_or(Value::Null),
        "BOOL" => row
            .try_get::<Option<bool>, _>(idx)
            .ok()
            .flatten()
            .map(Value::Bool)
            .unwrap_or(Value::Null),
        "BYTEA" => row
            .try_get::<Option<Vec<u8>>, _>(idx)
            .ok()
            .flatten()
            .map(Value::Bytes)
            .unwrap_or(Value::Null),
        "DATE" => row
            .try_get::<Option<chrono::NaiveDate>, _>(idx)
            .ok()
            .flatten()
            .map(|v| Value::Timestamp(v.format("%Y-%m-%d").to_string()))
            .unwrap_or(Value::Null),
        "TIMESTAMP" => row
            .try_get::<Option<chrono::NaiveDateTime>, _>(idx)
            .ok()
            .flatten()
            .map(|v| Value::Timestamp(v.format("%Y-%m-%d %H:%M:%S").to_string()))
            .unwrap_or(Value::Null),
        "TIMESTAMPTZ" => row
            .try_get::<Option<chrono::DateTime<chrono::Utc>>, _>(idx)
            .ok()
            .flatten()
            .map(|v| Value::Timestamp(v.format("%Y-%m-%d %H:%M:%S%:z").to_string()))
            .unwrap_or(Value::Null),
        "TIME" => row
            .try_get::<Option<chrono::NaiveTime>, _>(idx)
            .ok()
            .flatten()
            .map(|v| Value::Text(v.format("%H:%M:%S").to_string()))
            .unwrap_or(Value::Null),
        "JSON" | "JSONB" => row
            .try_get::<Option<serde_json::Value>, _>(idx)
            .ok()
            .flatten()
            .map(|v| Value::Json(v.to_string()))
            .unwrap_or(Value::Null),
        _ => row
            .try_get::<Option<String>, _>(idx)
            .ok()
            .flatten()
            .map(Value::Text)
            .unwrap_or(Value::Null),
    }
}

fn decode_row(row: &PgRow) -> Row {
    (0..row.len()).map(|i| decode_value(row, i)).collect()
}

fn columns_of(row: &PgRow) -> Vec<ColumnMeta> {
    row.columns()
        .iter()
        .map(|c| ColumnMeta::new(c.name(), c.type_info().name()))
        .collect()
}

#[async_trait]
impl RelationalDriver for PostgresDriver {
    fn kind(&self) -> DbKind {
        DbKind::Postgres
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
        self.pool.try_read().map(|p| p.is_some()).unwrap_or(false)
    }

    async fn connect(&self) -> DriverResult<()> {
        if self.is_connected() {
            return Ok(());
        }
        let mut opts = PgConnectOptions::new()
            .host(&self.config.host)
            .port(self.config.port)
            .username(&self.config.username)
            .database(if self.config.database.is_empty() { "postgres" } else { &self.config.database })
            .ssl_mode(ssl_mode(self.config.ssl))
            .application_name("DBHelper");
        if !self.password.is_empty() {
            opts = opts.password(&self.password);
        }
        let pool = PgPoolOptions::new()
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
        let version: String = sqlx::query_scalar("SELECT version()")
            .fetch_one(&pool)
            .await
            .unwrap_or_else(|_| "PostgreSQL".into());
        let db: String = sqlx::query_scalar("SELECT current_database()")
            .fetch_one(&pool)
            .await
            .unwrap_or_else(|_| self.config.database.clone());
        Ok(ServerInfo {
            version: version.split(',').next().unwrap_or("PostgreSQL").to_string(),
            edition: "PostgreSQL".into(),
            uptime_secs: None,
            current_database: db,
            extra: Vec::new(),
        })
    }

    async fn list_databases(&self) -> DriverResult<Vec<String>> {
        let pool = self.pool().await?;
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT datname FROM pg_database WHERE NOT datistemplate ORDER BY datname",
        )
        .fetch_all(&pool)
        .await
        .map_err(|e| DriverError::Query(e.to_string()))?;
        Ok(rows.into_iter().map(|(n,)| n).collect())
    }

    async fn list_objects(
        &self,
        _database: Option<&str>,
        kinds: &[ObjectKind],
    ) -> DriverResult<Vec<SchemaObject>> {
        let pool = self.pool().await?;
        let mut out = Vec::new();
        if kinds.is_empty() || kinds.contains(&ObjectKind::Table) || kinds.contains(&ObjectKind::View) {
            let rows: Vec<(String, String, String, f32, Option<String>)> = sqlx::query_as(
                "SELECT n.nspname, c.relname, c.relkind::text, c.reltuples, obj_description(c.oid) \
                 FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
                 WHERE c.relkind IN ('r','p','v','m','f') AND n.nspname NOT IN ('pg_catalog','information_schema') \
                 AND n.nspname NOT LIKE 'pg_toast%' ORDER BY n.nspname, c.relname",
            )
            .fetch_all(&pool)
            .await
            .map_err(|e| DriverError::Query(e.to_string()))?;
            for (schema, name, relkind, rows_est, comment) in rows {
                out.push(SchemaObject {
                    name,
                    kind: if relkind == "v" || relkind == "m" { ObjectKind::View } else { ObjectKind::Table },
                    schema: Some(schema),
                    row_estimate: if rows_est >= 0.0 { Some(rows_est as u64) } else { None },
                    comment,
                });
            }
        }
        if kinds.is_empty() || kinds.contains(&ObjectKind::Function) || kinds.contains(&ObjectKind::Procedure) {
            let rows: Vec<(String, String, String)> = sqlx::query_as(
                "SELECT n.nspname, p.proname, p.prokind::text FROM pg_proc p \
                 JOIN pg_namespace n ON n.oid = p.pronamespace \
                 WHERE n.nspname NOT IN ('pg_catalog','information_schema') ORDER BY p.proname",
            )
            .fetch_all(&pool)
            .await
            .unwrap_or_default();
            for (schema, name, kind) in rows {
                out.push(SchemaObject {
                    kind: if kind == "p" { ObjectKind::Procedure } else { ObjectKind::Function },
                    name,
                    schema: Some(schema),
                    row_estimate: None,
                    comment: None,
                });
            }
        }
        Ok(out)
    }

    async fn describe_table(&self, _database: Option<&str>, table: &str) -> DriverResult<TableDef> {
        let pool = self.pool().await?;
        let (schema, name) = match table.split_once('.') {
            Some((s, t)) => (s.to_string(), t.to_string()),
            None => ("public".to_string(), table.to_string()),
        };

        let cols: Vec<(String, String, String, Option<String>, i32)> = sqlx::query_as(
            "SELECT column_name, data_type, is_nullable, column_default, \
                CASE WHEN column_name = ANY(COALESCE(( \
                    SELECT array_agg(kcu.column_name) FROM information_schema.table_constraints tc \
                    JOIN information_schema.key_column_usage kcu ON kcu.constraint_name = tc.constraint_name \
                    WHERE tc.table_schema = $1 AND tc.table_name = $2 AND tc.constraint_type = 'PRIMARY KEY' \
                ), ARRAY[]::text[])) THEN 1 ELSE 0 END \
             FROM information_schema.columns WHERE table_schema = $1 AND table_name = $2 \
             ORDER BY ordinal_position",
        )
        .bind(&schema)
        .bind(&name)
        .fetch_all(&pool)
        .await
        .map_err(|e| DriverError::Query(e.to_string()))?;

        let fks: Vec<(String, String, String, String)> = sqlx::query_as(
            "SELECT kcu.column_name, ccu.table_name, ccu.column_name, tc.constraint_name \
             FROM information_schema.table_constraints tc \
             JOIN information_schema.key_column_usage kcu ON kcu.constraint_name = tc.constraint_name \
             JOIN information_schema.constraint_column_usage ccu ON ccu.constraint_name = tc.constraint_name \
             WHERE tc.constraint_type = 'FOREIGN KEY' AND tc.table_schema = $1 AND tc.table_name = $2",
        )
        .bind(&schema)
        .bind(&name)
        .fetch_all(&pool)
        .await
        .unwrap_or_default();

        let uniques: Vec<(String,)> = sqlx::query_as(
            "SELECT kcu.column_name FROM information_schema.table_constraints tc \
             JOIN information_schema.key_column_usage kcu ON kcu.constraint_name = tc.constraint_name \
             WHERE tc.constraint_type = 'UNIQUE' AND tc.table_schema = $1 AND tc.table_name = $2",
        )
        .bind(&schema)
        .bind(&name)
        .fetch_all(&pool)
        .await
        .unwrap_or_default();

        let indexes: Vec<(String, bool, String)> = sqlx::query_as(
            "SELECT i.relname, ix.indisunique, a.attname \
             FROM pg_index ix JOIN pg_class i ON i.oid = ix.indexrelid \
             JOIN pg_class t ON t.oid = ix.indrelid \
             JOIN pg_namespace n ON n.oid = t.relnamespace \
             JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = ANY(ix.indkey) \
             WHERE t.relname = $1 AND n.nspname = $2",
        )
        .bind(&name)
        .bind(&schema)
        .fetch_all(&pool)
        .await
        .unwrap_or_default();

        let mut columns = Vec::new();
        for (i, (col, dtype, nullable, default, is_pk)) in cols.into_iter().enumerate() {
            let fk = fks.iter().find(|(c, _, _, _)| c == &col).map(|(c, t, rc, cn)| ForeignKey {
                column: c.clone(),
                ref_table: t.clone(),
                ref_column: rc.clone(),
                constraint: cn.clone(),
            });
            columns.push(ColumnDef {
                name: col.clone(),
                data_type: dtype,
                nullable: nullable.eq_ignore_ascii_case("YES"),
                default,
                is_pk: is_pk == 1,
                is_unique: uniques.iter().any(|(u,)| u == &col),
                fk,
                extra: String::new(),
                comment: None,
                ordinal: i as i64,
            });
        }

        let mut idx_map: Vec<IndexDef> = Vec::new();
        for (iname, uniq, col) in indexes {
            match idx_map.iter_mut().find(|i| i.name == iname) {
                Some(e) => e.columns.push(col),
                None => idx_map.push(IndexDef { name: iname, unique: uniq, primary: false, columns: vec![col] }),
            }
        }

        Ok(TableDef {
            schema: Some(schema),
            name,
            columns,
            indexes: idx_map,
            row_estimate: None,
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
            read_only: is_read_only_sql(sql),
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
                                        read_only: is_read_only_sql(&sql),
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
            last_insert_id: None,
            notices: Vec::new(),
        })
    }

    async fn explain(&self, sql: &str) -> DriverResult<QueryOutcome> {
        self.query(&format!("EXPLAIN (ANALYZE false, FORMAT TEXT) {sql}"), 500, Duration::from_secs(15))
            .await
    }

    async fn ddl(&self, database: Option<&str>, table: &str) -> DriverResult<String> {
        let def = self.describe_table(database, table).await?;
        self.create_table_ddl(&def, database).await
    }

    async fn estimate_affected(&self, sql: &str) -> DriverResult<Option<u64>> {
        let Some(count_sql) = estimate_sql(DbKind::Postgres, sql) else { return Ok(None) };
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
            quote_ident(DbKind::Postgres, table)
        ))
        .fetch_one(&pool)
        .await
        .map_err(|e| DriverError::Query(e.to_string()))?;
        Ok(count as u64)
    }

    async fn sessions(&self) -> DriverResult<Vec<SessionInfo>> {
        let pool = self.pool().await?;
        let rows: Vec<(i32, String, String, String, String, f64, String)> = sqlx::query_as(
            "SELECT pid, usename, datname, state, query, \
                EXTRACT(EPOCH FROM (now() - query_start))::float8, query \
             FROM pg_stat_activity WHERE pid <> pg_backend_pid()",
        )
        .fetch_all(&pool)
        .await
        .map_err(|e| DriverError::Query(e.to_string()))?;
        Ok(rows
            .into_iter()
            .map(|(pid, user, db, state, _q, elapsed, sql)| SessionInfo {
                id: pid.to_string(),
                user,
                database: db,
                state: state.clone(),
                command: state,
                elapsed_secs: elapsed,
                sql,
            })
            .collect())
    }

    async fn kill_session(&self, id: &str) -> DriverResult<()> {
        let pool = self.pool().await?;
        let pid: i32 = id.parse().map_err(|_| DriverError::Query("会话 ID 非法".into()))?;
        sqlx::query("SELECT pg_terminate_backend($1)")
            .bind(pid)
            .execute(&pool)
            .await
            .map_err(|e| DriverError::Query(e.to_string()))?;
        Ok(())
    }

    async fn slow_queries(&self, limit: usize) -> DriverResult<Vec<SlowQuery>> {
        let pool = self.pool().await?;
        let rows: Vec<(f64, String)> = sqlx::query_as(
            "SELECT EXTRACT(EPOCH FROM (now() - query_start))::float8, query FROM pg_stat_activity \
             WHERE state = 'active' AND pid <> pg_backend_pid() ORDER BY 1 DESC LIMIT $1",
        )
        .bind(limit as i64)
        .fetch_all(&pool)
        .await
        .unwrap_or_default();
        Ok(rows
            .into_iter()
            .map(|(secs, sql)| SlowQuery { elapsed_secs: secs, session_id: String::new(), sql })
            .collect())
    }

    async fn create_table_ddl(&self, def: &TableDef, _database: Option<&str>) -> DriverResult<String> {
        let mut cols = Vec::new();
        for c in &def.columns {
            let mut line = format!("  {} {}", quote_ident(DbKind::Postgres, &c.name), c.data_type);
            if !c.nullable {
                line.push_str(" NOT NULL");
            }
            if let Some(d) = &c.default {
                line.push_str(&format!(" DEFAULT {d}"));
            }
            cols.push(line);
        }
        let pks: Vec<String> = def
            .columns
            .iter()
            .filter(|c| c.is_pk)
            .map(|c| quote_ident(DbKind::Postgres, &c.name))
            .collect();
        if !pks.is_empty() {
            cols.push(format!("  PRIMARY KEY ({})", pks.join(", ")));
        }
        for (col, fk) in def.foreign_keys() {
            cols.push(format!(
                "  FOREIGN KEY ({}) REFERENCES {} ({})",
                quote_ident(DbKind::Postgres, &col.name),
                quote_ident(DbKind::Postgres, &fk.ref_table),
                quote_ident(DbKind::Postgres, &fk.ref_column)
            ));
        }
        Ok(format!(
            "CREATE TABLE {} (\n{}\n);",
            quote_ident(DbKind::Postgres, &def.name),
            cols.join(",\n")
        ))
    }
}
