//! 新功能域的 update 处理：查询智能 / 对象设计器 / BI 仪表板 / 迁移同步 / 服务器管理 / 导入。
//!
//! 从 update.rs 主 match 以 catch-all 形式进入，避免主文件继续膨胀。

use crate::state::*;
use crate::theme::Tone;
use crate::{App, Msg};
use dbhelper_models::{ColumnDef, QueryOutcome, TableDef, Value};
use iced::Task;

impl App {
    pub fn update_features(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            // -----------------------------------------------------------------
            // 查询智能
            // -----------------------------------------------------------------
            Msg::CompletionShow => {
                let word = self.word_at_caret();
                let mut items: Vec<String> = Vec::new();
                // 1) 连接树里所有表名
                for objs in self.tree.tables.values().chain(self.tree.views.values()) {
                    for o in objs {
                        if word.is_empty() || o.name.to_lowercase().contains(&word.to_lowercase()) {
                            items.push(o.name.clone());
                        }
                    }
                }
                // 2) 当前数据页已加载的列（tbl.col 形式）
                if let Some(dt) = self.current_data() {
                    if let Some(def) = &dt.def {
                        for c in &def.columns {
                            let full = format!("{}.{}", dt.table, c.name);
                            if word.is_empty() || full.to_lowercase().contains(&word.to_lowercase()) {
                                items.push(full);
                            }
                            if word.is_empty() || c.name.to_lowercase().contains(&word.to_lowercase()) {
                                items.push(c.name.clone());
                            }
                        }
                    }
                }
                // 3) SQL 关键字
                for kw in [
                    "SELECT", "FROM", "WHERE", "GROUP BY", "ORDER BY", "HAVING", "JOIN",
                    "LEFT JOIN", "INNER JOIN", "LIMIT", "INSERT INTO", "UPDATE", "DELETE FROM",
                    "CREATE TABLE", "ALTER TABLE", "DROP TABLE", "COUNT", "SUM", "AVG", "MAX", "MIN",
                ] {
                    if word.is_empty() || kw.to_lowercase().starts_with(&word.to_lowercase()) {
                        items.push(kw.to_string());
                    }
                }
                items.truncate(30);
                self.completion.items = items;
                self.completion.visible = !self.completion.items.is_empty();
                self.completion.selected = 0;
                Task::none()
            }
            Msg::CompletionHide => {
                self.completion.visible = false;
                Task::none()
            }
            Msg::CompletionPick(i) => {
                if let Some(item) = self.completion.items.get(i).cloned() {
                    if let Some(tab) = self.current_query_mut() {
                        let word = Self::word_range(&tab.sql, tab.caret);
                        tab.sql.replace_range(word.0..word.1, &item);
                        tab.caret = word.0 + item.len();
                        tab.anchor = tab.caret;
                    }
                }
                self.completion.visible = false;
                let _ = self.run_diagnostics();
                Task::none()
            }
            Msg::DiagnosticsReady(diags) => {
                self.diagnostics = diags;
                Task::none()
            }
            Msg::HoverWord(w) => {
                self.hover_word = w;
                Task::none()
            }
            Msg::ExplainRun => {
                let Some(core) = self.core.clone() else { return Task::none() };
                let Some(tab) = self.current_query() else { return Task::none() };
                let sql = tab.sql.clone();
                let conn = tab.conn;
                if sql.trim().is_empty() {
                    return Task::none();
                }
                self.toast("正在生成执行计划…", Tone::Neutral);
                Task::perform(
                    async move { core.explain(conn, sql).await.map_err(|e| e.to_string()) },
                    Msg::ExplainReady,
                )
            }
            Msg::ExplainReady(result) => {
                if let Some(tab) = self.current_query_mut() {
                    match result {
                        Ok(outcome) => {
                            tab.plan_columns = outcome.columns.iter().map(|c| c.name.clone()).collect();
                            tab.plan_rows = outcome
                                .rows
                                .iter()
                                .map(|r| r.iter().map(|v| v.display()).collect())
                                .collect();
                            tab.error = None;
                            self.toast("执行计划已生成", Tone::Success);
                        }
                        Err(e) => {
                            tab.error = Some(format!("EXPLAIN 失败：{e}"));
                            self.toast("执行计划生成失败", Tone::Error);
                        }
                    }
                }
                Task::none()
            }

            // -----------------------------------------------------------------
            // 对象设计器
            // -----------------------------------------------------------------
            Msg::DesignerOpen(conn, table) => {
                self.designer = DesignerState {
                    open: true,
                    conn: Some(conn),
                    table: table.clone(),
                    is_new: false,
                    ..Default::default()
                };
                self.page = Page::Designer;
                let Some(core) = self.core.clone() else { return Task::none() };
                Task::perform(
                    async move {
                        let def = core.describe_table(conn, table.clone()).await.map_err(|e| e.to_string())?;
                        let ddl = core.table_ddl(conn, table).await.unwrap_or_default();
                        Ok((def, ddl))
                    },
                    Msg::DesignerLoaded,
                )
            }
            Msg::DesignerNew(conn) => {
                let mut def = TableDef {
                    schema: None,
                    name: "new_table".into(),
                    columns: vec![
                        ColumnDef {
                            name: "id".into(),
                            data_type: "BIGINT".into(),
                            nullable: false,
                            default: None,
                            is_pk: true,
                            is_unique: true,
                            fk: None,
                            extra: "AUTO_INCREMENT".into(),
                            comment: None,
                            ordinal: 1,
                        },
                    ],
                    indexes: Vec::new(),
                    row_estimate: None,
                    comment: None,
                };
                if let Some(first) = def.columns.first_mut() {
                    first.extra = String::new();
                }
                self.designer = DesignerState {
                    open: true,
                    conn: Some(conn),
                    table: def.name.clone(),
                    is_new: true,
                    def: Some(def.clone()),
                    original: None,
                    ..Default::default()
                };
                self.page = Page::Designer;
                Task::none()
            }
            Msg::DesignerLoaded(result) => {
                match result {
                    Ok((def, ddl)) => {
                        self.designer.table = def.name.clone();
                        self.designer.original = Some(def.clone());
                        self.designer.def = Some(def);
                        self.designer.ddl_preview = ddl;
                        self.designer.error = None;
                    }
                    Err(e) => self.designer.error = Some(e),
                }
                Task::none()
            }
            Msg::DesignerTab(i) => {
                self.designer.tab = i.min(3);
                Task::none()
            }
            Msg::DesignerColField(idx, field, value) => {
                if let Some(def) = &mut self.designer.def {
                    if let Some(col) = def.columns.get_mut(idx) {
                        match field.as_str() {
                            "name" => col.name = value,
                            "type" => col.data_type = value,
                            "default" => {
                                col.default = if value.trim().is_empty() { None } else { Some(value) }
                            }
                            "comment" => {
                                col.comment = if value.trim().is_empty() { None } else { Some(value) }
                            }
                            "nullable" => col.nullable = value == "true",
                            "pk" => col.is_pk = value == "true",
                            _ => {}
                        }
                    }
                }
                Task::none()
            }
            Msg::DesignerAddCol => {
                if let Some(def) = &mut self.designer.def {
                    let n = def.columns.len() as i64 + 1;
                    def.columns.push(ColumnDef {
                        name: format!("col_{n}"),
                        data_type: "VARCHAR(255)".into(),
                        nullable: true,
                        default: None,
                        is_pk: false,
                        is_unique: false,
                        fk: None,
                        extra: String::new(),
                        comment: None,
                        ordinal: n,
                    });
                }
                Task::none()
            }
            Msg::DesignerDelCol(idx) => {
                if let Some(def) = &mut self.designer.def {
                    if def.columns.len() > 1 && idx < def.columns.len() {
                        def.columns.remove(idx);
                    }
                }
                Task::none()
            }
            Msg::DesignerDdlField(which, value) => {
                match which.as_str() {
                    "index" => self.designer.index_ddl = value,
                    "trigger" => self.designer.trigger_ddl = value,
                    _ => {}
                }
                Task::none()
            }
            Msg::DesignerSave => {
                let (Some(core), Some(conn), Some(def)) =
                    (self.core.clone(), self.designer.conn, self.designer.def.clone())
                else {
                    return Task::none();
                };
                self.designer.saving = true;
                self.designer.error = None;
                self.designer.saved = None;
                let orig = self.designer.original.clone();
                let extra_index = self.designer.index_ddl.clone();
                let extra_trigger = self.designer.trigger_ddl.clone();
                Task::perform(
                    async move {
                        let mut out = core.def_save_table(conn, def, orig).await.map_err(|e| e.to_string())?;
                        for stmt in extra_index.split(';').chain(extra_trigger.split(';')) {
                            let s = stmt.trim();
                            if s.is_empty() {
                                continue;
                            }
                            if let Err(e) = core.exec_ddl(conn, s.to_string()).await {
                                return Err(format!("附加 DDL 执行失败：{}", e));
                            }
                            out.push_str(s);
                            out.push_str(";\n");
                        }
                        Ok(out)
                    },
                    Msg::DesignerSaved,
                )
            }
            Msg::DesignerSaved(result) => {
                self.designer.saving = false;
                match result {
                    Ok(ddl) => {
                        self.designer.saved = Some(ddl);
                        self.designer.original = self.designer.def.clone();
                        self.toast("表结构已保存", Tone::Success);
                    }
                    Err(e) => {
                        self.designer.error = Some(e.clone());
                        self.toast(format!("保存失败：{e}"), Tone::Error);
                    }
                }
                Task::none()
            }
            Msg::DesignerClose => {
                self.designer.open = false;
                Task::none()
            }

            // -----------------------------------------------------------------
            // BI 仪表板
            // -----------------------------------------------------------------
            Msg::DashOpen(conn) => {
                if !self.conns.iter().any(|c| c.id == conn) {
                    self.toast("请先选择一个连接", Tone::Warning);
                    return Task::none();
                }
                self.dash.conn = Some(conn);
                self.page = Page::Dash;
                Task::none()
            }
            Msg::DashPick(i) => {
                if i < self.dash.pages.len() {
                    self.dash.active = i;
                }
                Task::none()
            }
            Msg::DashAddPage => {
                self.dash.pages.push(Dashboard {
                    name: format!("仪表板 {}", self.dash.pages.len() + 1),
                    charts: vec![ChartCard::default()],
                });
                self.dash.active = self.dash.pages.len() - 1;
                Task::none()
            }
            Msg::DashAddChart(page) => {
                if let Some(p) = self.dash.pages.get_mut(page) {
                    p.charts.push(ChartCard::default());
                }
                Task::none()
            }
            Msg::DashChartKind(idx, kind) => {
                if let Some(p) = self.dash.pages.get_mut(self.dash.active) {
                    if let Some(c) = p.charts.get_mut(idx) {
                        c.kind = kind;
                    }
                }
                Task::none()
            }
            Msg::DashChartSql(idx, sql) => {
                if let Some(p) = self.dash.pages.get_mut(self.dash.active) {
                    if let Some(c) = p.charts.get_mut(idx) {
                        c.sql = sql;
                    }
                }
                Task::none()
            }
            Msg::DashChartTitle(idx, title) => {
                if let Some(p) = self.dash.pages.get_mut(self.dash.active) {
                    if let Some(c) = p.charts.get_mut(idx) {
                        c.title = title;
                    }
                }
                Task::none()
            }
            Msg::DashRun(idx) => self.run_chart(idx),
            Msg::DashRunAll => {
                let n = self
                    .dash
                    .pages
                    .get(self.dash.active)
                    .map(|p| p.charts.len())
                    .unwrap_or(0);
                self.dash.last_refresh = dbhelper_utils::textfmt::now_secs();
                let mut tasks = Vec::new();
                for i in 0..n {
                    tasks.push(self.run_chart(i));
                }
                Task::batch(tasks)
            }
            Msg::DashChartResult(idx, result) => {
                if let Some(p) = self.dash.pages.get_mut(self.dash.active) {
                    if let Some(c) = p.charts.get_mut(idx) {
                        c.running = false;
                        match result {
                            Ok((labels, values)) => {
                                c.labels = labels;
                                c.values = values;
                                c.error = None;
                            }
                            Err(e) => c.error = Some(e),
                        }
                    }
                }
                Task::none()
            }
            Msg::DashDelChart(idx) => {
                if let Some(p) = self.dash.pages.get_mut(self.dash.active) {
                    if idx < p.charts.len() {
                        p.charts.remove(idx);
                    }
                }
                Task::none()
            }
            Msg::DashDelPage(idx) => {
                if self.dash.pages.len() > 1 && idx < self.dash.pages.len() {
                    self.dash.pages.remove(idx);
                    self.dash.active = self.dash.active.min(self.dash.pages.len() - 1);
                }
                Task::none()
            }
            Msg::DashParamEdit(name, value) => {
                if name.is_empty() {
                    return Task::none();
                }
                match self.dash.params.iter_mut().find(|(n, _)| *n == name) {
                    Some(p) => p.1 = value,
                    None => self.dash.params.push((name, value)),
                }
                Task::none()
            }
            Msg::DashToggleAuto => {
                self.dash.auto_refresh = !self.dash.auto_refresh;
                Task::none()
            }
            Msg::DashToggleTiled => {
                self.dash.tiled = !self.dash.tiled;
                Task::none()
            }

            // -----------------------------------------------------------------
            // 迁移与同步
            // -----------------------------------------------------------------
            Msg::TransferOpen => {
                self.transfer.open = true;
                self.page = Page::Transfer;
                Task::none()
            }
            Msg::TransferModePick(i) => {
                if let Some(m) = TransferMode::ALL.get(i) {
                    self.transfer.mode = *m;
                }
                Task::none()
            }
            Msg::TransferSource(c) => {
                self.transfer.source_conn = Some(c);
                Task::none()
            }
            Msg::TransferTarget(c) => {
                self.transfer.target_conn = Some(c);
                Task::none()
            }
            Msg::TransferTable(t) => {
                self.transfer.table = t;
                Task::none()
            }
            Msg::TransferRun => self.run_transfer(),
            Msg::TransferLog(line) => {
                if line.is_empty() {
                    self.transfer.log.clear();
                } else {
                    self.transfer.log.push(line);
                }
                Task::none()
            }
            Msg::TransferDone(result) => {
                self.transfer.running = false;
                match result {
                    Ok(()) => {
                        self.transfer.log.push("✓ 迁移完成".into());
                        self.toast("迁移完成", Tone::Success);
                    }
                    Err(e) => {
                        self.transfer.error = Some(e.clone());
                        self.transfer.log.push(format!("✗ {e}"));
                        self.toast(format!("迁移失败：{e}"), Tone::Error);
                    }
                }
                Task::none()
            }
            Msg::TransferClose => {
                self.transfer.open = false;
                Task::none()
            }

            // -----------------------------------------------------------------
            // 服务器管理
            // -----------------------------------------------------------------
            Msg::AdminOpen(conn) => {
                if !self.conns.iter().any(|c| c.id == conn) {
                    self.toast("请先选择一个连接", Tone::Warning);
                    return Task::none();
                }
                self.admin.conn = Some(conn);
                self.page = Page::Admin;
                Task::none()
            }
            Msg::AdminTab(i) => {
                self.admin.tab = i.min(4);
                self.admin.rows.clear();
                self.admin.columns.clear();
                Task::none()
            }
            Msg::AdminRun => self.run_admin(),
            Msg::AdminLoaded(result) => {
                self.admin.running = false;
                match result {
                    Ok((cols, rows)) => {
                        self.admin.columns = cols;
                        self.admin.rows = rows;
                        self.admin.error = None;
                    }
                    Err(e) => self.admin.error = Some(e),
                }
                Task::none()
            }
            Msg::AdminPipeline(s) => {
                self.admin.pipeline = s;
                Task::none()
            }
            Msg::AdminPubSubChannel(s) => {
                self.admin.pubsub_channel = s;
                Task::none()
            }
            Msg::AdminPubSubToggle => {
                self.admin.pubsub_live = !self.admin.pubsub_live;
                if !self.admin.pubsub_live {
                    self.admin.pubsub_messages.push("— 已停止订阅 —".into());
                }
                Task::none()
            }
            Msg::AdminPubSubMsg(m) => {
                self.admin.pubsub_messages.push(m);
                if self.admin.pubsub_messages.len() > 200 {
                    self.admin.pubsub_messages.remove(0);
                }
                Task::none()
            }

            // -----------------------------------------------------------------
            // 导入
            // -----------------------------------------------------------------
            Msg::ImportOpen => {
                // 打开导入向导弹窗；选文件是弹窗内的独立动作
                self.import.open = true;
                self.modal = crate::state::Modal::Import;
                Task::none()
            }
            Msg::ImportPickFile => {
                let Some(core) = self.core.clone() else { return Task::none() };
                Task::perform(
                    async move {
                        let path = rfd::AsyncFileDialog::new()
                            .add_filter("数据文件", &["csv", "json", "xlsx", "tsv"])
                            .pick_file()
                            .await
                            .map(|f| f.path().to_string_lossy().to_string())
                            .ok_or("已取消")?;
                        let (columns, rows) = dbhelper_core::export::read_import_file(path.clone()).await?;
                        Ok((columns, rows, path))
                    },
                    Msg::ImportLoaded,
                )
            }
            Msg::ImportFilePicked(Some(path)) => {
                self.import.path = path.clone();
                Task::perform(
                    async move {
                        let (columns, rows) = dbhelper_core::export::read_import_file(path.clone()).await?;
                        Ok((columns, rows, path))
                    },
                    Msg::ImportLoaded,
                )
            }
            Msg::ImportFilePicked(None) => Task::none(),
            Msg::ImportLoaded(result) => match result {
                Ok((columns, rows, path)) => {
                    self.import.path = path;
                    self.import.columns = columns;
                    self.import.preview = rows.into_iter().take(100).collect();
                    self.import.error = None;
                    Task::none()
                }
                Err(e) => {
                    self.import.error = Some(format!("读取失败：{e}"));
                    Task::none()
                }
            },
            Msg::ImportTable(t) => {
                self.import.table = t;
                Task::none()
            }
            Msg::ImportRun => {
                let (Some(core), Some(conn)) = (self.core.clone(), self.active_conn) else {
                    self.import.error = Some("请先选择连接".into());
                    return Task::none();
                };
                if self.import.table.trim().is_empty() {
                    self.import.error = Some("请填写目标表名".into());
                    return Task::none();
                }
                self.import.running = true;
                let table = self.import.table.trim().to_string();
                let columns = self.import.columns.clone();
                let rows: Vec<Vec<Value>> = self.import.preview.clone();
                Task::perform(
                    async move {
                        core.import_rows(conn, table, columns, rows)
                            .await
                            .map_err(|e| e.to_string())
                    },
                    Msg::ImportDone,
                )
            }
            Msg::ImportDone(result) => {
                self.import.running = false;
                match result {
                    Ok(n) => {
                        self.import.result = Some(format!("已导入 {n} 行"));
                        self.toast(format!("导入完成：{n} 行"), Tone::Success);
                    }
                    Err(e) => {
                        self.import.error = Some(e.clone());
                        self.toast(format!("导入失败：{e}"), Tone::Error);
                    }
                }
                Task::none()
            }
            Msg::ImportClose => {
                self.import.open = false;
                if matches!(self.modal, crate::state::Modal::Import) {
                    self.modal = crate::state::Modal::None;
                }
                Task::none()
            }

            // -----------------------------------------------------------------
            // 数据编辑增强：视图切换 / 脱敏 / 批量编辑
            // -----------------------------------------------------------------
            Msg::DataViewMode(mode) => {
                if let Some(tab) = self.current_data_mut() {
                    tab.view_mode = mode.min(3);
                }
                Task::none()
            }
            Msg::DataMaskToggle => {
                if let Some(tab) = self.current_data_mut() {
                    tab.mask = !tab.mask;
                }
                Task::none()
            }
            Msg::DataBatchField(f) => {
                if let Some(tab) = self.current_data_mut() {
                    tab.batch_field = f;
                }
                Task::none()
            }
            Msg::DataBatchValue(v) => {
                if let Some(tab) = self.current_data_mut() {
                    tab.batch_value = v;
                }
                Task::none()
            }
            Msg::DataBatchApply => {
                let Some(tab) = self.current_data_mut() else { return Task::none() };
                if tab.batch_field.trim().is_empty() {
                    self.toast("请先选择要批量设置的列", Tone::Warning);
                    return Task::none();
                }
                let col_name = tab.batch_field.trim().to_string();
                let value = tab.batch_value.clone();
                let Some(col_idx) = tab.columns.iter().position(|c| c.name == col_name) else {
                    self.toast(format!("列不存在：{col_name}"), Tone::Warning);
                    return Task::none();
                };
                let n = tab.rows.len();
                for row in &mut tab.rows {
                    if let Some(cell) = row.get_mut(col_idx) {
                        *cell = if value.is_empty() {
                            Value::Null
                        } else {
                            Value::Text(value.clone())
                        };
                    }
                }
                tab.dirty = tab.dirty.saturating_add(n);
                self.toast(format!("已批量设置 {n} 行的 {col_name}，记得提交"), Tone::Success);
                Task::none()
            }

            Msg::Noop => Task::none(),
            _ => Task::none(),
        }
    }

    // -----------------------------------------------------------------------
    // 辅助
    // -----------------------------------------------------------------------

    fn word_at_caret(&self) -> String {
        let Some(tab) = self.current_query() else { return String::new() };
        Self::word_range(&tab.sql, tab.caret).2
    }

    /// 返回 (start, end, word)：光标处的词边界。
    fn word_range(sql: &str, caret: usize) -> (usize, usize, String) {
        let bytes = sql.as_bytes();
        let caret = caret.min(bytes.len());
        let mut start = caret;
        while start > 0 {
            let b = bytes[start - 1];
            if b.is_ascii_alphanumeric() || b == b'_' || b == b'.' {
                start -= 1;
            } else {
                break;
            }
        }
        let mut end = caret;
        while end < bytes.len() {
            let b = bytes[end];
            if b.is_ascii_alphanumeric() || b == b'_' || b == b'.' {
                end += 1;
            } else {
                break;
            }
        }
        (start, end, sql[start..end].to_string())
    }

    /// 轻量 SQL 诊断：括号/引号平衡 + 高危语句提示。
    ///
    /// 逻辑实现在 `dbhelper_utils::sql::diagnose`（纯函数，已被单元测试覆盖），
    /// 这里只做状态同步。
    pub fn run_diagnostics(&mut self) -> Task<Msg> {
        let Some(tab) = self.current_query() else { return Task::none() };
        let sql = tab.sql.clone();
        self.diagnostics = dbhelper_utils::sql::diagnose(&sql)
            .into_iter()
            .map(|d| Diagnostic { line: d.line, message: d.message })
            .collect();
        Task::none()
    }

    /// 运行单个图表查询：首列文本作类目、首个数值列作值。
    fn run_chart(&mut self, idx: usize) -> Task<Msg> {
        let Some(core) = self.core.clone() else { return Task::none() };
        let Some(conn) = self.dash.conn else { return Task::none() };
        let Some(sql) = self
            .dash
            .pages
            .get(self.dash.active)
            .and_then(|p| p.charts.get(idx))
            .map(|c| c.sql.clone())
        else {
            return Task::none();
        };
        // 参数替换 :name -> value
        let mut sql = sql;
        for (name, value) in &self.dash.params {
            sql = sql.replace(&format!(":{name}"), value);
        }
        if let Some(p) = self.dash.pages.get_mut(self.dash.active) {
            if let Some(c) = p.charts.get_mut(idx) {
                c.running = true;
                c.error = None;
            }
        }
        Task::perform(
            async move {
                let out: QueryOutcome = core.admin_query(conn, sql).await.map_err(|e| e.to_string())?;
                let labels: Vec<String> = out
                    .rows
                    .iter()
                    .map(|r| r.first().map(|v| v.display()).unwrap_or_default())
                    .collect();
                // 数值列：跳过第 0 列（类目列），且要求至少有一个非空值，
                // 否则「全 NULL 列」会被误判为数值列。
                let numeric = (1..out.columns.len())
                    .find(|&i| {
                        let vals: Vec<&Value> = out
                            .rows
                            .iter()
                            .filter_map(|r| r.get(i))
                            .filter(|v| !v.is_null())
                            .collect();
                        !vals.is_empty()
                            && vals.iter().all(|v| {
                                matches!(v, Value::Int(_) | Value::Float(_) | Value::Decimal(_))
                            })
                    })
                    .unwrap_or_else(|| {
                        // 退化：取最后一列
                        out.columns.len().saturating_sub(1)
                    });
                let values: Vec<f64> = out
                    .rows
                    .iter()
                    .map(|r| match r.get(numeric) {
                        Some(Value::Int(v)) => *v as f64,
                        Some(Value::Float(v)) => *v,
                        Some(Value::Decimal(s)) => s.parse().unwrap_or(0.0),
                        _ => 0.0,
                    })
                    .collect();
                Ok((labels, values))
            },
            move |r| Msg::DashChartResult(idx, r),
        )
    }

    /// 迁移执行：结构同步走 DDL 转移，数据/同步/复制走 SELECT → 批量写入。
    fn run_transfer(&mut self) -> Task<Msg> {
        let Some(core) = self.core.clone() else { return Task::none() };
        let (Some(src), Some(dst)) = (self.transfer.source_conn, self.transfer.target_conn) else {
            self.transfer.error = Some("请选择源连接与目标连接".into());
            return Task::none();
        };
        if src == dst {
            self.transfer.error = Some("源与目标不能是同一连接".into());
            return Task::none();
        }
        let table = self.transfer.table.trim().to_string();
        if table.is_empty() {
            self.transfer.error = Some("请填写要迁移的表名".into());
            return Task::none();
        }
        self.transfer.running = true;
        self.transfer.error = None;
        self.transfer.log.push(format!("→ 开始{}：{table}", self.transfer.mode.label()));
        let mode = self.transfer.mode;
        Task::perform(
            async move {
                match mode {
                    TransferMode::Structure => {
                        let ddl = core.table_ddl(src, table.clone()).await.map_err(|e| e.to_string())?;
                        // DDL 必须走 execute（admin_query 面向结果集查询）
                        core.exec_ddl(dst, ddl).await.map_err(|e| e.to_string())?;
                        Ok(())
                    }
                    TransferMode::Data | TransferMode::Copy | TransferMode::Sync => {
                        if mode != TransferMode::Data {
                            // Copy / Sync 也带结构
                            if let Ok(ddl) = core.table_ddl(src, table.clone()).await {
                                let _ = core.exec_ddl(dst, ddl).await;
                            }
                        }
                        // 表名做标识符转义，避免拼进 SQL 时被注入
                        let quoted = dbhelper_utils::sql::quote_table_name(table.as_str());
                        let out = core
                            .admin_query(src, format!("SELECT * FROM {quoted}"))
                            .await
                            .map_err(|e| e.to_string())?;
                        let columns: Vec<String> = out.columns.iter().map(|c| c.name.clone()).collect();
                        if out.rows.is_empty() {
                            return Ok(());
                        }
                        core.import_rows(dst, table, columns, out.rows)
                            .await
                            .map_err(|e| e.to_string())?;
                        Ok(())
                    }
                }
            },
            Msg::TransferDone,
        )
    }

    /// 服务器管理查询：按页签分发方言 SQL。
    fn run_admin(&mut self) -> Task<Msg> {
        let Some(core) = self.core.clone() else { return Task::none() };
        let Some(conn) = self.admin.conn else { return Task::none() };
        let kind = self
            .conns
            .iter()
            .find(|c| c.id == conn)
            .map(|c| c.kind)
            .unwrap_or(dbhelper_models::DbKind::Mysql);
        self.admin.running = true;
        self.admin.error = None;
        let tab = self.admin.tab;
        let pipeline = self.admin.pipeline.clone();
        let channel = self.admin.pubsub_channel.clone();

        // Pub-Sub：Redis 专用轮询 PUBSUB CHANNELS/NUMSUB
        if tab == 3 {
            if kind != dbhelper_models::DbKind::Redis {
                self.admin.running = false;
                self.admin.error = Some("Pub-Sub 仅适用于 Redis 连接".into());
                return Task::none();
            }
            let live = self.admin.pubsub_live;
            return Task::perform(
                async move {
                    let handle = core.ensure_connected(conn).await.map_err(|e| e.to_string())?;
                    let redis = handle.redis().ok_or("非 Redis 连接")?;
                    let pattern = if channel.trim().is_empty() {
                        vec!["PUBSUB".to_string(), "CHANNELS".to_string()]
                    } else {
                        vec!["PUBSUB".to_string(), "CHANNELS".to_string(), channel.clone()]
                    };
                    let channels = redis.raw_command(&pattern).await.map_err(|e| e.to_string())?;
                    let mut rows = vec![vec!["频道".to_string(), channels.to_string()]];
                    let numsub = redis
                        .raw_command(&["PUBSUB".to_string(), "NUMSUB".to_string(), channel.clone()])
                        .await
                        .map(|j| j.to_string())
                        .unwrap_or_default();
                    rows.push(vec!["订阅数".to_string(), numsub]);
                    rows.push(vec!["活跃状态".to_string(), if live { "订阅中" } else { "已停止" }.to_string()]);
                    Ok((vec!["项目".to_string(), "值".to_string()], rows))
                },
                Msg::AdminLoaded,
            );
        }

        let sql = match tab {
            0 => match kind {
                dbhelper_models::DbKind::Mysql => {
                    "SELECT user, host, Select_priv, Insert_priv, Update_priv, Delete_priv, Create_priv, Drop_priv FROM mysql.user".to_string()
                }
                dbhelper_models::DbKind::Postgres => {
                    "SELECT usename AS user, usesuper AS super, usecreatedb AS create_db, usecatupd AS catalog FROM pg_user".to_string()
                }
                dbhelper_models::DbKind::SqlServer => {
                    "SELECT name, type_desc, is_disabled FROM sys.sql_logins".to_string()
                }
                dbhelper_models::DbKind::Oracle => {
                    "SELECT username, account_status, created FROM all_users".to_string()
                }
                dbhelper_models::DbKind::Sqlite => {
                    return Task::perform(
                        async { Ok((vec!["提示".into()], vec![vec!["SQLite 无用户体系".to_string()]])) },
                        Msg::AdminLoaded,
                    );
                }
                _ => "SELECT 1".to_string(),
            },
            1 => match kind {
                dbhelper_models::DbKind::Mysql => {
                    "SELECT id, user, host, db, command, time, state, LEFT(info, 120) AS info FROM information_schema.processlist".to_string()
                }
                dbhelper_models::DbKind::Postgres => {
                    "SELECT pid, usename, state, LEFT(query, 120) AS query, now() - query_start AS duration FROM pg_stat_activity".to_string()
                }
                dbhelper_models::DbKind::SqlServer => {
                    "SELECT session_id, status, command, wait_type, LEFT(text, 120) AS text FROM sys.dm_exec_requests r CROSS APPLY sys.dm_exec_sql_text(r.sql_handle)".to_string()
                }
                dbhelper_models::DbKind::Oracle => {
                    "SELECT sid, serial#, username, status, sql_text FROM v$session s, v$sql q WHERE s.sql_id = q.sql_id".to_string()
                }
                _ => "SELECT 1".to_string(),
            },
            2 => {
                // 聚合管道：Mongo 专用；其它类型退回普通查询
                if kind == dbhelper_models::DbKind::Mongodb {
                    let pipeline_str = if pipeline.trim().is_empty() { "[]".to_string() } else { pipeline.clone() };
                    return Task::perform(
                        async move {
                            let handle = core.ensure_connected(conn).await.map_err(|e| e.to_string())?;
                            let mongo = handle.mongo().ok_or("非 MongoDB 连接")?;
                            let db = "test";
                            let pipeline_val: serde_json::Value =
                                serde_json::from_str(&pipeline_str).unwrap_or(serde_json::json!([]));
                            let docs = mongo.aggregate(db, "collection", pipeline_val).await.map_err(|e| e.to_string())?;
                            let rows: Vec<Vec<String>> = docs
                                .iter()
                                .map(|d| vec![serde_json::to_string_pretty(d).unwrap_or_default()])
                                .collect();
                            Ok((vec!["聚合结果".into()], rows))
                        },
                        Msg::AdminLoaded,
                    );
                }
                if pipeline.trim().is_empty() {
                    return Task::perform(
                        async { Ok((vec!["提示".into()], vec![vec!["请输入要执行的查询/管道".to_string()]])) },
                        Msg::AdminLoaded,
                    );
                }
                pipeline.clone()
            }
            // 4 模式分析
            _ => match kind {
                dbhelper_models::DbKind::Mysql => {
                    "SELECT table_schema, table_name, table_rows, ROUND(data_length/1024/1024,2) AS data_mb, ROUND(index_length/1024/1024,2) AS index_mb FROM information_schema.tables ORDER BY data_length DESC LIMIT 50".to_string()
                }
                dbhelper_models::DbKind::Postgres => {
                    "SELECT schemaname, relname, n_live_tup, pg_size_pretty(pg_total_relation_size(relid)) AS total FROM pg_stat_user_tables ORDER BY pg_total_relation_size(relid) DESC LIMIT 50".to_string()
                }
                dbhelper_models::DbKind::SqlServer => {
                    "SELECT TOP 50 s.name AS schema_name, t.name AS table_name, p.rows, SUM(a.total_pages) * 8 / 1024 AS mb FROM sys.tables t JOIN sys.schemas s ON t.schema_id = s.schema_id JOIN sys.indexes i ON t.object_id = i.object_id JOIN sys.partitions p ON i.object_id = p.object_id AND i.index_id = p.index_id JOIN sys.allocation_units a ON p.partition_id = a.container_id GROUP BY s.name, t.name, p.rows ORDER BY mb DESC".to_string()
                }
                dbhelper_models::DbKind::Sqlite => {
                    "SELECT name AS table_name FROM sqlite_master WHERE type='table' ORDER BY name".to_string()
                }
                _ => "SELECT 1".to_string(),
            },
        };

        Task::perform(
            async move {
                let out = core.admin_query(conn, sql).await.map_err(|e| e.to_string())?;
                let cols = out.columns.iter().map(|c| c.name.clone()).collect();
                let rows = out
                    .rows
                    .iter()
                    .map(|r| r.iter().map(|v| v.display()).collect())
                    .collect();
                Ok((cols, rows))
            },
            Msg::AdminLoaded,
        )
    }
}
