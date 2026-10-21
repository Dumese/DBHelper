//! 领域模型：连接、对象结构、结果集、设置、历史记录。
//!
//! 本 crate 不含任何 I/O，只提供可序列化的纯数据类型，
//! 供 drivers / core / ui 三层共享，避免循环依赖。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::time::Duration;

pub type ConnId = i64;

/// 支持的 8 种数据库。顺序即连接向导中的展示顺序。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DbKind {
    Mysql,
    Postgres,
    Oracle,
    SqlServer,
    Sqlite,
    Redis,
    Mongodb,
    Elasticsearch,
}

impl DbKind {
    pub const ALL: [DbKind; 8] = [
        DbKind::Mysql,
        DbKind::Postgres,
        DbKind::Oracle,
        DbKind::SqlServer,
        DbKind::Sqlite,
        DbKind::Redis,
        DbKind::Mongodb,
        DbKind::Elasticsearch,
    ];

    pub fn label(self) -> &'static str {
        match self {
            DbKind::Mysql => "MySQL",
            DbKind::Postgres => "PostgreSQL",
            DbKind::Oracle => "Oracle",
            DbKind::SqlServer => "SQL Server",
            DbKind::Sqlite => "SQLite",
            DbKind::Redis => "Redis",
            DbKind::Mongodb => "MongoDB",
            DbKind::Elasticsearch => "Elasticsearch",
        }
    }

    /// 连接树 / 图标使用的短名。
    pub fn short(self) -> &'static str {
        match self {
            DbKind::Mysql => "MySQL",
            DbKind::Postgres => "PG",
            DbKind::Oracle => "ORA",
            DbKind::SqlServer => "MSSQL",
            DbKind::Sqlite => "SQLite",
            DbKind::Redis => "Redis",
            DbKind::Mongodb => "Mongo",
            DbKind::Elasticsearch => "Elastic",
        }
    }

    pub fn is_relational(self) -> bool {
        matches!(
            self,
            DbKind::Mysql
                | DbKind::Postgres
                | DbKind::Oracle
                | DbKind::SqlServer
                | DbKind::Sqlite
        )
    }

    pub fn is_file_based(self) -> bool {
        matches!(self, DbKind::Sqlite)
    }

    pub fn default_port(self) -> u16 {
        match self {
            DbKind::Mysql => 3306,
            DbKind::Postgres => 5432,
            DbKind::Oracle => 1521,
            DbKind::SqlServer => 1433,
            DbKind::Sqlite => 0,
            DbKind::Redis => 6379,
            DbKind::Mongodb => 27017,
            DbKind::Elasticsearch => 9200,
        }
    }

    /// 数据库图标的主色（用于连接树小标识），与原型图配色一致。
    pub fn accent_hex(self) -> &'static str {
        match self {
            DbKind::Mysql => "#00758F",
            DbKind::Postgres => "#336791",
            DbKind::Oracle => "#C74634",
            DbKind::SqlServer => "#A91D22",
            DbKind::Sqlite => "#0F80CC",
            DbKind::Redis => "#D82C20",
            DbKind::Mongodb => "#13AA52",
            DbKind::Elasticsearch => "#FEC514",
        }
    }

    pub fn driver_note(self) -> &'static str {
        match self {
            DbKind::Mysql | DbKind::Postgres | DbKind::Sqlite => "sqlx (纯 Rust, 连接池)",
            DbKind::SqlServer => "tiberius (TDS, 纯 Rust)",
            DbKind::Oracle => "rust-oracle (ODPI-C)",
            DbKind::Redis => "redis-rs (tokio)",
            DbKind::Mongodb => "mongodb 官方异步驱动",
            DbKind::Elasticsearch => "reqwest REST",
        }
    }
}

impl fmt::Display for DbKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// TLS / SSL 校验级别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SslMode {
    #[default]
    Disabled,
    Preferred,
    Required,
    VerifyCa,
    VerifyFull,
}

impl SslMode {
    pub const ALL: [SslMode; 5] = [
        SslMode::Disabled,
        SslMode::Preferred,
        SslMode::Required,
        SslMode::VerifyCa,
        SslMode::VerifyFull,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SslMode::Disabled => "禁用",
            SslMode::Preferred => "优先",
            SslMode::Required => "必须",
            SslMode::VerifyCa => "校验 CA",
            SslMode::VerifyFull => "完全校验",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SshConfig {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: SshAuth,
    /// 私钥路径（仅 KeyFile 模式）。
    pub key_path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SshAuth {
    #[default]
    Password,
    KeyFile,
}

/// 一条连接的完整配置。密码不在此结构中，而是以引用名存入 Vault。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionConfig {
    pub id: ConnId,
    pub name: String,
    pub kind: DbKind,
    pub host: String,
    pub port: u16,
    /// 数据库名；SQLite 为文件路径。
    pub database: String,
    pub username: String,
    /// Vault 中的凭据键名。
    pub secret_key: Option<String>,
    pub group: String,
    /// 颜色标记，形如 "#E03E3E"。
    pub color: Option<String>,
    pub readonly: bool,
    pub ssl: SslMode,
    pub ssh: SshConfig,
    pub proxy: String,
    pub params: BTreeMap<String, String>,
    pub connect_timeout_ms: u64,
    pub statement_timeout_ms: u64,
    pub max_rows: u32,
    pub environment: String,
    pub created_at: i64,
}

impl ConnectionConfig {
    pub fn new(id: ConnId, name: impl Into<String>, kind: DbKind) -> Self {
        let name = name.into();
        Self {
            id,
            name,
            kind,
            host: if kind.is_file_based() { String::new() } else { "127.0.0.1".into() },
            port: kind.default_port(),
            database: String::new(),
            username: String::new(),
            secret_key: None,
            group: String::new(),
            color: None,
            readonly: false,
            ssl: if kind.is_file_based() { SslMode::Disabled } else { SslMode::Preferred },
            ssh: SshConfig::default(),
            proxy: String::new(),
            params: BTreeMap::new(),
            connect_timeout_ms: 8_000,
            statement_timeout_ms: 30_000,
            max_rows: 1_000,
            environment: "本地与沙箱".into(),
            created_at: 0,
        }
    }

    pub fn statement_timeout(&self) -> Duration {
        Duration::from_millis(self.statement_timeout_ms.clamp(500, 600_000))
    }

    /// 连接树右侧的副标题，例如 `MySQL 8.0.35 · prod-db-01:3306`。
    pub fn endpoint_label(&self) -> String {
        if self.kind.is_file_based() {
            format!("SQLite · {}", self.database)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }

    /// 列表类界面用的短端点：文件库只显示文件名，避免长路径撑破面板。
    pub fn endpoint_short(&self) -> String {
        if self.kind.is_file_based() {
            let name = std::path::Path::new(&self.database)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| self.database.clone());
            format!("SQLite · {name}")
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }

    pub fn secret_key(&self) -> String {
        self.secret_key
            .clone()
            .unwrap_or_else(|| format!("conn:{}", self.id))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Group {
    pub name: String,
    pub collapsed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectKind {
    Table,
    View,
    Procedure,
    Function,
    Trigger,
    Event,
    Index,
    Constraint,
    Column,
    Collection,
    Key,
}

impl ObjectKind {
    pub fn label(self) -> &'static str {
        match self {
            ObjectKind::Table => "表",
            ObjectKind::View => "视图",
            ObjectKind::Procedure => "存储过程",
            ObjectKind::Function => "函数",
            ObjectKind::Trigger => "触发器",
            ObjectKind::Event => "事件",
            ObjectKind::Index => "索引",
            ObjectKind::Constraint => "约束",
            ObjectKind::Column => "列",
            ObjectKind::Collection => "集合",
            ObjectKind::Key => "键",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaObject {
    pub name: String,
    pub kind: ObjectKind,
    pub schema: Option<String>,
    pub row_estimate: Option<u64>,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ColumnDef {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub default: Option<String>,
    pub is_pk: bool,
    pub is_unique: bool,
    pub fk: Option<ForeignKey>,
    pub extra: String,
    pub comment: Option<String>,
    pub ordinal: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForeignKey {
    pub column: String,
    pub ref_table: String,
    pub ref_column: String,
    pub constraint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct IndexDef {
    pub name: String,
    pub columns: Vec<String>,
    pub unique: bool,
    pub primary: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TableDef {
    pub schema: Option<String>,
    pub name: String,
    pub columns: Vec<ColumnDef>,
    pub indexes: Vec<IndexDef>,
    pub row_estimate: Option<u64>,
    pub comment: Option<String>,
}

impl TableDef {
    pub fn primary_key(&self) -> Vec<&ColumnDef> {
        self.columns.iter().filter(|c| c.is_pk).collect()
    }

    pub fn foreign_keys(&self) -> Vec<(&ColumnDef, &ForeignKey)> {
        self.columns
            .iter()
            .filter_map(|c| c.fk.as_ref().map(|fk| (c, fk)))
            .collect()
    }
}

// ---------------------------------------------------------------------------
// 结果集
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnMeta {
    pub name: String,
    pub type_name: String,
}

impl ColumnMeta {
    pub fn new(name: impl Into<String>, type_name: impl Into<String>) -> Self {
        Self { name: name.into(), type_name: type_name.into() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value {
    Null,
    Int(i64),
    Float(f64),
    Decimal(String),
    Text(String),
    Bool(bool),
    Bytes(Vec<u8>),
    Timestamp(String),
    Json(String),
}

impl Value {
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    /// 网格展示用文本（不带引号）。
    pub fn display(&self) -> String {
        match self {
            Value::Null => "NULL".into(),
            Value::Int(v) => v.to_string(),
            Value::Float(v) => {
                if v.fract() == 0.0 && v.abs() < 1e15 {
                    format!("{v:.0}")
                } else {
                    format!("{v}")
                }
            }
            Value::Decimal(s) | Value::Text(s) | Value::Timestamp(s) | Value::Json(s) => s.clone(),
            Value::Bool(b) => {
                if *b {
                    "true".into()
                } else {
                    "false".into()
                }
            }
            Value::Bytes(b) => format!("<{} bytes>", b.len()),
        }
    }

    /// SQL 字面量（导出 INSERT 时使用）。
    pub fn to_sql_literal(&self) -> String {
        match self {
            Value::Null => "NULL".into(),
            Value::Int(v) => v.to_string(),
            Value::Float(v) => v.to_string(),
            Value::Decimal(s) => s.clone(),
            Value::Bool(b) => if *b { "TRUE" } else { "FALSE" }.into(),
            Value::Bytes(b) => {
                let mut s = String::with_capacity(b.len() * 2 + 3);
                s.push_str("X'");
                for byte in b {
                    s.push_str(&format!("{byte:02X}"));
                }
                s.push('\'');
                s
            }
            Value::Text(s) | Value::Timestamp(s) | Value::Json(s) => {
                format!("'{}'", s.replace('\'', "''"))
            }
        }
    }

    pub fn edit_text(&self) -> String {
        match self {
            Value::Null => String::new(),
            other => other.display(),
        }
    }
}

pub type Row = Vec<Value>;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QueryOutcome {
    pub columns: Vec<ColumnMeta>,
    pub rows: Vec<Row>,
    pub rows_affected: u64,
    pub elapsed_ms: u64,
    pub truncated: bool,
    pub limit: u32,
    pub notices: Vec<String>,
    /// 该语句是否为只读（用于只读连接校验）。
    pub read_only: bool,
}

impl QueryOutcome {
    pub fn empty() -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// 首批列元数据。
    Columns {
        columns: Vec<ColumnMeta>,
        read_only: bool,
    },
    Batch(Vec<Row>),
    Done {
        elapsed_ms: u64,
        rows_affected: u64,
        truncated: bool,
    },
    Failed(String),
}

#[derive(Debug, Clone, Default)]
pub struct ExecOutcome {
    pub rows_affected: u64,
    pub elapsed_ms: u64,
    pub last_insert_id: Option<i64>,
    pub notices: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ServerInfo {
    pub version: String,
    pub edition: String,
    pub uptime_secs: Option<u64>,
    pub current_database: String,
    pub extra: Vec<(String, String)>,
}

#[derive(Debug, Clone, Default)]
pub struct SessionInfo {
    pub id: String,
    pub user: String,
    pub database: String,
    pub state: String,
    pub command: String,
    pub elapsed_secs: f64,
    pub sql: String,
}

#[derive(Debug, Clone, Default)]
pub struct SlowQuery {
    pub elapsed_secs: f64,
    pub session_id: String,
    pub sql: String,
}

/// 结构化对象浏览用的数据库节点。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseNode {
    pub name: String,
    pub size_bytes: Option<u64>,
}

// ---------------------------------------------------------------------------
// 设置 / 历史
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    #[default]
    Light,
    Dark,
}

impl std::fmt::Display for ThemeMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

impl ThemeMode {
    pub fn label(self) -> &'static str {
        match self {
            ThemeMode::Light => "浅色",
            ThemeMode::Dark => "深色",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    #[default]
    ZhCn,
    EnUs,
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

impl Language {
    pub const ALL: [Language; 2] = [Language::ZhCn, Language::EnUs];

    pub fn label(self) -> &'static str {
        match self {
            Language::ZhCn => "简体中文",
            Language::EnUs => "English",
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Language::ZhCn => "zh-CN",
            Language::EnUs => "en-US",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub theme: ThemeMode,
    pub language: Language,
    pub editor_font: String,
    pub ui_font: String,
    pub ui_serif: bool,
    pub font_size: u16,
    pub show_line_numbers: bool,
    pub auto_wrap: bool,
    pub syntax_highlight: bool,
    pub bracket_highlight: bool,
    pub log_dir: String,
    pub config_path: String,
    pub max_rows: u32,
    pub statement_timeout_ms: u64,
    pub memory_limit_mb: u64,
    pub sidebar_width: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: ThemeMode::Light,
            language: Language::ZhCn,
            editor_font: "JetBrains Mono".into(),
            ui_font: "Noto Sans SC".into(),
            ui_serif: false,
            font_size: 14,
            show_line_numbers: true,
            auto_wrap: true,
            syntax_highlight: true,
            bracket_highlight: false,
            log_dir: String::new(),
            config_path: String::new(),
            max_rows: 1_000,
            statement_timeout_ms: 30_000,
            memory_limit_mb: 500,
            sidebar_width: 210.0,
        }
    }
}

impl Settings {
    /// 界面字号缩放系数（基准 14px）。
    pub fn scale(&self) -> f32 {
        self.font_size as f32 / 14.0
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: i64,
    pub conn_id: ConnId,
    pub conn_name: String,
    pub sql: String,
    pub executed_at: i64,
    pub elapsed_ms: u64,
    pub rows: u64,
    pub ok: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedQuery {
    pub id: i64,
    pub name: String,
    pub sql: String,
    pub conn_id: Option<ConnId>,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snippet {
    pub id: i64,
    pub name: String,
    pub body: String,
}

/// 调度任务（自动化）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScheduleTask {
    pub id: i64,
    pub name: String,
    pub conn_id: ConnId,
    pub sql: String,
    pub interval_secs: u64,
    pub enabled: bool,
    pub last_run: Option<i64>,
    pub last_result: Option<String>,
}
