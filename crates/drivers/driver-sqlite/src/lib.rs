//! SQLite 驱动（sqlx）。作为零网络依赖的本地引擎，同时承担首启动演示库。

use async_trait::async_trait;
use dbhelper_models::*;
use rel_common::*;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteRow};
use sqlx::Row as _;
use sqlx::{Column, TypeInfo};
use std::str::FromStr;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};

pub struct SqliteDriver {
    config: ConnectionConfig,
    pool: RwLock<Option<sqlx::SqlitePool>>,
}

impl SqliteDriver {
    pub fn new(config: ConnectionConfig) -> Self {
        Self { config, pool: RwLock::new(None) }
    }

    async fn pool(&self) -> DriverResult<sqlx::SqlitePool> {
        self.pool
            .read()
            .await
            .clone()
            .ok_or(DriverError::NotConnected)
    }

    fn database_label(&self) -> String {
        let path = std::path::Path::new(&self.config.database);
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("main")
            .to_string()
    }
}

fn decode_value(row: &SqliteRow, idx: usize) -> Value {
    // 注意：`row.column(idx).type_info()` 是语句声明的类型，聚合/表达式列
    // （如 COUNT(x)、SUM(x)）常被声明为 NULL，若据此提前返回会丢掉真实值。
    // 这里读原始值，用 sqlite3_value_type 的运行时类型判断。
    use sqlx::{TypeInfo, ValueRef};
    let declared = row.column(idx).type_info().name().to_ascii_uppercase();
    let raw = match row.try_get_raw(idx) {
        Ok(raw) => raw,
        Err(_) => return Value::Null,
    };
    let info = raw.type_info().name().to_ascii_uppercase();
    if matches!(info.as_str(), "NULL") {
        return Value::Null;
    }
    match info.as_str() {
        "INTEGER" | "INT" | "BIGINT" | "SMALLINT" | "TINYINT" => {
            match row.try_get::<Option<i64>, _>(idx) {
                Ok(Some(v)) => Value::Int(v),
                Ok(None) => Value::Null,
                Err(_) => row
                    .try_get::<Option<f64>, _>(idx)
                    .ok()
                    .flatten()
                    .map(Value::Float)
                    .unwrap_or(Value::Null),
            }
        }
        "REAL" | "FLOAT" | "DOUBLE" | "NUMERIC" | "DECIMAL" => {
            match row.try_get::<Option<f64>, _>(idx) {
                Ok(Some(v)) => Value::Float(v),
                Ok(None) => Value::Null,
                Err(_) => Value::Null,
            }
        }
        "BLOB" => row
            .try_get::<Option<Vec<u8>>, _>(idx)
            .ok()
            .flatten()
            .map(Value::Bytes)
            .unwrap_or(Value::Null),
        _ => match row.try_get::<Option<String>, _>(idx) {
            Ok(Some(v)) => {
                if info.contains("DATE") || info.contains("TIME") {
                    Value::Timestamp(v)
                } else {
                    Value::Text(v)
                }
            }
            Ok(None) => Value::Null,
            Err(_) => {
                let _ = &declared;
                Value::Null
            }
        },
    }
}

fn decode_row(row: &SqliteRow) -> Row {
    (0..row.len()).map(|i| decode_value(row, i)).collect()
}

#[async_trait]
impl RelationalDriver for SqliteDriver {
    fn kind(&self) -> DbKind {
        DbKind::Sqlite
    }

    fn config(&self) -> &ConnectionConfig {
        &self.config
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            multi_database: false,
            schemas: false,
            sessions: false,
            kill_session: false,
            explain: true,
            ddl: true,
            procedures: false,
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
        let path = if self.config.database.trim().is_empty() {
            ":memory:".to_string()
        } else {
            self.config.database.clone()
        };
        if path != ":memory:" {
            if let Some(parent) = std::path::Path::new(&path).parent() {
                let _ = std::fs::create_dir_all(parent);
            }
        }
        let options = SqliteConnectOptions::from_str(&path)
            .map_err(|e| DriverError::Connect(e.to_string()))?
            .create_if_missing(true)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5));

        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .acquire_timeout(Duration::from_millis(self.config.connect_timeout_ms))
            .connect_with(options)
            .await
            .map_err(|e| DriverError::Connect(e.to_string()))?;

        sqlx::query("PRAGMA journal_mode = WAL")
            .execute(&pool)
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
        let version: String = sqlx::query_scalar("SELECT sqlite_version()")
            .fetch_one(&pool)
            .await
            .unwrap_or_else(|_| "3.x".into());
        let (pages, page_size): (i64, i64) = sqlx::query_as("PRAGMA page_count")
            .fetch_one(&pool)
            .await
            .map(|(p,): (i64,)| (p, 0))
            .unwrap_or((0, 0));
        let _ = (pages, page_size);
        Ok(ServerInfo {
            version: format!("SQLite {version}"),
            edition: "embedded".into(),
            uptime_secs: None,
            current_database: self.database_label(),
            extra: vec![("journal".into(), "WAL".into())],
        })
    }

    async fn list_databases(&self) -> DriverResult<Vec<String>> {
        Ok(vec![self.database_label()])
    }

    async fn list_objects(
        &self,
        _database: Option<&str>,
        kinds: &[ObjectKind],
    ) -> DriverResult<Vec<SchemaObject>> {
        let pool = self.pool().await?;
        let rows: Vec<(String, String, Option<i64>)> = sqlx::query_as(
            "SELECT name, type, (SELECT COUNT(*) FROM pragma_table_info(m.name)) \
             FROM sqlite_master m WHERE type IN ('table','view','trigger','index') \
             AND name NOT LIKE 'sqlite_%' ORDER BY type, name",
        )
        .fetch_all(&pool)
        .await
        .map_err(|e| DriverError::Query(e.to_string()))?;

        let mut out = Vec::new();
        for (name, kind, _cols) in rows {
            let object_kind = match kind.as_str() {
                "table" => ObjectKind::Table,
                "view" => ObjectKind::View,
                "trigger" => ObjectKind::Trigger,
                "index" => ObjectKind::Index,
                _ => continue,
            };
            if !kinds.is_empty() && !kinds.contains(&object_kind) {
                continue;
            }
            let row_estimate = if object_kind == ObjectKind::Table {
                sqlx::query_scalar::<_, i64>(&format!(
                    "SELECT COUNT(*) FROM \"{}\"",
                    name.replace('"', "\"\"")
                ))
                .fetch_one(&pool)
                .await
                .ok()
                .map(|v| v as u64)
            } else {
                None
            };
            out.push(SchemaObject {
                name,
                kind: object_kind,
                schema: None,
                row_estimate,
                comment: None,
            });
        }
        Ok(out)
    }

    async fn describe_table(&self, _database: Option<&str>, table: &str) -> DriverResult<TableDef> {
        let pool = self.pool().await?;
        let cols: Vec<(i64, String, String, i64, Option<String>, i64)> =
            sqlx::query_as(&format!("PRAGMA table_info(\"{}\")", table.replace('"', "\"\"")))
                .fetch_all(&pool)
                .await
                .map_err(|e| DriverError::Query(e.to_string()))?;

        // PRAGMA foreign_key_list 的列序为：id, seq, 引用表, 本表列, 引用列, on_update...
        let fks: Vec<(i64, i64, String, String, String, String)> =
            sqlx::query_as(&format!("PRAGMA foreign_key_list(\"{}\")", table.replace('"', "\"\"")))
                .fetch_all(&pool)
                .await
                .unwrap_or_default();

        let idx: Vec<(String, i64)> = sqlx::query_as(&format!(
            "SELECT name, \"unique\" FROM pragma_index_list(\"{}\")",
            table.replace('"', "\"\"")
        ))
        .fetch_all(&pool)
        .await
        .unwrap_or_default();

        let mut columns = Vec::new();
        for (cid, name, data_type, notnull, default, pk) in cols {
            let fk = fks
                .iter()
                .find(|(_, _, _, from_col, _, _)| from_col == &name)
                .map(|(id, _, ref_table, _, to_col, _)| ForeignKey {
                    column: name.clone(),
                    ref_table: ref_table.clone(),
                    ref_column: to_col.clone(),
                    constraint: format!("fk_{table}_{id}"),
                });
            columns.push(ColumnDef {
                name,
                data_type: data_type.clone(),
                nullable: notnull == 0,
                default,
                is_pk: pk > 0,
                is_unique: false,
                fk,
                extra: String::new(),
                comment: None,
                ordinal: cid,
            });
        }

        let mut indexes = Vec::new();
        for (name, unique) in idx {
            let info: Vec<(i64, i64, String)> = sqlx::query_as(&format!(
                "SELECT seq, \"unique\", name FROM pragma_index_info('{}')",
                name.replace('\'', "''")
            ))
            .fetch_all(&pool)
            .await
            .unwrap_or_default();
            let mut info = info;
            info.sort_by_key(|(seq, _, _)| *seq);
            indexes.push(IndexDef {
                columns: info.into_iter().map(|(_, _, c)| c).collect(),
                unique: unique != 0,
                primary: false,
                name,
            });
        }

        let row_estimate = sqlx::query_scalar::<_, i64>(&format!(
            "SELECT COUNT(*) FROM \"{}\"",
            table.replace('"', "\"\"")
        ))
        .fetch_one(&pool)
        .await
        .ok()
        .map(|v| v as u64);

        Ok(TableDef {
            schema: None,
            name: table.to_string(),
            columns,
            indexes,
            row_estimate,
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

        let columns: Vec<ColumnMeta> = rows
            .first()
            .map(|r| {
                r.columns()
                    .iter()
                    .map(|c| ColumnMeta::new(c.name(), c.type_info().name()))
                    .collect()
            })
            .unwrap_or_else(|| infer_columns_from_sql(sql));

        let truncated = rows.len() > limit as usize;
        let rows: Vec<Row> = rows
            .iter()
            .take(limit as usize)
            .map(decode_row)
            .collect();

        Ok(QueryOutcome {
            columns,
            rows,
            rows_affected: 0,
            elapsed_ms: start.elapsed().as_millis() as u64,
            truncated,
            limit,
            notices: Vec::new(),
            read_only: is_query(sql),
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
            let fut = async {
                let mut rows = sqlx::query(&sql).fetch(&pool);
                let mut columns_sent = false;
                let mut batch: Vec<Row> = Vec::with_capacity(200);
                let mut total = 0usize;
                let mut truncated = false;

                use futures::StreamExt;
                while let Some(item) = rows.next().await {
                    match item {
                        Ok(row) => {
                            if !columns_sent {
                                let cols: Vec<ColumnMeta> = row
                                    .columns()
                                    .iter()
                                    .enumerate()
                                    .map(|(i, c)| {
                                        let declared = c.type_info().name().to_string();
                                        let actual = row
                                            .try_get_raw(i)
                                            .map(|v| {
                                                use sqlx::{TypeInfo, ValueRef};
                                                v.type_info().name().to_string()
                                            })
                                            .unwrap_or_else(|_| declared.clone());
                                        if actual.eq_ignore_ascii_case("NULL") {
                                            ColumnMeta::new(c.name(), declared)
                                        } else {
                                            ColumnMeta::new(c.name(), actual)
                                        }
                                    })
                                    .collect();
                                let _ = tx
                                    .send(StreamEvent::Columns { columns: cols, read_only: is_query(&sql) })
                                    .await;
                                columns_sent = true;
                            }
                            batch.push(decode_row(&row));
                            total += 1;
                            if batch.len() >= 200 {
                                if tx.send(StreamEvent::Batch(std::mem::take(&mut batch))).await.is_err() {
                                    return Err(());
                                }
                            }
                            if total >= limit as usize {
                                truncated = true;
                                break;
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(StreamEvent::Failed(e.to_string())).await;
                            return Err(());
                        }
                    }
                }
                if !batch.is_empty() {
                    let _ = tx.send(StreamEvent::Batch(batch)).await;
                }
                if !columns_sent {
                    let _ = tx
                        .send(StreamEvent::Columns { columns: infer_columns_from_sql(&sql), read_only: is_query(&sql) })
                        .await;
                }
                let _ = tx
                    .send(StreamEvent::Done {
                        elapsed_ms: start.elapsed().as_millis() as u64,
                        rows_affected: 0,
                        truncated,
                    })
                    .await;
                Ok::<(), ()>(())
            };

            if tokio::time::timeout(timeout, fut).await.is_err() {
                let _ = tx
                    .send(StreamEvent::Failed(format!(
                        "执行超时（{} ms），已取消",
                        timeout.as_millis()
                    )))
                    .await;
            }
        });
        Ok(())
    }

    async fn execute(&self, sql: &str, timeout: Duration) -> DriverResult<ExecOutcome> {
        let pool = self.pool().await?;
        let start = Instant::now();
        let fut = sqlx::query(sql).execute(&pool);
        let result = tokio::time::timeout(timeout, fut)
            .await
            .map_err(|_| DriverError::Timeout(timeout.as_millis() as u64))?
            .map_err(|e| DriverError::Query(e.to_string()))?;
        Ok(ExecOutcome {
            rows_affected: result.rows_affected(),
            elapsed_ms: start.elapsed().as_millis() as u64,
            last_insert_id: Some(result.last_insert_rowid()),
            notices: Vec::new(),
        })
    }

    async fn explain(&self, sql: &str) -> DriverResult<QueryOutcome> {
        self.query(&format!("EXPLAIN QUERY PLAN {sql}"), 200, Duration::from_secs(10)).await
    }

    async fn ddl(&self, _database: Option<&str>, table: &str) -> DriverResult<String> {
        let pool = self.pool().await?;
        let sql: Option<String> =
            sqlx::query_scalar("SELECT sql FROM sqlite_master WHERE name = ? OR tbl_name = ? LIMIT 1")
                .bind(table)
                .bind(table)
                .fetch_optional(&pool)
                .await
                .map_err(|e| DriverError::Query(e.to_string()))?;
        sql.map(|s| format!("{s};"))
            .ok_or_else(|| DriverError::Query(format!("未找到对象 {table}")))
    }

    async fn estimate_affected(&self, sql: &str) -> DriverResult<Option<u64>> {
        let Some(count_sql) = estimate_sql(DbKind::Sqlite, sql) else {
            return Ok(None);
        };
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
            "SELECT COUNT(*) FROM \"{}\"",
            table.replace('"', "\"\"")
        ))
        .fetch_one(&pool)
        .await
        .map_err(|e| DriverError::Query(e.to_string()))?;
        Ok(count as u64)
    }

    fn writer(&self) -> Option<&dyn RowWriter> {
        Some(self)
    }

    async fn create_table_ddl(&self, def: &TableDef, _database: Option<&str>) -> DriverResult<String> {
        let mut cols = Vec::new();
        for c in &def.columns {
            let mut line = format!("  \"{}\" {}", c.name, c.data_type);
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
            .map(|c| format!("\"{}\"", c.name))
            .collect();
        if !pks.is_empty() {
            cols.push(format!("  PRIMARY KEY ({})", pks.join(", ")));
        }
        Ok(format!(
            "CREATE TABLE \"{}\" (\n{}\n);",
            def.name,
            cols.join(",\n")
        ))
    }
}

fn is_query(sql: &str) -> bool {
    let first = sql
        .trim_start_matches(|c: char| c.is_whitespace() || c == '-' || c == '/' || c == '*')
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_uppercase();
    matches!(first.as_str(), "SELECT" | "WITH" | "EXPLAIN" | "PRAGMA" | "SHOW")
}

fn infer_columns_from_sql(sql: &str) -> Vec<ColumnMeta> {
    let upper = sql.to_uppercase();
    if let Some(pos) = upper.find("SELECT") {
        let rest = &sql[pos + 6..];
        let end = rest.to_uppercase().find(" FROM").unwrap_or(rest.len());
        let list = &rest[..end];
        let cols: Vec<ColumnMeta> = list
            .split(',')
            .filter_map(|part| {
                let raw = part.trim();
                if raw.is_empty() || raw.chars().all(|c| c.is_ascii_digit()) {
                    return None;
                }
                let last = raw.split_whitespace().last().unwrap_or(raw);
                let name = last
                    .rsplit('.')
                    .next()
                    .unwrap_or(last)
                    .trim_matches(|c| c == '"' || c == '`' || c == ';')
                    .to_string();
                if name.is_empty() {
                    None
                } else {
                    Some(ColumnMeta::new(name, "text"))
                }
            })
            .collect();
        if !cols.is_empty() {
            return cols;
        }
    }
    Vec::new()
}

// ---------------------------------------------------------------------------
// 参数化写入（行内编辑 / CSV 导入）
// ---------------------------------------------------------------------------

#[async_trait]
impl RowWriter for SqliteDriver {
    async fn update_row(
        &self,
        table: &str,
        pk: &[(String, Value)],
        changes: &[(String, Value)],
    ) -> DriverResult<u64> {
        if changes.is_empty() {
            return Ok(0);
        }
        let pool = self.pool().await?;
        let mut qb = sqlx::QueryBuilder::<sqlx::Sqlite>::new("UPDATE ");
        qb.push(format!("\"{}\" SET ", table.replace('"', "\"\"")));
        for (i, (col, value)) in changes.iter().enumerate() {
            if i > 0 {
                qb.push(", ");
            }
            qb.push(format!("\"{}\" = ", col.replace('"', "\"\"")));
            push_bind(&mut qb, value);
        }
        qb.push(" WHERE ");
        for (i, (col, value)) in pk.iter().enumerate() {
            if i > 0 {
                qb.push(" AND ");
            }
            qb.push(format!("\"{}\" = ", col.replace('"', "\"\"")));
            push_bind(&mut qb, value);
        }
        let res = qb
            .build()
            .execute(&pool)
            .await
            .map_err(|e| DriverError::Query(e.to_string()))?;
        Ok(res.rows_affected())
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
        let pool = self.pool().await?;
        let mut qb = sqlx::QueryBuilder::<sqlx::Sqlite>::new("INSERT INTO ");
        qb.push(format!("\"{}\" (", table.replace('"', "\"\"")));
        for (i, col) in columns.iter().enumerate() {
            if i > 0 {
                qb.push(", ");
            }
            qb.push(format!("\"{}\"", col.replace('"', "\"\"")));
        }
        qb.push(") VALUES ");
        for (ri, row) in rows.iter().enumerate() {
            if ri > 0 {
                qb.push(", ");
            }
            qb.push("(");
            for (ci, value) in row.iter().enumerate() {
                if ci > 0 {
                    qb.push(", ");
                }
                push_bind(&mut qb, value);
            }
            qb.push(")");
        }
        let res = qb
            .build()
            .execute(&pool)
            .await
            .map_err(|e| DriverError::Query(e.to_string()))?;
        Ok(res.rows_affected())
    }
}

fn push_bind<'a>(qb: &mut sqlx::QueryBuilder<'a, sqlx::Sqlite>, value: &'a Value) {
    match value {
        Value::Null => {
            qb.push_bind(Option::<String>::None);
        }
        Value::Int(i) => {
            qb.push_bind(*i);
        }
        Value::Float(f) => {
            qb.push_bind(*f);
        }
        Value::Decimal(s) | Value::Text(s) | Value::Timestamp(s) | Value::Json(s) => {
            qb.push_bind(s.clone());
        }
        Value::Bool(b) => {
            qb.push_bind(*b);
        }
        Value::Bytes(b) => {
            qb.push_bind(b.clone());
        }
    }
}
