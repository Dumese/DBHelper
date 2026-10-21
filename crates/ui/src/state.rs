//! 界面状态定义（与视图分离，便于单独推理状态迁移）。

use dbhelper_core::Metrics;
use dbhelper_models::*;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Connections,
    Query,
    Data,
    Er,
    Redis,
    Mongo,
    Es,
    Monitor,
    Settings,
    /// 对象设计器（表 / 视图 / 索引 / 触发器）。
    Designer,
    /// BI 仪表板。
    Dash,
    /// 迁移与同步。
    Transfer,
    /// 服务器管理（用户权限 / 命令监控 / 聚合 / Pub-Sub / 模式分析）。
    Admin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sidebar {
    Connections,
    Keyspace,
    Collections,
    Indices,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabKind {
    Query,
    Data,
    Er,
    Redis,
    Mongo,
    Es,
    Monitor,
}

#[derive(Debug, Clone)]
pub struct Tab {
    pub title: String,
    pub kind: TabKind,
    pub conn: ConnId,
    pub db: Option<String>,
    pub table: Option<String>,
    pub index: usize,
}

#[derive(Debug, Clone, Default)]
pub struct TreeData {
    pub databases: HashMap<ConnId, Vec<String>>,
    pub tables: HashMap<(ConnId, String), Vec<SchemaObject>>,
    pub views: HashMap<(ConnId, String), Vec<SchemaObject>>,
    pub loading: Option<ConnId>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TreePayload {
    pub databases: Vec<String>,
    pub tables: Vec<SchemaObject>,
    pub views: Vec<SchemaObject>,
}

// ---------------------------------------------------------------------------
// 查询工作区
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResultTab {
    #[default]
    Grid,
    Messages,
    Plan,
}

#[derive(Debug, Clone, Default)]
pub struct QueryTab {
    pub index: usize,
    pub title: String,
    pub conn: ConnId,
    pub sql: String,
    pub caret: usize,
    pub anchor: usize,
    pub scroll: f32,
    pub running: bool,
    pub started: Option<std::time::Instant>,
    pub columns: Vec<ColumnMeta>,
    pub rows: Vec<Row>,
    pub elapsed_ms: u64,
    pub truncated: bool,
    pub read_only: bool,
    pub error: Option<String>,
    pub notices: Vec<String>,
    pub result_tab: ResultTab,
    pub page: usize,
    pub page_size: usize,
    pub total_rows: Option<u64>,
    pub viewport: f32,
    pub gridded: bool,
    /// EXPLAIN 结果（可视化执行计划）。
    pub plan_columns: Vec<String>,
    pub plan_rows: Vec<Vec<String>>,
}

impl QueryTab {
    pub fn new(index: usize, conn: ConnId, title: String) -> Self {
        Self {
            index,
            title,
            conn,
            sql: "SELECT u.id, u.name, u.email, COUNT(o.id) AS orders\nFROM users u\nLEFT JOIN orders o ON o.user_id = u.id\nWHERE u.created_at >= '2026-01-01'\nGROUP BY u.id\nORDER BY orders DESC LIMIT 100;".into(),
            page_size: 100,
            ..Default::default()
        }
    }

    pub fn page_rows(&self) -> &[Row] {
        let start = (self.page * self.page_size).min(self.rows.len());
        let end = (start + self.page_size).min(self.rows.len());
        &self.rows[start..end]
    }

    pub fn page_count(&self) -> usize {
        (self.rows.len() + self.page_size - 1) / self.page_size.max(1)
    }
}

// ---------------------------------------------------------------------------
// 数据网格
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct FilterChip {
    pub column: String,
    pub op: String,
    pub value: String,
}

impl FilterChip {
    pub fn label(&self) -> String {
        format!("{} {} {}", self.column, self.op, self.value)
    }
}

#[derive(Debug, Clone)]
pub struct EditField {
    pub name: String,
    pub type_name: String,
    pub value: String,
    pub original: String,
    pub is_pk: bool,
}

#[derive(Debug, Clone, Default)]
pub struct DataTab {
    pub index: usize,
    pub conn: ConnId,
    pub table: String,
    pub db: Option<String>,
    pub columns: Vec<ColumnMeta>,
    pub def: Option<TableDef>,
    pub rows: Vec<Row>,
    pub total: u64,
    pub page: usize,
    pub page_size: usize,
    pub selected: Option<usize>,
    pub filters: Vec<FilterChip>,
    pub search: String,
    pub sort: Option<(String, bool)>,
    pub edit: Vec<EditField>,
    pub error: Option<String>,
    pub loading: bool,
    pub dirty: usize,
    pub viewport: f32,
    /// 视图模式：0 网格 / 1 表单 / 2 树 / 3 JSON。
    pub view_mode: usize,
    /// 数据脱敏开关（手机号/邮箱/身份证等敏感列打码）。
    pub mask: bool,
    /// 批量编辑：统一设置某列的值。
    pub batch_field: String,
    pub batch_value: String,
}

impl DataTab {
    pub fn new(index: usize, conn: ConnId, table: String, db: Option<String>) -> Self {
        Self { index, conn, table, db, page_size: 100, ..Default::default() }
    }

    pub fn page_count(&self) -> usize {
        ((self.total as usize) + self.page_size - 1) / self.page_size.max(1)
    }

    pub fn pending(&self) -> usize {
        self.edit.iter().filter(|f| f.value != f.original).count()
    }

    pub fn primary_key(&self) -> Vec<String> {
        self.def
            .as_ref()
            .map(|d| d.columns.iter().filter(|c| c.is_pk).map(|c| c.name.clone()).collect())
            .unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// ER
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ErNode {
    pub table: TableDef,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Debug, Clone, Default)]
pub struct ErState {
    pub conn: Option<ConnId>,
    pub nodes: Vec<ErNode>,
    pub selected: Option<usize>,
    pub dragging: Option<(usize, iced::Point)>,
    pub zoom: f32,
    pub mode: usize,
    pub dirty: usize,
    pub error: Option<String>,
    pub loading: bool,
}

impl ErState {
    pub fn reset(&mut self, conn: ConnId) {
        *self = ErState { conn: Some(conn), zoom: 1.0, ..Default::default() };
    }
}

// ---------------------------------------------------------------------------
// NoSQL
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct RedisState {
    pub conn: Option<ConnId>,
    pub info: Option<driver_redis::RedisInfo>,
    pub pattern: String,
    pub db: u8,
    pub keys: Vec<String>,
    pub selected: Option<String>,
    pub key_type: String,
    pub fields: Vec<(String, String)>,
    pub ttl: i64,
    pub memory: u64,
    pub encoding: String,
    pub tab: usize,
    pub error: Option<String>,
    pub loading: bool,
    pub edit_value: String,
    pub command_log: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct MongoState {
    pub conn: Option<ConnId>,
    pub databases: Vec<String>,
    pub db: Option<String>,
    pub collections: Vec<(String, u64)>,
    pub collection: Option<String>,
    pub docs: Vec<serde_json::Value>,
    pub selected: Option<usize>,
    pub pipeline: String,
    pub tab: usize,
    pub error: Option<String>,
    pub loading: bool,
}

#[derive(Debug, Clone, Default)]
pub struct EsState {
    pub conn: Option<ConnId>,
    pub indices: Vec<(String, String, u64, String)>,
    pub index: Option<String>,
    pub dsl: String,
    pub hits: Vec<serde_json::Value>,
    pub total: u64,
    pub took: u64,
    pub timed_out: bool,
    pub error: Option<String>,
    pub loading: bool,
}

// ---------------------------------------------------------------------------
// 监控
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct MonitorState {
    pub conn: Option<ConnId>,
    pub range: usize,
    pub sessions: Vec<SessionInfo>,
    pub slow: Vec<SlowQuery>,
    pub error: Option<String>,
    pub last_update: i64,
}

// ---------------------------------------------------------------------------
// 对象设计器（表 / 视图 / 索引 / 触发器）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct DesignerState {
    pub open: bool,
    pub conn: Option<ConnId>,
    pub table: String,
    /// 0 列设计 / 1 索引 / 2 触发器 / 3 DDL 预览
    pub tab: usize,
    pub is_new: bool,
    pub def: Option<TableDef>,
    pub original: Option<TableDef>,
    pub ddl_preview: String,
    pub index_ddl: String,
    pub trigger_ddl: String,
    pub saving: bool,
    pub error: Option<String>,
    pub saved: Option<String>,
}

// ---------------------------------------------------------------------------
// BI 仪表板
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartKind {
    Bar,
    Line,
    Area,
    Pie,
}

impl ChartKind {
    pub const ALL: [ChartKind; 4] = [ChartKind::Bar, ChartKind::Line, ChartKind::Area, ChartKind::Pie];
    pub fn label(self) -> &'static str {
        match self {
            ChartKind::Bar => "柱状图",
            ChartKind::Line => "折线图",
            ChartKind::Area => "面积图",
            ChartKind::Pie => "饼图",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChartCard {
    pub title: String,
    pub sql: String,
    pub kind: ChartKind,
    /// 最近一次运行的类别标签与数值序列。
    pub labels: Vec<String>,
    pub values: Vec<f64>,
    pub error: Option<String>,
    pub running: bool,
}

impl Default for ChartCard {
    fn default() -> Self {
        Self {
            title: "新图表".into(),
            sql: "SELECT name, COUNT(*) AS n FROM users GROUP BY name LIMIT 8".into(),
            kind: ChartKind::Bar,
            labels: Vec::new(),
            values: Vec::new(),
            error: None,
            running: false,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Dashboard {
    pub name: String,
    pub charts: Vec<ChartCard>,
}

#[derive(Debug, Clone)]
pub struct DashState {
    pub conn: Option<ConnId>,
    pub pages: Vec<Dashboard>,
    pub active: usize,
    pub auto_refresh: bool,
    pub refresh_secs: u64,
    pub last_refresh: i64,
    /// 参数变量（:param 形式）名称→值。
    pub params: Vec<(String, String)>,
    pub tiled: bool,
}

impl Default for DashState {
    fn default() -> Self {
        Self {
            conn: None,
            pages: vec![Dashboard {
                name: "仪表板 1".into(),
                charts: vec![ChartCard::default(), ChartCard {
                    title: "订单趋势".into(),
                    sql: "SELECT DATE(created_at) AS d, COUNT(*) AS n FROM orders GROUP BY d LIMIT 30".into(),
                    kind: ChartKind::Line,
                    ..Default::default()
                }],
            }],
            active: 0,
            auto_refresh: true,
            refresh_secs: 30,
            last_refresh: 0,
            params: Vec::new(),
            tiled: true,
        }
    }
}

// ---------------------------------------------------------------------------
// 迁移与同步
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferMode {
    Structure,
    Data,
    Sync,
    Copy,
}

impl TransferMode {
    pub const ALL: [TransferMode; 4] =
        [TransferMode::Structure, TransferMode::Data, TransferMode::Sync, TransferMode::Copy];
    pub fn label(self) -> &'static str {
        match self {
            TransferMode::Structure => "结构同步",
            TransferMode::Data => "数据传输",
            TransferMode::Sync => "数据同步",
            TransferMode::Copy => "跨库复制",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TransferState {
    pub open: bool,
    pub mode: TransferMode,
    pub source_conn: Option<ConnId>,
    pub target_conn: Option<ConnId>,
    pub table: String,
    pub running: bool,
    pub log: Vec<String>,
    pub error: Option<String>,
}

impl Default for TransferMode {
    fn default() -> Self {
        TransferMode::Structure
    }
}

// ---------------------------------------------------------------------------
// 服务器管理
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct AdminState {
    pub conn: Option<ConnId>,
    /// 0 用户权限 / 1 命令监控 / 2 聚合管道 / 3 Pub-Sub / 4 模式分析
    pub tab: usize,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub pipeline: String,
    pub pubsub_channel: String,
    pub pubsub_messages: Vec<String>,
    pub pubsub_live: bool,
    pub analysis: String,
    pub running: bool,
    pub error: Option<String>,
}

// ---------------------------------------------------------------------------
// 导入向导
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct ImportState {
    pub open: bool,
    pub path: String,
    pub table: String,
    pub columns: Vec<String>,
    pub preview: Vec<Vec<Value>>,
    pub running: bool,
    pub result: Option<String>,
    pub error: Option<String>,
}

// ---------------------------------------------------------------------------
// 查询智能（补全 / 诊断 / 悬停）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct CompletionState {
    pub visible: bool,
    pub items: Vec<String>,
    pub selected: usize,
}

#[derive(Debug, Clone, Default)]
pub struct Diagnostic {
    pub line: usize,
    pub message: String,
}

// ---------------------------------------------------------------------------
// 设置
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct SettingsUi {
    pub section: usize,
    pub export_step: i32,
    pub export_items: [bool; 4],
    pub export_format: usize,
    pub export_result: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ConnUi {
    pub recent: Vec<ConnectionConfig>,
    pub menu_for: Option<ConnId>,
}

// ---------------------------------------------------------------------------
// 向导
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Wizard {
    pub step: i32,
    pub kind: DbKind,
    pub name: String,
    pub host: String,
    pub port: String,
    pub database: String,
    pub user: String,
    pub password: String,
    pub group: String,
    pub readonly: bool,
    pub ssl: usize,
    pub ssh: bool,
    pub ssh_host: String,
    pub testing: bool,
    pub test_result: Option<Result<String, String>>,
    pub error: Option<String>,
    pub saving: bool,
    pub edit_id: Option<ConnId>,
}

impl Wizard {
    pub fn new(kind: DbKind) -> Self {
        Self {
            step: 0,
            kind,
            name: String::new(),
            host: if kind.is_file_based() { String::new() } else { "127.0.0.1".into() },
            port: kind.default_port().to_string(),
            database: String::new(),
            user: String::new(),
            password: String::new(),
            group: "本地与沙箱".into(),
            readonly: false,
            ssl: if kind.is_file_based() { 0 } else { 1 },
            ssh: false,
            ssh_host: String::new(),
            testing: false,
            test_result: None,
            error: None,
            saving: false,
            edit_id: None,
        }
    }

    pub fn to_config(&self, id: ConnId) -> ConnectionConfig {
        let mut cfg = ConnectionConfig::new(id, self.name.trim().to_string(), self.kind);
        cfg.host = self.host.trim().to_string();
        cfg.port = self.port.parse().unwrap_or(self.kind.default_port());
        cfg.database = self.database.trim().to_string();
        cfg.username = self.user.trim().to_string();
        cfg.group = self.group.trim().to_string();
        cfg.readonly = self.readonly;
        cfg.ssl = SslMode::ALL[self.ssl.min(SslMode::ALL.len() - 1)];
        cfg.ssh.enabled = self.ssh;
        cfg.ssh.host = self.ssh_host.clone();
        cfg.created_at = dbhelper_utils::textfmt::now_secs();
        cfg
    }

    pub fn username_hint(&self) -> String {
        self.user.clone()
    }

    pub fn validate(&self) -> Option<String> {
        if self.name.trim().is_empty() {
            return Some("请填写连接名称".into());
        }
        if self.kind.is_file_based() {
            if self.database.trim().is_empty() {
                return Some("请选择 SQLite 数据库文件".into());
            }
        } else {
            if self.host.trim().is_empty() {
                return Some("请填写主机地址".into());
            }
            if self.port.parse::<u16>().is_err() {
                return Some("端口必须是 1–65535 的数字".into());
            }
        }
        None
    }
}

// ---------------------------------------------------------------------------
// 弹层
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum Modal {
    None,
    Wizard,
    Export,
    Import,
    Confirm(Confirm),
    About,
    Shortcuts,
    History,
}

#[derive(Debug, Clone)]
pub struct Confirm {
    pub title: String,
    pub message: String,
    pub detail: Option<String>,
    pub danger: bool,
    pub action: ConfirmAction,
}

#[derive(Debug, Clone)]
pub enum ConfirmAction {
    RunSql { conn: ConnId, sql: String },
    DeleteConnection(ConnId),
    DeleteKey(String),
    KillSession(String),
    DeleteDoc(usize),
}

#[derive(Debug, Clone)]
pub struct PendingDanger {
    pub conn: ConnId,
    pub sql: String,
    pub action: String,
    pub affected: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct Toast {
    pub text: String,
    pub tone: crate::theme::Tone,
    pub at: std::time::Instant,
}
