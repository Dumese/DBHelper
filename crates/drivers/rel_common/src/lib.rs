//! 关系型数据库驱动的统一抽象。
//!
//! 五个关系型驱动（SQLite / MySQL / PostgreSQL / SQL Server / Oracle）
//! 全部实现本文件的 [`RelationalDriver`]，由 core 层通过枚举分派，
//! 新增数据库只需新增 crate 与一个枚举分支，不改核心逻辑。

use async_trait::async_trait;
use dbhelper_models::{
    ColumnMeta, ConnectionConfig, DbKind, ExecOutcome, ObjectKind, QueryOutcome, SchemaObject,
    ServerInfo, SessionInfo, SlowQuery, StreamEvent, TableDef,
};
use std::time::Duration;
use thiserror::Error;
use tokio::sync::mpsc;

#[derive(Debug, Error)]
pub enum DriverError {
    #[error("连接失败: {0}")]
    Connect(String),
    #[error("未连接")]
    NotConnected,
    #[error("SQL 执行失败: {0}")]
    Query(String),
    #[error("执行超时（{0} ms）")]
    Timeout(u64),
    #[error("结果集超过上限（{0} 行），已截断")]
    Truncated(u32),
    #[error("当前连接为只读模式，已阻止写操作")]
    ReadOnly,
    #[error("该引擎不支持此操作: {0}")]
    Unsupported(&'static str),
    #[error("驱动内部错误: {0}")]
    Internal(String),
}

impl DriverError {
    pub fn user_message(&self) -> String {
        self.to_string()
    }
}

pub type DriverResult<T> = Result<T, DriverError>;

/// 关系型驱动能力声明（UI 据此灰化/隐藏不支持的入口）。
#[derive(Debug, Clone, Copy, Default)]
pub struct Capabilities {
    pub multi_database: bool,
    pub schemas: bool,
    pub sessions: bool,
    pub kill_session: bool,
    pub explain: bool,
    pub ddl: bool,
    pub procedures: bool,
    pub triggers: bool,
    pub events: bool,
    pub backup: bool,
}

#[async_trait]
pub trait RelationalDriver: Send + Sync {
    fn kind(&self) -> DbKind;

    fn config(&self) -> &ConnectionConfig;

    /// 注入连接口令。无口令驱动（如 SQLite）无需覆盖，默认空操作。
    fn set_password(&mut self, _password: String) {}

    fn capabilities(&self) -> Capabilities;

    fn is_connected(&self) -> bool;

    /// 建立连接池；幂等。
    async fn connect(&self) -> DriverResult<()>;

    /// 关闭连接池。
    async fn close(&self);

    /// 连通性探测，返回往返耗时（毫秒）。
    async fn ping(&self) -> DriverResult<u64>;

    async fn server_info(&self) -> DriverResult<ServerInfo>;

    async fn list_databases(&self) -> DriverResult<Vec<String>>;

    async fn list_objects(
        &self,
        database: Option<&str>,
        kinds: &[ObjectKind],
    ) -> DriverResult<Vec<SchemaObject>>;

    async fn describe_table(&self, database: Option<&str>, table: &str) -> DriverResult<TableDef>;

    /// 一次性执行并返回完整结果（受 `limit` 限制）。
    async fn query(
        &self,
        sql: &str,
        limit: u32,
        timeout: Duration,
    ) -> DriverResult<QueryOutcome>;

    /// 流式执行：驱动内部起任务，按批推送至有界通道，实现背压。
    async fn stream(
        &self,
        sql: String,
        limit: u32,
        timeout: Duration,
        tx: mpsc::Sender<StreamEvent>,
    ) -> DriverResult<()>;

    /// 执行写语句，返回影响行数。
    async fn execute(&self, sql: &str, timeout: Duration) -> DriverResult<ExecOutcome>;

    async fn explain(&self, sql: &str) -> DriverResult<QueryOutcome>;

    async fn ddl(&self, database: Option<&str>, table: &str) -> DriverResult<String>;

    /// 估算语句影响行数（用于高危操作二次确认）。
    async fn estimate_affected(&self, sql: &str) -> DriverResult<Option<u64>>;

    async fn count_rows(&self, database: Option<&str>, table: &str) -> DriverResult<u64>;

    async fn sessions(&self) -> DriverResult<Vec<SessionInfo>> {
        Err(DriverError::Unsupported("会话列表"))
    }

    async fn kill_session(&self, _id: &str) -> DriverResult<()> {
        Err(DriverError::Unsupported("终止会话"))
    }

    async fn slow_queries(&self, _limit: usize) -> DriverResult<Vec<SlowQuery>> {
        Ok(Vec::new())
    }

    /// 生成建表 DDL（表设计器用）。
    async fn create_table_ddl(&self, def: &TableDef, database: Option<&str>) -> DriverResult<String>;

    /// 若驱动支持参数化写入，返回 RowWriter 视图。
    fn writer(&self) -> Option<&dyn RowWriter> {
        None
    }
}

// ---------------------------------------------------------------------------
// 通用工具
// ---------------------------------------------------------------------------

/// 标识符引用：按方言选择引号并转义。
pub fn quote_ident(kind: DbKind, ident: &str) -> String {
    match kind {
        DbKind::Mysql | DbKind::Sqlite => format!("`{}`", ident.replace('`', "``")),
        DbKind::SqlServer => format!("[{}]", ident.replace(']', "]]")),
        DbKind::Postgres | DbKind::Oracle => format!("\"{}\"", ident.replace('"', "\"\"")),
        _ => ident.to_string(),
    }
}

/// 限定名（schema.table）。
pub fn qualified(kind: DbKind, schema: Option<&str>, table: &str) -> String {
    match schema {
        Some(s) if !s.is_empty() => {
            format!("{}.{}", quote_ident(kind, s), quote_ident(kind, table))
        }
        _ => quote_ident(kind, table),
    }
}

/// 把类型名归一化，供 UI 展示与编辑校验使用。
pub fn normalize_type(type_name: &str) -> String {
    let t = type_name.trim().to_ascii_lowercase();
    let base = t.split('(').next().unwrap_or(&t).trim().to_string();
    match base.as_str() {
        "int" | "int4" | "integer" => "integer".into(),
        "bigint" | "int8" => "bigint".into(),
        "smallint" | "int2" => "smallint".into(),
        "tinyint" => "tinyint".into(),
        "varchar" | "character varying" | "nvarchar" | "varchar2" => "varchar".into(),
        "text" | "longtext" | "mediumtext" | "clob" => "text".into(),
        "decimal" | "numeric" | "number" => "decimal".into(),
        "float" | "double" | "real" | "double precision" => "float".into(),
        "datetime" | "timestamp" | "timestamptz" | "timestamp with time zone" => "timestamp".into(),
        "date" => "date".into(),
        "time" => "time".into(),
        "bool" | "boolean" | "bit" => "boolean".into(),
        "blob" | "bytea" | "varbinary" | "binary" | "raw" => "blob".into(),
        "json" | "jsonb" => "json".into(),
        other => other.to_string(),
    }
}

pub fn has_scale(type_name: &str) -> Option<u32> {
    let lower = type_name.to_ascii_lowercase();
    let start = lower.find('(')?;
    let end = lower[start..].find(')')? + start;
    let inner = &lower[start + 1..end];
    inner.split(',').nth(1).and_then(|s| s.trim().parse().ok())
}

/// 从 `SELECT ... FROM t` 中提取表名（用于结果集列类型回填）。
pub fn table_from_query(sql: &str) -> Option<String> {
    let upper = sql.to_uppercase();
    let idx = upper.find("FROM")?;
    let rest = &sql[idx + 4..];
    let token = rest
        .trim_start()
        .split(|c: char| c.is_whitespace() || c == ';' || c == ',')
        .next()?;
    let name = token.trim_matches(|c| c == '"' || c == '`' || c == '[' || c == ']');
    if name.is_empty() || name.contains('(') {
        None
    } else {
        Some(name.rsplit('.').next().unwrap_or(name).to_string())
    }
}

/// 生成 `SELECT * FROM t LIMIT n` 的方言版本。
pub fn limit_sql(kind: DbKind, sql: &str, limit: u32) -> String {
    let trimmed = sql.trim().trim_end_matches(';');
    match kind {
        DbKind::SqlServer => {
            // 已含 TOP 或 OFFSET 则不再追加
            if trimmed.to_uppercase().contains(" TOP ") || trimmed.to_uppercase().contains("OFFSET") {
                trimmed.to_string()
            } else if let Some(pos) = trimmed.to_uppercase().find("SELECT") {
                let mut out = String::new();
                out.push_str(&trimmed[..pos + 6]);
                out.push_str(&format!(" TOP {limit}"));
                out.push_str(&trimmed[pos + 6..]);
                out
            } else {
                trimmed.to_string()
            }
        }
        DbKind::Oracle if !trimmed.to_uppercase().contains("FETCH FIRST") && !trimmed.to_uppercase().contains("ROWNUM") => {
            format!("{trimmed} FETCH FIRST {limit} ROWS ONLY")
        }
        _ => {
            if trimmed.to_uppercase().contains(" LIMIT ") {
                trimmed.to_string()
            } else {
                format!("{trimmed} LIMIT {limit}")
            }
        }
    }
}

/// 用于高危确认的影响行数预估 SQL。
pub fn estimate_sql(kind: DbKind, sql: &str) -> Option<String> {
    let upper = sql.to_uppercase();
    let first = upper.trim_start().split_whitespace().next()?;
    if first != "DELETE" && first != "UPDATE" {
        return None;
    }
    let from_idx = upper.find("FROM")?;
    let after_from = &sql[from_idx + 4..];
    let table: String = after_from
        .trim_start()
        .chars()
        .take_while(|c| !c.is_whitespace() && *c != ';')
        .collect();
    let where_idx = upper.find("WHERE")?;
    let where_clause = sql[where_idx + 5..].trim().trim_end_matches(';');
    if table.is_empty() {
        return None;
    }
    let _ = kind;
    Some(format!("SELECT COUNT(*) FROM {table} WHERE {where_clause}"))
}

/// 便捷：构造列元数据。
pub fn col(name: &str, type_name: &str) -> ColumnMeta {
    ColumnMeta::new(name, type_name)
}

/// 是否为只读语句（仅看首个关键字，注释/空白已跳过）。
pub fn is_read_only_sql(sql: &str) -> bool {
    let mut s = sql.trim_start();
    loop {
        if s.starts_with("--") {
            match s.find('\n') {
                Some(p) => s = s[p + 1..].trim_start(),
                None => return true,
            }
        } else if s.starts_with("/*") {
            match s.find("*/") {
                Some(p) => s = s[p + 2..].trim_start(),
                None => return true,
            }
        } else {
            break;
        }
    }
    let first = s.split_whitespace().next().unwrap_or("").to_ascii_uppercase();
    matches!(
        first.as_str(),
        "SELECT" | "SHOW" | "DESCRIBE" | "DESC" | "EXPLAIN" | "PRAGMA" | "WITH" | "VALUES"
    )
}

/// 行内编辑 / 批量导入所需的参数化写入能力。
/// 默认返回「不支持」，由各驱动按自身能力覆盖。
#[async_trait]
pub trait RowWriter: Send + Sync {
    async fn update_row(
        &self,
        table: &str,
        pk: &[(String, dbhelper_models::Value)],
        changes: &[(String, dbhelper_models::Value)],
    ) -> DriverResult<u64>;

    async fn insert_rows(
        &self,
        table: &str,
        columns: &[String],
        rows: &[Vec<dbhelper_models::Value>],
    ) -> DriverResult<u64>;
}
