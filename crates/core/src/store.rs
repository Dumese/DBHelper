//! 应用本地库（`%APPDATA%\DBHelper\dbhelper.db`）。
//!
//! 保存连接配置（不含密码）、分组、设置、查询历史、已保存查询、代码片段与调度任务。
//! 密码统一走 [`dbhelper_utils::Vault`]，本库只保存 Vault 键名。

use dbhelper_models::*;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use std::path::Path;
use std::str::FromStr;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("本地库错误: {0}")]
    Db(#[from] sqlx::Error),
    #[error("序列化错误: {0}")]
    Json(#[from] serde_json::Error),
}

pub type StoreResult<T> = Result<T, StoreError>;

pub struct Store {
    pool: SqlitePool,
    next_id: std::sync::atomic::AtomicI64,
}

impl Store {
    pub async fn open(path: &Path) -> StoreResult<Self> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let opts = SqliteConnectOptions::from_str(&path.to_string_lossy())
            .map_err(|e| sqlx::Error::Configuration(Box::new(e)))?
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(2)
            .connect_with(opts)
            .await?;

        let store = Self { pool, next_id: std::sync::atomic::AtomicI64::new(1) };
        store.migrate().await?;
        Ok(store)
    }

    async fn migrate(&self) -> StoreResult<()> {
        let ddl = [
            "CREATE TABLE IF NOT EXISTS meta (k TEXT PRIMARY KEY, v TEXT NOT NULL)",
            "CREATE TABLE IF NOT EXISTS connections (id INTEGER PRIMARY KEY, json TEXT NOT NULL, sort INTEGER NOT NULL DEFAULT 0)",
            "CREATE TABLE IF NOT EXISTS groups (name TEXT PRIMARY KEY, sort INTEGER NOT NULL DEFAULT 0)",
            "CREATE TABLE IF NOT EXISTS history (id INTEGER PRIMARY KEY AUTOINCREMENT, conn_id INTEGER NOT NULL, conn_name TEXT NOT NULL, sql TEXT NOT NULL, executed_at INTEGER NOT NULL, elapsed_ms INTEGER NOT NULL, rows INTEGER NOT NULL, ok INTEGER NOT NULL, error TEXT)",
            "CREATE TABLE IF NOT EXISTS saved_queries (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, sql TEXT NOT NULL, conn_id INTEGER, created_at INTEGER NOT NULL)",
            "CREATE TABLE IF NOT EXISTS snippets (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, body TEXT NOT NULL)",
            "CREATE TABLE IF NOT EXISTS schedules (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, conn_id INTEGER NOT NULL, sql TEXT NOT NULL, interval_secs INTEGER NOT NULL, enabled INTEGER NOT NULL, last_run INTEGER, last_result TEXT)",
        ];
        for stmt in ddl {
            sqlx::query(stmt).execute(&self.pool).await?;
        }
        Ok(())
    }

    pub async fn meta_get(&self, key: &str) -> StoreResult<Option<String>> {
        let row: Option<(String,)> = sqlx::query_as("SELECT v FROM meta WHERE k = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| r.0))
    }

    pub async fn meta_set(&self, key: &str, value: &str) -> StoreResult<()> {
        sqlx::query("INSERT INTO meta(k, v) VALUES(?, ?) ON CONFLICT(k) DO UPDATE SET v = excluded.v")
            .bind(key)
            .bind(value)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // ---- 连接 ----

    pub async fn load_connections(&self) -> StoreResult<Vec<ConnectionConfig>> {
        let rows: Vec<(i64, String, i64)> =
            sqlx::query_as("SELECT id, json, sort FROM connections ORDER BY sort, id")
                .fetch_all(&self.pool)
                .await?;
        let mut out = Vec::new();
        for (id, json, _) in rows {
            match serde_json::from_str::<ConnectionConfig>(&json) {
                Ok(mut cfg) => {
                    if cfg.id == 0 {
                        cfg.id = id;
                    }
                    out.push(cfg);
                }
                Err(e) => tracing::warn!("连接配置解析失败 id={id}: {e}"),
            }
        }
        Ok(out)
    }

    pub async fn save_connection(&self, cfg: &ConnectionConfig) -> StoreResult<()> {
        let json = serde_json::to_string(cfg)?;
        sqlx::query(
            "INSERT INTO connections(id, json, sort) VALUES(?, ?, 0) \
             ON CONFLICT(id) DO UPDATE SET json = excluded.json",
        )
        .bind(cfg.id)
        .bind(json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_connection(&self, id: ConnId) -> StoreResult<()> {
        sqlx::query("DELETE FROM connections WHERE id = ?").bind(id).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn next_connection_id(&self) -> ConnId {
        let max: Option<(i64,)> = sqlx::query_as("SELECT MAX(id) FROM connections")
            .fetch_optional(&self.pool)
            .await
            .ok()
            .flatten();
        // 取「数据库已存最大 id」与「内存自增计数器」的较大者 +1，避免并发/重启后撞 id。
        let db_max = max.map(|m| m.0).unwrap_or(0);
        let stored = self.next_id.load(std::sync::atomic::Ordering::Relaxed);
        let id = std::cmp::max(db_max, stored) + 1;
        self.next_id.store(id + 1, std::sync::atomic::Ordering::Relaxed);
        id
    }

    // ---- 分组 ----

    pub async fn load_groups(&self) -> StoreResult<Vec<String>> {
        let rows: Vec<(String,)> = sqlx::query_as("SELECT name FROM groups ORDER BY sort, name")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    pub async fn ensure_group(&self, name: &str) -> StoreResult<()> {
        sqlx::query("INSERT OR IGNORE INTO groups(name, sort) VALUES(?, 0)")
            .bind(name)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // ---- 历史 ----

    pub async fn push_history(&self, entry: &HistoryEntry) -> StoreResult<i64> {
        let res = sqlx::query(
            "INSERT INTO history(conn_id, conn_name, sql, executed_at, elapsed_ms, rows, ok, error) \
             VALUES(?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(entry.conn_id)
        .bind(&entry.conn_name)
        .bind(&entry.sql)
        .bind(entry.executed_at)
        .bind(entry.elapsed_ms as i64)
        .bind(entry.rows as i64)
        .bind(entry.ok as i64)
        .bind(&entry.error)
        .execute(&self.pool)
        .await?;
        Ok(res.last_insert_rowid())
    }

    pub async fn history(&self, limit: i64) -> StoreResult<Vec<HistoryEntry>> {
        let rows = sqlx::query(
            "SELECT id, conn_id, conn_name, sql, executed_at, elapsed_ms, rows, ok, error \
             FROM history ORDER BY id DESC LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| HistoryEntry {
                id: r.get("id"),
                conn_id: r.get("conn_id"),
                conn_name: r.get("conn_name"),
                sql: r.get("sql"),
                executed_at: r.get("executed_at"),
                elapsed_ms: r.get::<i64, _>("elapsed_ms") as u64,
                rows: r.get::<i64, _>("rows") as u64,
                ok: r.get::<i64, _>("ok") != 0,
                error: r.get("error"),
            })
            .collect())
    }

    pub async fn clear_history(&self) -> StoreResult<()> {
        sqlx::query("DELETE FROM history").execute(&self.pool).await?;
        Ok(())
    }

    // ---- 已保存查询 ----

    pub async fn save_query(&self, name: &str, sql: &str, conn_id: Option<ConnId>) -> StoreResult<i64> {
        let res = sqlx::query(
            "INSERT INTO saved_queries(name, sql, conn_id, created_at) VALUES(?, ?, ?, ?)",
        )
        .bind(name)
        .bind(sql)
        .bind(conn_id)
        .bind(dbhelper_utils::textfmt::now_secs())
        .execute(&self.pool)
        .await?;
        Ok(res.last_insert_rowid())
    }

    pub async fn saved_queries(&self) -> StoreResult<Vec<SavedQuery>> {
        let rows = sqlx::query("SELECT id, name, sql, conn_id, created_at FROM saved_queries ORDER BY id DESC")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| SavedQuery {
                id: r.get("id"),
                name: r.get("name"),
                sql: r.get("sql"),
                conn_id: r.get("conn_id"),
                created_at: r.get("created_at"),
            })
            .collect())
    }

    pub async fn delete_saved_query(&self, id: i64) -> StoreResult<()> {
        sqlx::query("DELETE FROM saved_queries WHERE id = ?").bind(id).execute(&self.pool).await?;
        Ok(())
    }

    // ---- 片段 ----

    pub async fn snippets(&self) -> StoreResult<Vec<Snippet>> {
        let rows = sqlx::query("SELECT id, name, body FROM snippets ORDER BY name")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| Snippet { id: r.get("id"), name: r.get("name"), body: r.get("body") })
            .collect())
    }

    pub async fn add_snippet(&self, name: &str, body: &str) -> StoreResult<()> {
        sqlx::query("INSERT INTO snippets(name, body) VALUES(?, ?)")
            .bind(name)
            .bind(body)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // ---- 调度 ----

    pub async fn schedules(&self) -> StoreResult<Vec<ScheduleTask>> {
        let rows = sqlx::query(
            "SELECT id, name, conn_id, sql, interval_secs, enabled, last_run, last_result FROM schedules ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| ScheduleTask {
                id: r.get("id"),
                name: r.get("name"),
                conn_id: r.get("conn_id"),
                sql: r.get("sql"),
                interval_secs: r.get::<i64, _>("interval_secs") as u64,
                enabled: r.get::<i64, _>("enabled") != 0,
                last_run: r.get("last_run"),
                last_result: r.get("last_result"),
            })
            .collect())
    }

    pub async fn save_schedule(&self, task: &ScheduleTask) -> StoreResult<i64> {
        if task.id == 0 {
            let res = sqlx::query(
                "INSERT INTO schedules(name, conn_id, sql, interval_secs, enabled, last_run, last_result) \
                 VALUES(?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&task.name)
            .bind(task.conn_id)
            .bind(&task.sql)
            .bind(task.interval_secs as i64)
            .bind(task.enabled as i64)
            .bind(task.last_run)
            .bind(&task.last_result)
            .execute(&self.pool)
            .await?;
            Ok(res.last_insert_rowid())
        } else {
            sqlx::query(
                "UPDATE schedules SET name = ?, conn_id = ?, sql = ?, interval_secs = ?, enabled = ?, \
                 last_run = ?, last_result = ? WHERE id = ?",
            )
            .bind(&task.name)
            .bind(task.conn_id)
            .bind(&task.sql)
            .bind(task.interval_secs as i64)
            .bind(task.enabled as i64)
            .bind(task.last_run)
            .bind(&task.last_result)
            .bind(task.id)
            .execute(&self.pool)
            .await?;
            Ok(task.id)
        }
    }

    pub async fn delete_schedule(&self, id: i64) -> StoreResult<()> {
        sqlx::query("DELETE FROM schedules WHERE id = ?").bind(id).execute(&self.pool).await?;
        Ok(())
    }
}
