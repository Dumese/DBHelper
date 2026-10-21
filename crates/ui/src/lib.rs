//! DBHelper UI：状态机 + 视图装配。
//!
//! 采用 iced 的 Elm 架构：`Msg` 是唯一状态变更入口，`view` 是纯函数。
//! 所有耗时操作（连接、查询、导出）通过 `Task` 派发到 tokio 运行时。

pub mod i18n;
pub mod icons;
pub mod kit;
pub mod perf;
pub mod state;
pub mod theme;
pub mod update;
pub mod update_features;
pub mod update_ops;
pub mod view;
pub mod widgets;

use dbhelper_core::{AppCore, Metrics};
use dbhelper_models::*;
use iced::{Element, Font, Subscription, Task, Theme};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use kit::Ctx;
use state::*;

/// 无边框窗口的缩放热区判定：记录最近一次光标位置与窗口尺寸，
/// 事件订阅里直接读写，不产生额外的 Msg（否则鼠标移动会触发整窗重绘）。
#[derive(Default)]
pub struct WinProbe {
    pub cursor: iced::Point,
    pub size: iced::Size,
}

static PROBE: std::sync::OnceLock<std::sync::Mutex<WinProbe>> = std::sync::OnceLock::new();

/// 正在进行的边缘缩放：方向、按下时的光标、窗口尺寸与位置。
static RESIZE: std::sync::OnceLock<std::sync::Mutex<Option<ResizeState>>> = std::sync::OnceLock::new();

#[derive(Clone, Copy)]
pub struct ResizeState {
    pub dir: iced::window::Direction,
    pub cursor: iced::Point,
    pub size: iced::Size,
    pub pos: iced::Point,
}

fn resize_state() -> &'static std::sync::Mutex<Option<ResizeState>> {
    RESIZE.get_or_init(|| std::sync::Mutex::new(None))
}

/// 供 update 读取当前光标与窗口尺寸。
pub fn probe_snapshot() -> (iced::Point, iced::Size) {
    let p = probe().lock().unwrap();
    (p.cursor, p.size)
}

pub fn set_resize_state(dir: iced::window::Direction, cursor: iced::Point, size: iced::Size, pos: iced::Point) {
    if let Ok(mut r) = resize_state().lock() {
        *r = Some(ResizeState { dir, cursor, size, pos });
    }
}

fn probe() -> &'static std::sync::Mutex<WinProbe> {
    PROBE.get_or_init(|| {
        std::sync::Mutex::new(WinProbe {
            cursor: iced::Point::ORIGIN,
            size: iced::Size::new(1440.0, 900.0),
        })
    })
}

pub struct App {
    /// 无边框窗口的最大化实现为「全屏模式」，可覆盖任务栏
    pub fullscreen: bool,
    /// 当前视口尺寸（逻辑像素），用于响应式布局
    pub viewport: iced::Size,
    pub core: Option<Arc<AppCore>>,
    pub boot_error: Option<String>,
    pub settings: Settings,
    pub window: iced::window::Id,

    pub page: Page,
    pub tabs: Vec<Tab>,
    pub active_tab: usize,
    pub conns: Vec<ConnectionConfig>,
    pub groups: Vec<String>,
    pub expanded: HashSet<String>,
    pub selected_node: Option<String>,
    pub search: String,
    pub tree: TreeData,

    pub wizard: Option<Wizard>,
    pub modal: Modal,
    pub toasts: Vec<Toast>,

    pub query_tabs: Vec<QueryTab>,
    pub data_tabs: Vec<DataTab>,
    pub er: ErState,
    pub redis: RedisState,
    pub mongo: MongoState,
    pub es: EsState,
    pub monitor: MonitorState,
    pub settings_ui: SettingsUi,
    pub conn_ui: ConnUi,
    pub designer: DesignerState,
    pub dash: DashState,
    pub transfer: TransferState,
    pub admin: AdminState,
    pub import: ImportState,
    pub completion: CompletionState,

    pub active_conn: Option<ConnId>,
    pub database: Option<String>,

    pub metrics: Metrics,
    pub now: i64,
    pub busy: Option<String>,
    pub editor_focus: bool,
    pub caret_blink: bool,
    pub confirm: Option<Confirm>,
    pub connected: HashSet<ConnId>,
    pub nav_stack: Vec<Page>,
    pub es_editor: iced::widget::text_editor::Content,
    pub mongo_editor: iced::widget::text_editor::Content,
    /// 实时 SQL 诊断（括号/引号/高危语句）。
    pub diagnostics: Vec<Diagnostic>,
    /// 编辑器悬停词（对象信息提示）。
    pub hover_word: Option<String>,
}

#[derive(Clone)]
pub enum Msg {
    Booted(Result<Arc<AppCore>, String>),
    Tick,
    Blink,
    WindowDrag,
    WindowMinimize,
    WindowReady(Option<iced::window::Id>),
    ViewportResized(iced::Size),
    ResizeReady(iced::window::Direction, iced::Point, iced::Size, Option<iced::Point>),
    ResizeTo((iced::Size, iced::Point)),
    WindowMaximize,
    WindowResize(iced::window::Direction),
    WindowClose,

    // 导航
    SelectTab(usize),
    CloseTab(usize),
    NewQueryTab,
    ToggleNode(String),
    SearchChanged(String),
    OpenConnection(ConnId),
    ConnectConnection(ConnId),
    Connected(ConnId, Result<String, String>),
    TreeLoaded(ConnId, Result<TreePayload, String>),
    OpenTable(ConnId, String),
    OpenMonitor(ConnId),
    LoadEr(ConnId),
    OpenRedis(ConnId),
    OpenMongo(ConnId),
    OpenEs(ConnId),
    OpenSettings,
    GoConnections,
    RefreshTree,
    ConnMenu(ConnId),
    DeleteConnection(ConnId),

    // 向导
    WizardOpen,
    WizardStart,
    WizardClose,
    WizardKind(DbKind),
    WizardStep(i32),
    WizardName(String),
    WizardHost(String),
    WizardPort(String),
    WizardDatabase(String),
    WizardUser(String),
    WizardPassword(String),
    WizardGroup(String),
    WizardReadonly(bool),
    WizardSsl(usize),
    WizardSsh(bool),
    WizardSshHost(String),
    WizardTest,
    WizardTested(Result<String, String>),
    WizardSave,
    WizardSaved(Result<(), String>),
    WizardBrowseFile,

    // 查询
    QueryEdit(String),
    QueryRun,
    QueryStop,
    QueryFormat,
    QuerySave,
    QueryTabPick(usize),
    ResultTabPick(usize),
    Stream(usize, StreamEvent),
    QueryStarted(usize, Result<std::sync::Arc<tokio::sync::Mutex<Option<tokio::sync::mpsc::Receiver<StreamEvent>>>>, String>),
    QueryDone(usize),
    EditorPaste(Option<String>),
    EstimateReady(Option<u64>),
    DdlReady(String),
    DdlSaved(Result<(), String>),
    MongoDocsDeleted(Result<u64, String>),
    ExportResult,
    ExportPicked(Option<String>),
    CopyResult,
    EditorClick(usize),
    EditorKey(iced::keyboard::Event),
    EditorIme(String),
    EditorFocus(bool),

    // 数据网格
    DataLoaded(usize, Result<TableDef, String>),
    DataRows(usize, Result<(Vec<Row>, u64), String>),
    DataPage(i64),
    DataSelect(usize),
    DataCellPick(String, String),
    DataFieldEdit(String),
    DataCommit,
    DataUndo,
    DataCommitted(Result<u64, String>),
    DataSearch(String),
    DataFilterRemove(usize),
    DataSort(String),
    DataAddFilter,
    DataReload,
    DataViewMode(usize),
    DataMaskToggle,
    DataBatchField(String),
    DataBatchValue(String),
    DataBatchApply,

    // ER
    ErLoaded(ConnId, Result<Vec<TableDef>, String>),
    ErDragStart(usize, iced::Point),
    ErDrag(usize, iced::Point),
    ErDragEnd,
    ErSelect(usize),
    ErZoom(f32),
    ErAutoLayout,
    ErExportDdl,
    ErAddTable,
    ErMode(usize),

    // Redis
    RedisLoaded(ConnId, Result<driver_redis::RedisInfo, String>),
    RedisScan(String),
    RedisKeys(Vec<String>),
    RedisPickDb(u8),
    RedisPickKey(String),
    RedisKeyLoaded(Result<(String, Vec<(String, String)>, i64, u64, String), String>),
    RedisTypeTab(usize),
    RedisEdit(String),
    RedisSave,
    RedisDelete,

    // Mongo
    MongoLoaded(Result<(Vec<String>, Vec<(String, u64)>), String>),
    MongoPickDb(String),
    MongoPickColl(String),
    MongoDocs(Result<Vec<serde_json::Value>, String>),
    MongoPipeline(String),
    MongoRun,
    MongoViewTab(usize),
    MongoActions(usize),
    MongoAction(iced::widget::text_editor::Action),
    EsAction(iced::widget::text_editor::Action),
    MongoDeleteDoc(usize),

    // ES
    EsLoaded(Result<Vec<(String, String, u64, String)>, String>),
    EsPickIndex(String),
    EsDsl(String),
    EsRun,
    EsResult(Result<(u64, u64, Vec<serde_json::Value>, bool), String>),

    // 监控
    MonitorSessions(Vec<SessionInfo>),
    MonitorRange(usize),
    MonitorKill(String),

    // 设置
    SettingsSection(usize),
    SetTheme(ThemeMode),
    SetLanguage(Language),
    SetEditorFont(String),
    SetUiFont(String),
    SetSerif(bool),
    SetFontSize(u16),
    SetLineNumbers(bool),
    SetAutoWrap(bool),
    SetSyntax(bool),
    SetBracket(bool),
    BrowseDir(DirTarget),
    DirPicked(DirTarget, Option<String>),
    OpenExportWizard,
    ExportStep(i32),
    ExportToggle(usize, bool),
    ExportFormatPick(usize),
    ExportRun,
    ExportDone(Result<usize, String>),

    // 查询智能：补全 / 诊断 / 悬停 / 执行计划
    CompletionShow,
    CompletionHide,
    CompletionPick(usize),
    DiagnosticsReady(Vec<Diagnostic>),
    HoverWord(Option<String>),
    ExplainRun,
    ExplainReady(Result<dbhelper_models::QueryOutcome, String>),

    // 对象设计器
    DesignerOpen(ConnId, String),
    DesignerNew(ConnId),
    DesignerLoaded(Result<(TableDef, String), String>),
    DesignerTab(usize),
    DesignerColField(usize, String, String),
    DesignerAddCol,
    DesignerDelCol(usize),
    DesignerDdlField(String, String),
    DesignerSave,
    DesignerSaved(Result<String, String>),
    DesignerClose,

    // BI 仪表板
    DashOpen(ConnId),
    DashPick(usize),
    DashAddPage,
    DashAddChart(usize),
    DashChartKind(usize, ChartKind),
    DashChartSql(usize, String),
    DashChartTitle(usize, String),
    DashRun(usize),
    DashRunAll,
    DashChartResult(usize, Result<(Vec<String>, Vec<f64>), String>),
    DashDelChart(usize),
    DashDelPage(usize),
    DashParamEdit(String, String),
    DashToggleAuto,
    DashToggleTiled,

    // 迁移与同步
    TransferOpen,
    TransferModePick(usize),
    TransferSource(ConnId),
    TransferTarget(ConnId),
    TransferTable(String),
    TransferRun,
    TransferLog(String),
    TransferDone(Result<(), String>),
    TransferClose,

    // 服务器管理
    AdminOpen(ConnId),
    AdminTab(usize),
    AdminRun,
    AdminLoaded(Result<(Vec<String>, Vec<Vec<String>>), String>),
    AdminPipeline(String),
    AdminPubSubChannel(String),
    AdminPubSubToggle,
    AdminPubSubMsg(String),

    // 导入
    ImportOpen,
    ImportPickFile,
    ImportFilePicked(Option<String>),
    ImportLoaded(Result<(Vec<String>, Vec<Vec<Value>>, String), String>),
    ImportTable(String),
    ImportRun,
    ImportDone(Result<u64, String>),
    ImportClose,

    // 通用
    ToastClose(usize),
    DismissModal,
    ConfirmRun,
    CancelConfirm,
    CopyText(String),
    Noop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirTarget {
    Log,
    Config,
}

impl WinProbe {
    fn clone_probe(&self) -> (f32, f32, f32, f32) {
        (self.cursor.x, self.cursor.y, self.size.width, self.size.height)
    }
}

/// 距离窗口四边 6px 内视为缩放热区。
fn edge_of((x, y, w, h): (f32, f32, f32, f32)) -> Option<iced::window::Direction> {
    use iced::window::Direction;
    let m = 6.0;
    let left = x <= m;
    let right = x >= w - m;
    let top = y <= m;
    let bottom = y >= h - m;
    match (left, right, top, bottom) {
        (_, true, _, true) => Some(Direction::SouthEast),
        (_, true, _, _) => Some(Direction::East),
        (_, _, _, true) => Some(Direction::South),
        _ => None,
    }
}

/// 依据按下时的状态与当前光标，算出新的窗口矩形（最少 1100×700）。
fn compute_resize(st: ResizeState, cursor: iced::Point) -> (iced::Size, iced::Point) {
    use iced::window::Direction;
    const MIN_W: f32 = 1100.0;
    const MIN_H: f32 = 700.0;
    let dx = cursor.x - st.cursor.x;
    let dy = cursor.y - st.cursor.y;
    let mut x = st.pos.x;
    let mut y = st.pos.y;
    let mut w = st.size.width;
    let mut h = st.size.height;

    match st.dir {
        Direction::East | Direction::NorthEast | Direction::SouthEast => w = st.size.width + dx,
        Direction::West | Direction::NorthWest | Direction::SouthWest => {
            w = st.size.width - dx;
            x = st.pos.x + dx;
        }
        _ => {}
    }
    match st.dir {
        Direction::South | Direction::SouthEast | Direction::SouthWest => h = st.size.height + dy,
        Direction::North | Direction::NorthEast | Direction::NorthWest => {
            h = st.size.height - dy;
            y = st.pos.y + dy;
        }
        _ => {}
    }

    // 触底时反向修正位置，避免窗口跳动
    if w < MIN_W {
        if matches!(st.dir, Direction::West | Direction::NorthWest | Direction::SouthWest) {
            x -= MIN_W - w;
        }
        w = MIN_W;
    }
    if h < MIN_H {
        if matches!(st.dir, Direction::North | Direction::NorthEast | Direction::NorthWest) {
            y -= MIN_H - h;
        }
        h = MIN_H;
    }
    (iced::Size::new(w, h), iced::Point::new(x, y))
}

impl App {
    pub fn boot() -> (Self, Task<Msg>) {
        let app = Self {
            fullscreen: false,
            viewport: iced::Size::new(1440.0, 900.0),
            core: None,
            boot_error: None,
            settings: Settings::default(),
            window: iced::window::Id::unique(),
            page: Page::Connections,
            tabs: Vec::new(),
            active_tab: 0,
            conns: Vec::new(),
            groups: Vec::new(),
            expanded: HashSet::new(),
            selected_node: None,
            search: String::new(),
            tree: TreeData::default(),
            wizard: None,
            modal: Modal::None,
            toasts: Vec::new(),
            query_tabs: Vec::new(),
            data_tabs: Vec::new(),
            er: ErState::default(),
            redis: RedisState::default(),
            mongo: MongoState::default(),
            es: EsState::default(),
            monitor: MonitorState::default(),
            settings_ui: SettingsUi::default(),
            conn_ui: ConnUi::default(),
            designer: DesignerState::default(),
            dash: DashState::default(),
            transfer: TransferState::default(),
            admin: AdminState::default(),
            import: ImportState::default(),
            completion: CompletionState::default(),
            active_conn: None,
            database: None,
            metrics: Metrics::default(),
            now: dbhelper_utils::textfmt::now_secs(),
            busy: None,
            editor_focus: false,
            caret_blink: true,
            confirm: None,
            connected: HashSet::new(),
            nav_stack: Vec::new(),
            es_editor: iced::widget::text_editor::Content::with_text(
                "{\n  \"query\": { \"bool\": { \"must\": [\n    { \"match\": { \"message\": \"timeout\" } },\n    { \"range\": { \"@timestamp\": { \"gte\": \"now-15m\" } } }\n  ] } },\n  \"size\": 50,\n  \"sort\": [ { \"@timestamp\": { \"order\": \"desc\" } } ]\n}",
            ),
            mongo_editor: iced::widget::text_editor::Content::with_text(
                "[ { $match: { status: \"active\" } }, { $group: { _id: \"$role\", n: { $sum: 1 } } } ]",
            ),
            diagnostics: Vec::new(),
            hover_word: None,
        };
        let task = Task::perform(
            async {
                tracing::info!("ui: 开始核心初始化");
                let r = AppCore::boot().await;
                tracing::info!("ui: 核心初始化结束 ok={}", r.is_ok());
                r.map_err(|e| e.to_string())
            },
            Msg::Booted,
        );
        // Id::unique() 与真实窗口 id 不同，启动后取一次真实 id（关闭/最大化都依赖它）
        let win = iced::window::latest().map(Msg::WindowReady);
        (app, Task::batch([task, win]))
    }

    pub fn ctx(&self) -> Ctx {
        Ctx::new(
            if self.settings.theme == ThemeMode::Dark { theme::Tokens::DARK } else { theme::Tokens::LIGHT },
            self.settings.language,
            &self.settings.ui_font,
            &self.settings.editor_font,
            self.settings.ui_serif,
            self.settings.scale(),
        )
    }

    pub fn theme(&self) -> Theme {
        if self.settings.theme == ThemeMode::Dark {
            Theme::Dark
        } else {
            Theme::Light
        }
    }

    pub fn default_font(&self) -> Font {
        self.ctx().ui
    }

    pub fn subscription(&self) -> Subscription<Msg> {
        let mut subs = vec![iced::time::every(Duration::from_secs(1)).map(|_| Msg::Tick)];
        // 光标闪烁只在编辑器聚焦时开启，避免空闲时每 530ms 全界面重绘
        if self.editor_focus && matches!(self.modal, Modal::None) {
            subs.push(iced::time::every(Duration::from_millis(530)).map(|_| Msg::Blink));
        }
        {
            subs.push(iced::window::resize_events().map(|(_, size)| {
                if let Ok(mut p) = probe().lock() {
                    p.size = size;
                }
                Msg::ViewportResized(size)
            }));
            subs.push(iced::event::listen_with(|event, _status, _id| match event {
                iced::Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
                    if let Ok(mut p) = probe().lock() {
                        p.cursor = position;
                    }
                    let st = (*resize_state().lock().ok()?)?;
                    Some(Msg::ResizeTo(compute_resize(st, position)))
                }
                iced::Event::Mouse(iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left)) => {
                    if let Ok(mut r) = resize_state().lock() {
                        *r = None;
                    }
                    None
                }
                iced::Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left)) => {
                    let (x, y, w, h) = probe().lock().ok()?.clone_probe();
                    if let Some(dir) = edge_of((x, y, w, h)) {
                        set_resize_state(dir, iced::Point::new(x, y), iced::Size::new(w, h), iced::Point::ORIGIN);
                        return None;
                    }
                    None
                }
                _ => None,
            }));
        }
        subs.push(iced::keyboard::listen().filter_map(|event| match event {
            iced::keyboard::Event::KeyPressed { key, .. }
                if key == iced::keyboard::Key::Named(iced::keyboard::key::Named::F11) =>
            {
                Some(Msg::WindowMaximize)
            }
            _ => None,
        }));
        if self.editor_focus && matches!(self.modal, Modal::None) {
            subs.push(iced::event::listen_with(|event, _status, _id| match event {
                iced::Event::Keyboard(k) => Some(Msg::EditorKey(k)),
                iced::Event::InputMethod(iced::advanced::input_method::Event::Commit(text)) => {
                    Some(Msg::EditorIme(text))
                }
                _ => None,
            }));
        }
        Subscription::batch(subs)
    }

    pub fn current_query(&self) -> Option<&QueryTab> {
        match self.tabs.get(self.active_tab) {
            Some(tab) if tab.kind == TabKind::Query => self.query_tabs.get(tab.index),
            _ => None,
        }
    }

    pub fn current_query_mut(&mut self) -> Option<&mut QueryTab> {
        let idx = match self.tabs.get(self.active_tab) {
            Some(tab) if tab.kind == TabKind::Query => tab.index,
            _ => return None,
        };
        self.query_tabs.get_mut(idx)
    }

    pub fn current_data(&self) -> Option<&DataTab> {
        match self.tabs.get(self.active_tab) {
            Some(tab) if tab.kind == TabKind::Data => self.data_tabs.get(tab.index),
            _ => None,
        }
    }

    pub fn current_data_mut(&mut self) -> Option<&mut DataTab> {
        let idx = match self.tabs.get(self.active_tab) {
            Some(tab) if tab.kind == TabKind::Data => tab.index,
            _ => return None,
        };
        self.data_tabs.get_mut(idx)
    }

    pub fn conn_display(&self, id: ConnId) -> String {
        self.conns
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.name.clone())
            .unwrap_or_else(|| format!("连接 {id}"))
    }

    pub fn toast(&mut self, text: impl Into<String>, tone: theme::Tone) {
        self.toasts.push(Toast { text: text.into(), tone, at: std::time::Instant::now() });
        if self.toasts.len() > 3 {
            self.toasts.remove(0);
        }
    }
}
