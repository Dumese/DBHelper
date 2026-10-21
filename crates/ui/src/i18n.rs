//! 国际化（i18n）：把界面中所有静态文案抽成「按语言取字符串」的字典。
//!
//! 设计：一个轻量 [`Tr`] 结构体持有当前 [`Language`]，每个方法返回
//! `&'static str`（中文 / 英文二选一）。视图里用 `ctx.tr.xxx()` 取代硬编码中文字面量，
//! 切换 `settings.language` 后下一帧 `view` 重建 `Ctx` 时即生效。
//!
//! 仅覆盖「外壳 + 设置页」这类常驻文案；各业务页（查询 / 数据 / Redis 等）的静态标签
//! 可后续按同一模式逐步接入，本模块已预留扩展点。

use dbhelper_models::Language;

#[derive(Clone, Copy, Debug)]
pub struct Tr {
    pub lang: Language,
}

impl Tr {
    pub fn new(lang: Language) -> Self {
        Self { lang }
    }

    /// 二选一：中文 / 英文。所有文案集中在此处，便于校对与补全。
    #[inline]
    fn s(&self, zh: &'static str, en: &'static str) -> &'static str {
        match self.lang {
            Language::ZhCn => zh,
            Language::EnUs => en,
        }
    }

    // -------------------------------------------------------------------
    // 设置页
    // -------------------------------------------------------------------
    pub fn settings(&self) -> &'static str {
        self.s("设置", "Settings")
    }
    pub fn settings_sub(&self) -> &'static str {
        self.s(
            "管理 DBHelper 的外观、编辑器行为与文件位置偏好。",
            "Manage DBHelper's appearance, editor behavior and file locations.",
        )
    }
    pub fn display_config(&self) -> &'static str {
        self.s("显示配置", "Display")
    }
    pub fn normal_config(&self) -> &'static str {
        self.s("常规配置", "General")
    }
    pub fn file_config(&self) -> &'static str {
        self.s("文件配置", "Files")
    }

    pub fn color_mode(&self) -> &'static str {
        self.s("颜色模式", "Color mode")
    }
    pub fn color_mode_desc(&self) -> &'static str {
        self.s(
            "切换深色或浅色界面主题（全局生效）",
            "Switch between dark and light theme (applies globally)",
        )
    }
    pub fn light(&self) -> &'static str {
        self.s("浅色", "Light")
    }
    pub fn dark(&self) -> &'static str {
        self.s("深色", "Dark")
    }

    pub fn language(&self) -> &'static str {
        self.s("语言", "Language")
    }
    pub fn language_desc(&self) -> &'static str {
        self.s("界面显示语言", "Interface display language")
    }

    pub fn editor_font(&self) -> &'static str {
        self.s("编辑器字体", "Editor font")
    }
    pub fn editor_font_desc(&self) -> &'static str {
        self.s(
            "代码编辑区使用，仅可选等宽字体",
            "Used in the code editor; monospace only",
        )
    }
    pub fn ui_font(&self) -> &'static str {
        self.s("显示字体", "Interface font")
    }
    pub fn ui_font_desc(&self) -> &'static str {
        self.s(
            "界面组件与按钮文字的显示",
            "Font for UI components and button text",
        )
    }
    pub fn sans(&self) -> &'static str {
        self.s("无衬线", "Sans")
    }
    pub fn serif(&self) -> &'static str {
        self.s("衬线", "Serif")
    }
    pub fn font_size(&self) -> &'static str {
        self.s("字体大小", "Font size")
    }
    pub fn font_size_desc(&self) -> &'static str {
        self.s(
            "调整界面文字大小（12–18 px）",
            "Adjust interface text size (12–18 px)",
        )
    }

    pub fn show_line_numbers(&self) -> &'static str {
        self.s("显示行号", "Show line numbers")
    }
    pub fn show_line_numbers_desc(&self) -> &'static str {
        self.s("在编辑器中显示行号", "Show line numbers in the editor")
    }
    pub fn auto_wrap(&self) -> &'static str {
        self.s("自动换行", "Word wrap")
    }
    pub fn auto_wrap_desc(&self) -> &'static str {
        self.s("长行自动折叠换行", "Wrap long lines automatically")
    }
    pub fn syntax_highlight(&self) -> &'static str {
        self.s("语法高亮", "Syntax highlight")
    }
    pub fn syntax_highlight_desc(&self) -> &'static str {
        self.s(
            "为 SQL 等语句着色关键字与函数",
            "Color SQL keywords and functions",
        )
    }
    pub fn bracket_highlight(&self) -> &'static str {
        self.s("括号高亮", "Bracket highlight")
    }
    pub fn bracket_highlight_desc(&self) -> &'static str {
        self.s("匹配括号时高亮提示", "Highlight matching brackets")
    }

    pub fn log_location(&self) -> &'static str {
        self.s("日志位置", "Log location")
    }
    pub fn log_location_desc(&self) -> &'static str {
        self.s(
            "应用程序日志文件保存路径",
            "Path where application log files are saved",
        )
    }
    pub fn config_location(&self) -> &'static str {
        self.s("配置文件位置", "Config location")
    }
    pub fn config_location_desc(&self) -> &'static str {
        self.s(
            "DBHelper 配置文件（config.json）路径",
            "Path of the DBHelper config file (config.json)",
        )
    }
    pub fn export_wizard(&self) -> &'static str {
        self.s("导出向导", "Export wizard")
    }
    pub fn export_wizard_desc(&self) -> &'static str {
        self.s(
            "将连接配置、查询历史与偏好导出为文件",
            "Export connection configs, query history and preferences",
        )
    }
    pub fn open_export_wizard(&self) -> &'static str {
        self.s("打开导出向导", "Open export wizard")
    }
    pub fn browse(&self) -> &'static str {
        self.s("浏览…", "Browse…")
    }

    // -------------------------------------------------------------------
    // 外壳常驻文案（标题栏 / 面包屑 / 侧栏 / 状态栏）
    // -------------------------------------------------------------------
    pub fn connections(&self) -> &'static str {
        self.s("连接", "Connections")
    }
    pub fn all_connections(&self) -> &'static str {
        self.s("全部连接", "All connections")
    }
    pub fn conn_management(&self) -> &'static str {
        self.s("连接管理", "Connections")
    }
    pub fn query(&self) -> &'static str {
        self.s("查询", "Query")
    }
    pub fn data(&self) -> &'static str {
        self.s("数据", "Data")
    }
    pub fn design(&self) -> &'static str {
        self.s("设计", "Design")
    }
    pub fn er_diagram(&self) -> &'static str {
        self.s("ER 图", "ER Diagram")
    }
    pub fn documents(&self) -> &'static str {
        self.s("文档", "Documents")
    }
    pub fn logs(&self) -> &'static str {
        self.s("日志", "Logs")
    }
    pub fn monitor(&self) -> &'static str {
        self.s("监控", "Monitor")
    }
    pub fn server(&self) -> &'static str {
        self.s("服务器", "Server")
    }
    pub fn cache(&self) -> &'static str {
        self.s("缓存", "Cache")
    }
    pub fn table(&self) -> &'static str {
        self.s("表", "Tables")
    }
    pub fn view(&self) -> &'static str {
        self.s("视图", "Views")
    }
    pub fn key(&self) -> &'static str {
        self.s("键", "Keys")
    }
    pub fn index(&self) -> &'static str {
        self.s("索引", "Indices")
    }
    pub fn keyspace(&self) -> &'static str {
        self.s("键空间", "Keyspace")
    }
    pub fn collections(&self) -> &'static str {
        self.s("集合", "Collections")
    }

    pub fn vault_encrypted(&self) -> &'static str {
        self.s("Vault 已加密", "Vault encrypted")
    }
    pub fn initializing(&self) -> &'static str {
        self.s("初始化中…", "Initializing…")
    }
    pub fn memory(&self) -> &'static str {
        self.s("内存", "Memory")
    }
    pub fn encrypted(&self) -> &'static str {
        self.s("已加密", "Encrypted")
    }
    pub fn connections_count(&self) -> &'static str {
        self.s("个连接", "connections")
    }

    pub fn search_conns(&self) -> &'static str {
        self.s("搜索连接、表、字段", "Search connections, tables, fields")
    }
    pub fn search_keys(&self) -> &'static str {
        self.s("匹配键名 *profile", "Match key name *profile")
    }
}
