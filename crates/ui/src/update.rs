//! 状态迁移：所有 `Msg` 的唯一处理入口。

use crate::kit::{BtnKind, Ctx};
use crate::state::*;
use crate::theme::{Tone, Tokens};
use crate::widgets::editor::{apply_edit, selected_text, EditOp};
use crate::{App, DirTarget, Msg};
use dbhelper_core::registry::Handle;
use dbhelper_models::*;
use dbhelper_utils::sql as sqlutil;
use iced::Task;
use std::collections::HashSet;
use std::time::{Duration, Instant};

impl App {
    pub fn update(&mut self, msg: Msg) -> Task<Msg> {
        let t0 = Instant::now();
        let task = self.update_inner(msg);
        crate::perf::record_update(t0.elapsed());
        task
    }

    fn update_inner(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::Booted(result) => self.on_booted(result),
            Msg::Tick => {
                self.now = dbhelper_utils::textfmt::now_secs();
                if let Some(core) = &self.core {
                    self.metrics = core.snapshot_metrics();
                }
                self.toasts.retain(|t| t.at.elapsed() < Duration::from_secs(4));
                self.refresh_live_metrics()
            }
            Msg::Blink => {
                self.caret_blink = !self.caret_blink;
                Task::none()
            }
            Msg::WindowDrag => iced::window::drag(self.window),
            Msg::WindowMinimize => iced::window::minimize(self.window, true),
            Msg::ViewportResized(size) => {
                self.viewport = size;
                Task::none()
            }
            Msg::WindowReady(id) => {
                if let Some(id) = id {
                    self.window = id;
                }
                Task::none()
            }
            Msg::WindowMaximize => {
                // 无边框窗口：直接调用 Win32 ShowWindow，最大化按工作区（不盖任务栏）
                self.fullscreen = !self.fullscreen;
                let maximize = self.fullscreen;
                maximize_window(self.window, maximize)
            }
            Msg::WindowResize(dir) => {
                // 记录按下时的窗口矩形，随后由 CursorMoved 驱动真正的缩放
                let (cursor, size) = crate::probe_snapshot();
                iced::window::position(self.window)
                    .map(move |pos| Msg::ResizeReady(dir, cursor, size, pos))
            }
            Msg::ResizeReady(dir, cursor, size, pos) => {
                crate::set_resize_state(dir, cursor, size, pos.unwrap_or(iced::Point::ORIGIN));
                Task::none()
            }
            Msg::ResizeTo((size, pos)) => Task::batch([
                iced::window::resize(self.window, size),
                iced::window::move_to(self.window, pos),
            ]),
            Msg::WindowClose => {
                tracing::info!("ui: 请求关闭窗口");
                iced::window::close(self.window)
            }

            Msg::SelectTab(i) => {
                self.active_tab = i;
                if let Some(tab) = self.tabs.get(i).cloned() {
                    self.page = match tab.kind {
                        TabKind::Query => Page::Query,
                        TabKind::Data => Page::Data,
                        TabKind::Er => Page::Er,
                        TabKind::Redis => Page::Redis,
                        TabKind::Mongo => Page::Mongo,
                        TabKind::Es => Page::Es,
                        TabKind::Monitor => Page::Monitor,
                    };
                    self.active_conn = Some(tab.conn);
                }
                Task::none()
            }
            Msg::CloseTab(i) => {
                if i < self.tabs.len() {
                    let tab = self.tabs.remove(i);
                    match tab.kind {
                        TabKind::Query => {
                            if tab.index < self.query_tabs.len() {
                                self.query_tabs.remove(tab.index);
                            }
                            for (n, t) in self.query_tabs.iter_mut().enumerate() {
                                t.index = n;
                            }
                            for t in self.tabs.iter_mut() {
                                if t.kind == TabKind::Query {
                                    if t.index > tab.index {
                                        t.index -= 1;
                                    }
                                }
                            }
                        }
                        TabKind::Data => {
                            if tab.index < self.data_tabs.len() {
                                self.data_tabs.remove(tab.index);
                            }
                            for (n, t) in self.data_tabs.iter_mut().enumerate() {
                                t.index = n;
                            }
                            for t in self.tabs.iter_mut() {
                                if t.kind == TabKind::Data && t.index > tab.index {
                                    t.index -= 1;
                                }
                            }
                        }
                        _ => {}
                    }
                    if self.tabs.is_empty() {
                        self.page = Page::Connections;
                        self.active_tab = 0;
                    } else {
                        self.active_tab = self.active_tab.min(self.tabs.len() - 1);
                    }
                }
                Task::none()
            }
            Msg::NewQueryTab => self.new_query_tab(),
            Msg::ToggleNode(key) => {
                if self.expanded.contains(&key) {
                    self.expanded.remove(&key);
                    Task::none()
                } else {
                    self.expanded.insert(key.clone());
                    if let Some(id) = key.strip_prefix("conn:").and_then(|s| s.parse::<i64>().ok()) {
                        self.active_conn = Some(id);
                        return self.load_tree(id);
                    }
                    Task::none()
                }
            }
            Msg::SearchChanged(v) => {
                self.search = v;
                Task::none()
            }
            Msg::OpenConnection(id) => {
                self.active_conn = Some(id);
                self.expanded.insert(format!("conn:{id}"));
                self.load_tree(id)
            }
            Msg::ConnectConnection(id) => self.connect(id),
            Msg::Connected(id, result) => {
                match result {
                    Ok(info) => {
                        self.connected.insert(id);
                        self.toast(format!("已连接 · {info}"), Tone::Success);
                    }
                    Err(e) => {
                        self.connected.remove(&id);
                        self.toast(format!("连接失败：{e}"), Tone::Error);
                    }
                }
                Task::none()
            }
            Msg::TreeLoaded(id, result) => {
                self.tree.loading = None;
                match result {
                    Ok(payload) => {
                        self.tree.databases.insert(id, payload.databases.clone());
                        for db in &payload.databases {
                            self.tree.tables.insert((id, db.clone()), payload.tables.clone());
                            self.tree.views.insert((id, db.clone()), payload.views.clone());
                        }
                        if self.database.is_none() {
                            self.database = payload.databases.first().cloned();
                        }
                        // 与原型一致：首屏默认展开「库 → 表」，直接看到对象清单
                        if let Some(db) = payload.databases.first() {
                            self.expanded.insert(format!("db:{id}:{db}"));
                            self.expanded.insert(format!("tables:{id}:{db}"));
                            self.expanded.insert(format!("views:{id}:{db}"));
                        }
                        self.connected.insert(id);
                    }
                    Err(e) => {
                        self.tree.last_error = Some(e.clone());
                        self.toast(format!("读取对象失败：{e}"), Tone::Error);
                    }
                }
                Task::none()
            }
            Msg::OpenTable(conn, table) => self.open_table(conn, table),
            Msg::OpenMonitor(conn) => self.open_monitor(conn),
            Msg::LoadEr(conn) => self.load_er(conn),
            Msg::OpenRedis(conn) => self.open_redis(conn),
            Msg::OpenMongo(conn) => self.open_mongo(conn),
            Msg::OpenEs(conn) => self.open_es(conn),
            Msg::OpenSettings => {
                self.page = Page::Settings;
                Task::none()
            }
            Msg::GoConnections => {
                self.page = Page::Connections;
                Task::none()
            }
            Msg::RefreshTree => {
                if let Some(id) = self.active_conn {
                    self.load_tree(id)
                } else {
                    Task::none()
                }
            }
            Msg::ConnMenu(id) => {
                self.conn_ui.menu_for = if self.conn_ui.menu_for == Some(id) { None } else { Some(id) };
                Task::none()
            }
            Msg::DeleteConnection(id) => {
                self.confirm = Some(Confirm {
                    title: "删除连接".into(),
                    message: format!("确定删除连接「{}」？", self.conn_name(id)),
                    detail: Some("凭据会同时从 Vault 中移除，此操作不可撤销。".into()),
                    danger: true,
                    action: ConfirmAction::DeleteConnection(id),
                });
                Task::none()
            }

            // ---- 向导 ----
            Msg::WizardOpen => {
                self.wizard = Some(Wizard::new(DbKind::Mysql));
                self.modal = Modal::Wizard;
                Task::none()
            }
            Msg::WizardStart => {
                // 首页已选好类型 → 直接进表单，跳过重复的「选类型」步骤
                let w = self.wizard.get_or_insert_with(|| Wizard::new(DbKind::Mysql));
                w.step = 1;
                w.error = None;
                self.modal = Modal::Wizard;
                Task::none()
            }
            Msg::WizardClose => {
                self.wizard = None;
                self.modal = Modal::None;
                Task::none()
            }
            Msg::WizardKind(kind) => {
                // 首页磁贴：只记录选择，不弹窗（「下一步」直接进表单）
                if let Some(w) = &mut self.wizard {
                    let name = w.name.clone();
                    let group = w.group.clone();
                    let readonly = w.readonly;
                    let mut next = Wizard::new(kind);
                    next.name = name;
                    next.group = group;
                    next.readonly = readonly;
                    if kind.is_file_based() {
                        next.database = String::new();
                    }
                    *w = next;
                } else {
                    self.wizard = Some(Wizard::new(kind));
                }
                Task::none()
            }
            Msg::WizardStep(delta) => {
                if let Some(w) = &mut self.wizard {
                    // 校验仅在「表单 → 测试」推进时进行；
                    // 第 0 步（选类型）没有任何必填字段，必须直接放行，
                    // 否则名称在第 1 步才能填，会在此死锁（表现为点「下一步」无反应）。
                    if delta > 0 {
                        if w.step >= 1 {
                            if let Some(err) = w.validate() {
                                w.error = Some(err);
                                return Task::none();
                            }
                        }
                        w.error = None;
                    }
                    w.step = (w.step + delta).clamp(0, 2);
                }
                Task::none()
            }
            Msg::WizardName(v) => self.wiz_mut(|w| w.name = v),
            Msg::WizardHost(v) => self.wiz_mut(|w| w.host = v),
            Msg::WizardPort(v) => self.wiz_mut(|w| w.port = v),
            Msg::WizardDatabase(v) => self.wiz_mut(|w| w.database = v),
            Msg::WizardUser(v) => self.wiz_mut(|w| w.user = v),
            Msg::WizardPassword(v) => self.wiz_mut(|w| w.password = v),
            Msg::WizardGroup(v) => self.wiz_mut(|w| w.group = v),
            Msg::WizardReadonly(v) => self.wiz_mut(|w| w.readonly = v),
            Msg::WizardSsl(i) => self.wiz_mut(|w| w.ssl = i),
            Msg::WizardSsh(v) => self.wiz_mut(|w| w.ssh = v),
            Msg::WizardSshHost(v) => self.wiz_mut(|w| w.ssh_host = v),
            Msg::WizardTest => self.wizard_test(),
            Msg::WizardTested(result) => {
                if let Some(w) = &mut self.wizard {
                    w.testing = false;
                    w.test_result = Some(result.clone());
                }
                match result {
                    Ok(info) => self.toast(format!("连接成功 · {info}"), Tone::Success),
                    Err(e) => self.toast(format!("连接失败：{e}"), Tone::Error),
                }
                Task::none()
            }
            Msg::WizardSave => self.wizard_save(),
            Msg::WizardSaved(result) => {
                if let Some(w) = &mut self.wizard {
                    w.saving = false;
                }
                match result {
                    Ok(()) => {
                        if let Some(core) = &self.core {
                            self.conns = core.connections();
                            self.refresh_recent();
                        }
                        self.wizard = None;
                        self.modal = Modal::None;
                        self.toast("连接已保存（凭据已加密写入 Vault）", Tone::Success);
                    }
                    Err(e) => {
                        if let Some(w) = &mut self.wizard {
                            w.error = Some(e.clone());
                        }
                        self.toast(format!("保存失败：{e}"), Tone::Error);
                    }
                }
                Task::none()
            }
            Msg::WizardBrowseFile => self.pick_sqlite_file(),

            // ---- 查询 ----
            Msg::QueryEdit(text) => {
                if let Some(tab) = self.current_query_mut() {
                    tab.sql = text;
                }
                self.run_diagnostics()
            }
            Msg::QueryRun => self.run_current_query(),
            Msg::QueryStop => {
                if let Some(tab) = self.current_query_mut() {
                    tab.running = false;
                    tab.error = Some("查询已停止（下一批数据返回时生效）".into());
                }
                Task::none()
            }
            Msg::QueryFormat => {
                if let Some(tab) = self.current_query_mut() {
                    tab.sql = sqlutil::format_sql(&tab.sql);
                    tab.caret = tab.sql.chars().count();
                }
                self.toast("已格式化 SQL", Tone::Neutral);
                Task::none()
            }
            Msg::QuerySave => {
                let Some(tab) = self.current_query() else { return Task::none() };
                let sql = tab.sql.clone();
                let conn = tab.conn;
                let Some(core) = self.core.clone() else { return Task::none() };
                let name = format!("查询 {}", self.query_tabs.len() + 1);
                self.toast("查询已保存", Tone::Success);
                Task::perform(
                    async move {
                        let _ = core.store.save_query(&name, &sql, Some(conn)).await;
                    },
                    |_| Msg::Noop,
                )
            }
            Msg::QueryTabPick(i) => {
                if let Some(pos) = self.tabs.iter().position(|t| t.kind == TabKind::Query && t.index == i) {
                    return self.update(Msg::SelectTab(pos));
                }
                Task::none()
            }
            Msg::ResultTabPick(i) => {
                if let Some(tab) = self.current_query_mut() {
                    tab.result_tab = match i {
                        0 => ResultTab::Grid,
                        1 => ResultTab::Messages,
                        _ => ResultTab::Plan,
                    };
                }
                Task::none()
            }
            Msg::Stream(idx, event) => self.on_stream(idx, event),
            Msg::QueryStarted(idx, Ok(rx)) => {
                let rx = rx.try_lock().ok().and_then(|mut g| g.take());
                match rx {
                    Some(rx) => Task::stream(crate::update_ops::stream_from_receiver(idx, rx)),
                    None => Task::none(),
                }
            }
            Msg::QueryStarted(idx, Err(e)) => {
                tracing::warn!("ui: QueryStarted 失败 tab={idx} {e}");
                if let Some(tab) = self.query_tabs.get_mut(idx) {
                    tab.running = false;
                    tab.error = Some(e.clone());
                }
                self.toast(format!("执行失败：{e}"), Tone::Error);
                Task::none()
            }
            Msg::EditorPaste(text) => {
                if let Some(text) = text {
                    if let Some(tab) = self.current_query_mut() {
                        let mut sql = tab.sql.clone();
                        let mut caret = tab.caret;
                        let mut anchor = tab.anchor;
                        apply_edit(&mut sql, &mut caret, &mut anchor, EditOp::Insert(text));
                        tab.sql = sql;
                        tab.caret = caret;
                        tab.anchor = anchor;
                    }
                }
                Task::none()
            }
            Msg::EstimateReady(affected) => {
                if let Some(confirm) = &mut self.confirm {
                    let mut detail = confirm.detail.clone().unwrap_or_default();
                    let est = match affected {
                        Some(n) => format!("\n影响行数预估：{n} 行"),
                        None => "\n影响行数预估：无法计算（可能涉及全表）".to_string(),
                    };
                    if !detail.contains("影响行数预估") {
                        detail.push_str(&est);
                    }
                    confirm.detail = Some(detail);
                }
                Task::none()
            }
            Msg::DdlReady(ddl) => {
                let task = Task::perform(
                    async move {
                        let path = rfd::AsyncFileDialog::new()
                            .set_file_name("schema.sql")
                            .add_filter("SQL", &["sql"])
                            .save_file()
                            .await
                            .map(|f| f.path().to_string_lossy().to_string());
                        if let Some(path) = path {
                            std::fs::write(path, ddl).map_err(|e| e.to_string())
                        } else {
                            Err("已取消".into())
                        }
                    },
                    Msg::DdlSaved,
                );
                task
            }
            Msg::DdlSaved(result) => {
                match result {
                    Ok(()) => self.toast("DDL 已导出", Tone::Success),
                    Err(e) if e == "已取消" => {}
                    Err(e) => self.toast(format!("导出失败：{e}"), Tone::Error),
                }
                Task::none()
            }
            Msg::MongoDocsDeleted(result) => {
                match result {
                    Ok(n) => {
                        self.toast(format!("已删除 {n} 个文档"), Tone::Success);
                        return self.mongo_load_docs();
                    }
                    Err(e) => self.toast(format!("删除失败：{e}"), Tone::Error),
                }
                Task::none()
            }
            Msg::QueryDone(idx) => {
                if let Some(tab) = self.query_tabs.get_mut(idx) {
                    tab.running = false;
                }
                Task::none()
            }
            Msg::ExportResult => self.export_current(),
            Msg::ExportPicked(path) => {
                let Some(path) = path else { return Task::none() };
                let Some(tab) = self.current_query() else { return Task::none() };
                let columns = tab.columns.clone();
                let rows = tab.rows.clone();
                let table = tab
                    .sql
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .windows(2)
                    .find(|w| w[0].eq_ignore_ascii_case("from"))
                    .map(|w| w[1].trim_matches(|c: char| c == ';' || c == '"').to_string())
                    .unwrap_or_else(|| "result".into());
                self.toast("正在导出…", Tone::Neutral);
                Task::perform(
                    async move {
                        dbhelper_core::export::export(
                            path,
                            dbhelper_core::export::ExportFormat::Csv,
                            columns,
                            rows,
                            table,
                        )
                        .await
                    },
                    |r| match r {
                        Ok(n) => Msg::ExportDone(Ok(n)),
                        Err(e) => Msg::ExportDone(Err(e)),
                    },
                )
            }
            Msg::CopyResult => {
                let Some(tab) = self.current_query() else { return Task::none() };
                let mut out = String::new();
                out.push_str(&tab.columns.iter().map(|c| c.name.clone()).collect::<Vec<_>>().join("\t"));
                out.push('\n');
                for row in &tab.rows {
                    out.push_str(&row.iter().map(|v| v.display()).collect::<Vec<_>>().join("\t"));
                    out.push('\n');
                }
                iced::clipboard::write(out)
            }
            Msg::EditorClick(index) => {
                if let Some(tab) = self.current_query_mut() {
                    if index != tab.caret {
                        tab.anchor = index;
                    }
                    tab.caret = index;
                }
                self.editor_focus = true;
                Task::none()
            }
            Msg::EditorFocus(v) => {
                self.editor_focus = v;
                Task::none()
            }
            Msg::EditorIme(text) => {
                if let Some(tab) = self.current_query_mut() {
                    let mut sql = tab.sql.clone();
                    let mut caret = tab.caret;
                    let mut anchor = tab.anchor;
                    apply_edit(&mut sql, &mut caret, &mut anchor, EditOp::Insert(text));
                    tab.sql = sql;
                    tab.caret = caret;
                    tab.anchor = anchor;
                }
                Task::none()
            }
            Msg::EditorKey(event) => self.on_editor_key(event),

            // ---- 数据网格 ----
            Msg::DataLoaded(idx, result) => {
                if let Some(tab) = self.data_tabs.get_mut(idx) {
                    match result {
                        Ok(def) => {
                            // 列信息来自表结构反射，网格与行内编辑都依赖它
                            tab.columns = def
                                .columns
                                .iter()
                                .map(|c| ColumnMeta::new(c.name.clone(), c.data_type.clone()))
                                .collect();
                            tab.def = Some(def);
                        }
                        Err(e) => tab.error = Some(e),
                    }
                }
                Task::none()
            }
            Msg::DataRows(idx, result) => {
                if let Some(tab) = self.data_tabs.get_mut(idx) {
                    tab.loading = false;
                    match result {
                        Ok((rows, total)) => {
                            tab.rows = rows;
                            tab.total = total;
                            tab.error = None;
                        }
                        Err(e) => tab.error = Some(e),
                    }
                }
                Task::none()
            }
            Msg::DataPage(delta) => {
                let Some(tab) = self.current_data_mut() else { return Task::none() };
                let max = tab.page_count().saturating_sub(1) as i64;
                let next = (tab.page as i64 + delta).clamp(0, max) as usize;
                tab.page = next;
                let idx = tab.index;
                self.reload_data(idx)
            }
            Msg::DataSelect(i) => {
                let Some(tab) = self.current_data_mut() else { return Task::none() };
                tab.selected = Some(i);
                tab.edit = build_edit_fields(tab, i);
                Task::none()
            }
            Msg::DataCellPick(field, value) => {
                if let Some(tab) = self.current_data_mut() {
                    if let Some(f) = tab.edit.iter_mut().find(|f| f.name == field) {
                        f.value = value;
                    }
                    tab.dirty = tab.pending();
                }
                Task::none()
            }
            Msg::DataFieldEdit(v) => {
                if let Some(tab) = self.current_data_mut() {
                    if let Some(f) = tab.edit.last_mut() {
                        f.value = v;
                    }
                    tab.dirty = tab.pending();
                }
                Task::none()
            }
            Msg::DataUndo => {
                if let Some(tab) = self.current_data_mut() {
                    for f in tab.edit.iter_mut() {
                        f.value = f.original.clone();
                    }
                    tab.dirty = 0;
                }
                Task::none()
            }
            Msg::DataCommit => self.commit_edit(),
            Msg::DataCommitted(result) => {
                match result {
                    Ok(n) => {
                        self.toast(format!("已提交 {n} 行更改"), Tone::Success);
                        if let Some(tab) = self.current_data() {
                            let idx = tab.index;
                            return self.reload_data(idx);
                        }
                    }
                    Err(e) => self.toast(format!("提交失败：{e}"), Tone::Error),
                }
                Task::none()
            }
            Msg::DataSearch(v) => {
                if let Some(tab) = self.current_data_mut() {
                    tab.search = v;
                }
                Task::none()
            }
            Msg::DataFilterRemove(i) => {
                if let Some(tab) = self.current_data_mut() {
                    if i < tab.filters.len() {
                        tab.filters.remove(i);
                    }
                    tab.page = 0;
                }
                if let Some(tab) = self.current_data() {
                    let idx = tab.index;
                    return self.reload_data(idx);
                }
                Task::none()
            }
            Msg::DataSort(col) => {
                if let Some(tab) = self.current_data_mut() {
                    tab.sort = match &tab.sort {
                        Some((c, asc)) if c == &col => Some((col.clone(), !asc)),
                        _ => Some((col, false)),
                    };
                    tab.page = 0;
                }
                if let Some(tab) = self.current_data() {
                    let idx = tab.index;
                    return self.reload_data(idx);
                }
                Task::none()
            }
            Msg::DataAddFilter => {
                self.toast("在 SQL 编辑器中可使用 WHERE 精确筛选", Tone::Neutral);
                Task::none()
            }
            Msg::DataReload => {
                if let Some(tab) = self.current_data() {
                    let idx = tab.index;
                    return self.reload_data(idx);
                }
                Task::none()
            }

            // ---- ER ----
            Msg::ErLoaded(conn, result) => {
                self.er.loading = false;
                match result {
                    Ok(defs) => {
                        self.er.reset(conn);
                        let mut nodes = Vec::new();
                        for (i, def) in defs.into_iter().enumerate() {
                            let cols = def.columns.len().max(1) as f32;
                            let h = 34.0 + cols * 22.0 + 10.0;
                            // 3 列布局：40 + 2*280 + 240 = 840，可容于 870 宽画布
                            let x = 40.0 + (i % 3) as f32 * 280.0;
                            let y = 40.0 + (i / 3) as f32 * 230.0;
                            nodes.push(ErNode { table: def, x, y, w: 240.0, h });
                        }
                        self.er.nodes = nodes;
                        self.er.dirty = 0;
                    }
                    Err(e) => self.er.error = Some(e),
                }
                Task::none()
            }
            Msg::ErDragStart(i, p) => {
                self.er.dragging = Some((i, p));
                self.er.selected = Some(i);
                Task::none()
            }
            Msg::ErDrag(i, p) => {
                if let Some((idx, _)) = self.er.dragging {
                    if let Some(node) = self.er.nodes.get_mut(idx) {
                        node.x = (node.x + p.x).max(0.0);
                        node.y = (node.y + p.y).max(0.0);
                    }
                    self.er.dirty += 1;
                }
                let _ = i;
                Task::none()
            }
            Msg::ErDragEnd => {
                self.er.dragging = None;
                Task::none()
            }
            Msg::ErSelect(i) => {
                self.er.selected = if self.er.selected == Some(i) { None } else { Some(i) };
                Task::none()
            }
            Msg::ErZoom(delta) => {
                self.er.zoom = (self.er.zoom + delta).clamp(0.5, 2.0);
                Task::none()
            }
            Msg::ErAutoLayout => {
                for (i, node) in self.er.nodes.iter_mut().enumerate() {
                    node.x = 40.0 + (i % 3) as f32 * 280.0;
                    node.y = 40.0 + (i / 3) as f32 * 230.0;
                }
                self.er.dirty += 1;
                self.toast("已自动布局", Tone::Neutral);
                Task::none()
            }
            Msg::ErExportDdl => self.export_ddl(),
            Msg::ErAddTable => {
                self.toast("请在 SQL 编辑器中执行 CREATE TABLE 新增表", Tone::Neutral);
                Task::none()
            }
            Msg::ErMode(i) => {
                self.er.mode = i;
                Task::none()
            }

            // ---- Redis ----
            Msg::RedisLoaded(conn, result) => {
                self.redis.loading = false;
                self.redis.conn = Some(conn);
                match result {
                    Ok(info) => {
                        self.redis.info = Some(info);
                        self.redis.error = None;
                        return self.redis_scan();
                    }
                    Err(e) => {
                        self.redis.error = Some(e.clone());
                        self.redis.keys.clear();
                    }
                }
                Task::none()
            }
            Msg::RedisScan(pattern) => {
                self.redis.pattern = pattern;
                self.redis_scan()
            }
            Msg::RedisKeys(keys) => {
                self.redis.keys = keys;
                Task::none()
            }
            Msg::RedisPickDb(db) => {
                self.redis.db = db;
                self.redis.selected = None;
                self.redis_scan()
            }
            Msg::RedisPickKey(key) => {
                self.redis.selected = Some(key.clone());
                self.redis.loading = true;
                self.redis_load_key(key)
            }
            Msg::RedisKeyLoaded(result) => {
                self.redis.loading = false;
                match result {
                    Ok((kt, fields, ttl, mem, enc)) => {
                        self.redis.key_type = kt;
                        self.redis.edit_value = fields.first().map(|(_, v)| v.clone()).unwrap_or_default();
                        self.redis.fields = fields;
                        self.redis.ttl = ttl;
                        self.redis.memory = mem;
                        self.redis.encoding = enc;
                        self.redis.error = None;
                    }
                    Err(e) => self.redis.error = Some(e),
                }
                Task::none()
            }
            Msg::RedisTypeTab(i) => {
                self.redis.tab = i;
                Task::none()
            }
            Msg::RedisEdit(v) => {
                self.redis.edit_value = v;
                Task::none()
            }
            Msg::RedisSave => self.redis_save(),
            Msg::RedisDelete => {
                if let Some(key) = self.redis.selected.clone() {
                    self.confirm = Some(Confirm {
                        title: "删除键".into(),
                        message: format!("确定删除键「{key}」？"),
                        detail: Some("该操作立即生效且不可撤销。".into()),
                        danger: true,
                        action: ConfirmAction::DeleteKey(key),
                    });
                }
                Task::none()
            }

            // ---- Mongo ----
            Msg::MongoLoaded(result) => {
                self.mongo.loading = false;
                match result {
                    Ok((dbs, colls)) => {
                        self.mongo.databases = dbs;
                        self.mongo.collections = colls;
                        self.mongo.error = None;
                    }
                    Err(e) => self.mongo.error = Some(e),
                }
                Task::none()
            }
            Msg::MongoPickDb(db) => {
                self.mongo.db = Some(db);
                self.mongo_load(None)
            }
            Msg::MongoPickColl(c) => {
                self.mongo.collection = Some(c.clone());
                self.mongo_load_docs()
            }
            Msg::MongoDocs(result) => {
                self.mongo.loading = false;
                match result {
                    Ok(docs) => {
                        self.mongo.docs = docs;
                        self.mongo.selected = Some(0);
                        self.mongo.error = None;
                    }
                    Err(e) => self.mongo.error = Some(e),
                }
                Task::none()
            }
            Msg::MongoPipeline(v) => {
                self.mongo.pipeline = v;
                Task::none()
            }
            Msg::MongoRun => self.mongo_run(),
            Msg::MongoViewTab(i) => {
                self.mongo.tab = i;
                Task::none()
            }
            Msg::MongoDeleteDoc(i) => {
                if let (Some(conn), Some(db), Some(coll), Some(doc)) = (
                    self.mongo.conn,
                    self.mongo.db.clone(),
                    self.mongo.collection.clone(),
                    self.mongo.docs.get(i).cloned(),
                ) {
                    let id = doc.get("_id").cloned().unwrap_or(serde_json::Value::Null);
                    let Some(core) = self.core.clone() else { return Task::none() };
                    return Task::perform(
                        async move {
                            let handle = core.ensure_connected(conn).await.map_err(|e| e.to_string())?;
                            let mongo = handle.mongo().ok_or("驱动不可用".to_string())?;
                            mongo.delete_by_id(&db, &coll, id).await.map_err(|e| e.to_string())
                        },
                        Msg::MongoDocsDeleted,
                    );
                }
                Task::none()
            }

            // ---- ES ----
            Msg::EsLoaded(result) => {
                self.es.loading = false;
                match result {
                    Ok(list) => {
                        self.es.error = None;
                        self.es.indices = list;
                        if self.es.index.is_none() {
                            if let Some((name, _, _, _)) = self.es.indices.first().cloned() {
                                self.es.index = Some(name.clone());
                                self.es.dsl = default_es_dsl(&name);
                            }
                        }
                    }
                    Err(e) => self.es.error = Some(e),
                }
                Task::none()
            }
            Msg::EsPickIndex(name) => {
                self.es.index = Some(name.clone());
                if self.es.dsl.trim().is_empty() {
                    self.es.dsl = default_es_dsl(&name);
                }
                Task::none()
            }
            Msg::EsDsl(v) => {
                self.es.dsl = v;
                Task::none()
            }
            Msg::EsAction(action) => {
                self.es_editor.perform(action);
                self.es.dsl = self.es_editor.text();
                Task::none()
            }
            Msg::MongoAction(action) => {
                self.mongo_editor.perform(action);
                self.mongo.pipeline = self.mongo_editor.text();
                Task::none()
            }
            Msg::MongoActions(i) => {
                self.mongo.selected = Some(i);
                Task::none()
            }
            Msg::EsRun => self.es_run(),
            Msg::EsResult(result) => {
                self.es.loading = false;
                match result {
                    Ok((total, took, hits, timed_out)) => {
                        self.es.total = total;
                        self.es.took = took;
                        self.es.hits = hits;
                        self.es.timed_out = timed_out;
                        self.es.error = None;
                    }
                    Err(e) => self.es.error = Some(e),
                }
                Task::none()
            }

            // ---- 监控 ----
            Msg::MonitorSessions(sessions) => {
                self.monitor.sessions = sessions;
                self.monitor.last_update = self.now;
                Task::none()
            }
            Msg::MonitorRange(i) => {
                self.monitor.range = i;
                Task::none()
            }
            Msg::MonitorKill(id) => {
                self.confirm = Some(Confirm {
                    title: "终止会话".into(),
                    message: format!("确定终止会话 {id}？"),
                    detail: Some("该会话正在执行的语句会被回滚。".into()),
                    danger: true,
                    action: ConfirmAction::KillSession(id),
                });
                Task::none()
            }

            // ---- 设置 ----
            Msg::SettingsSection(i) => {
                self.settings_ui.section = i;
                Task::none()
            }
            Msg::SetTheme(mode) => {
                self.settings.theme = mode;
                self.persist_settings()
            }
            Msg::SetLanguage(lang) => {
                self.settings.language = lang;
                self.persist_settings()
            }
            Msg::SetEditorFont(f) => {
                self.settings.editor_font = f;
                self.persist_settings()
            }
            Msg::SetUiFont(f) => {
                self.settings.ui_font = f;
                self.persist_settings()
            }
            Msg::SetSerif(v) => {
                self.settings.ui_serif = v;
                self.persist_settings()
            }
            Msg::SetFontSize(v) => {
                self.settings.font_size = v.clamp(12, 18);
                self.persist_settings()
            }
            Msg::SetLineNumbers(v) => {
                self.settings.show_line_numbers = v;
                self.persist_settings()
            }
            Msg::SetAutoWrap(v) => {
                self.settings.auto_wrap = v;
                self.persist_settings()
            }
            Msg::SetSyntax(v) => {
                self.settings.syntax_highlight = v;
                self.persist_settings()
            }
            Msg::SetBracket(v) => {
                self.settings.bracket_highlight = v;
                self.persist_settings()
            }
            Msg::BrowseDir(target) => {
                let start = match target {
                    DirTarget::Log => self.settings.log_dir.clone(),
                    DirTarget::Config => self.settings.config_path.clone(),
                };
                Task::perform(
                    async move {
                        let mut dialog = rfd::AsyncFileDialog::new();
                        if !start.is_empty() {
                            dialog = dialog.set_directory(start);
                        }
                        dialog.pick_folder().await.map(|f| f.path().to_string_lossy().to_string())
                    },
                    move |p| Msg::DirPicked(target, p),
                )
            }
            Msg::DirPicked(target, path) => {
                if let Some(path) = path {
                    match target {
                        DirTarget::Log => self.settings.log_dir = path,
                        DirTarget::Config => self.settings.config_path = path,
                    }
                    return self.persist_settings();
                }
                Task::none()
            }
            Msg::OpenExportWizard => {
                self.settings_ui.export_step = 0;
                self.settings_ui.export_items = [true, true, true, false];
                self.settings_ui.export_result = None;
                self.modal = Modal::Export;
                Task::none()
            }
            Msg::ExportStep(delta) => {
                self.settings_ui.export_step = (self.settings_ui.export_step + delta).clamp(0, 2);
                Task::none()
            }
            Msg::ExportToggle(i, v) => {
                if i < 4 {
                    self.settings_ui.export_items[i] = v;
                }
                Task::none()
            }
            Msg::ExportFormatPick(i) => {
                self.settings_ui.export_format = i;
                Task::none()
            }
            Msg::ExportRun => self.run_export_wizard(),
            Msg::ExportDone(result) => {
                match result {
                    Ok(n) => {
                        self.toast(format!("已导出 {n} 行"), Tone::Success);
                        self.settings_ui.export_result = Some(format!("已导出 {n} 行"));
                        self.modal = Modal::None;
                    }
                    Err(e) => {
                        self.settings_ui.export_result = Some(format!("导出失败：{e}"));
                        self.toast(format!("导出失败：{e}"), Tone::Error);
                    }
                }
                Task::none()
            }

            // ---- 通用 ----
            Msg::ToastClose(i) => {
                if i < self.toasts.len() {
                    self.toasts.remove(i);
                }
                Task::none()
            }
            Msg::DismissModal => {
                self.modal = Modal::None;
                self.wizard = None;
                self.confirm = None;
                Task::none()
            }
            Msg::ConfirmRun => self.confirm_run(),
            Msg::CancelConfirm => {
                self.confirm = None;
                Task::none()
            }
            Msg::CopyText(text) => iced::clipboard::write(text),
            Msg::Noop => Task::none(),
            // 其余消息（查询智能 / 设计器 / BI / 迁移 / 管理 / 导入）在 update_features
            other => self.update_features(other),
        }
    }

    // -----------------------------------------------------------------------
    // 辅助
    // -----------------------------------------------------------------------

    fn wiz_mut(&mut self, f: impl FnOnce(&mut Wizard)) -> Task<Msg> {
        if let Some(w) = &mut self.wizard {
            f(w);
            w.error = None;
        }
        Task::none()
    }

    fn conn_name(&self, id: ConnId) -> String {
        self.conns
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.name.clone())
            .unwrap_or_else(|| id.to_string())
    }

    fn refresh_recent(&mut self) {
        let mut list = self.conns.clone();
        list.sort_by_key(|c| -c.created_at);
        self.conn_ui.recent = list.into_iter().take(5).collect();
    }

    fn refresh_live_metrics(&mut self) -> Task<Msg> {
        if self.monitor.conn.is_some() {
            // 每秒采样一次活动会话（仅关系型且支持会话列表时）
            if let Some(core) = &self.core {
                if let Some(conn) = self.monitor.conn {
                    let core = core.clone();
                    let support = core
                        .connection(conn)
                        .map(|c| c.kind.is_relational() && c.kind != DbKind::Sqlite)
                        .unwrap_or(false);
                    if support {
                        return Task::perform(
                            async move {
                                let handle = core.ensure_connected(conn).await.ok()?;
                                let driver = handle.rel()?;
                                driver.sessions().await.ok()
                            },
                            |s| Msg::MonitorSessions(s.unwrap_or_default()),
                        );
                    }
                }
            }
        }
        Task::none()
    }

    fn on_booted(&mut self, result: Result<std::sync::Arc<dbhelper_core::AppCore>, String>) -> Task<Msg> {
        match result {
            Ok(core) => {
                let _ = std::fs::write(std::env::temp_dir().join("dbhelper_booted.flag"), "1");
                self.settings = core.settings.read().clone();
                // 旧配置里的 Inter/Consolas 等名字统一迁移到设计规范字体
                self.settings.ui_font = crate::theme::static_family(&self.settings.ui_font).to_string();
                self.settings.editor_font = crate::theme::static_family(&self.settings.editor_font).to_string();
                self.conns = core.connections();
                let mut groups: Vec<String> = self.conns.iter().map(|c| c.group.clone()).collect();
                groups.sort();
                groups.dedup();
                self.groups = groups;
                self.refresh_recent();
                self.core = Some(core.clone());
                // 自动展开并连接本地 SQLite，保证首屏即为真实数据
                if let Some(local) = self.conns.iter().find(|c| c.kind == DbKind::Sqlite).cloned() {
                    self.expanded.insert(format!("conn:{}", local.id));
                    self.active_conn = Some(local.id);
                    self.connected.insert(local.id);
                    let tree = self.load_tree(local.id);
                    let q = self.new_query_tab_for(local.id);
                    let run = self.run_current_query();
                    return Task::batch([tree, q, run]);
                }
                Task::none()
            }
            Err(e) => {
                self.boot_error = Some(e);
                Task::none()
            }
        }
    }

    fn persist_settings(&mut self) -> Task<Msg> {
        let Some(core) = self.core.clone() else { return Task::none() };
        let settings = self.settings.clone();
        Task::perform(
            async move {
                let _ = core.save_settings(&settings).await;
            },
            |_| Msg::Noop,
        )
    }

    fn connect(&mut self, id: ConnId) -> Task<Msg> {
        let Some(core) = self.core.clone() else { return Task::none() };
        Task::perform(
            async move {
                match core.ensure_connected(id).await {
                    Ok(handle) => Ok(handle.kind().label().to_string()),
                    Err(e) => Err(e.to_string()),
                }
            },
            move |r| Msg::Connected(id, r),
        )
    }

    fn load_tree(&mut self, id: ConnId) -> Task<Msg> {
        let Some(core) = self.core.clone() else { return Task::none() };
        self.tree.loading = Some(id);
        Task::perform(
            async move { load_tree_payload(core, id).await },
            move |r| Msg::TreeLoaded(id, r),
        )
    }

    pub(crate) fn new_query_tab(&mut self) -> Task<Msg> {
        let conn = match self.active_conn.or_else(|| self.conns.first().map(|c| c.id)) {
            Some(c) => c,
            None => return Task::none(),
        };
        self.new_query_tab_for(conn)
    }

    pub(crate) fn new_query_tab_for(&mut self, conn: ConnId) -> Task<Msg> {
        let index = self.query_tabs.len();
        let title = format!("查询 {}", index + 1);
        let tab = QueryTab::new(index, conn, title.clone());
        self.query_tabs.push(tab);
        self.tabs.push(Tab {
            title,
            kind: TabKind::Query,
            conn,
            db: None,
            table: None,
            index,
        });
        self.active_tab = self.tabs.len() - 1;
        self.page = Page::Query;
        self.editor_focus = true;
        Task::none()
    }

    fn open_table(&mut self, conn: ConnId, table: String) -> Task<Msg> {
        let db = self
            .tree
            .databases
            .get(&conn)
            .and_then(|d| d.first())
            .cloned()
            .or_else(|| self.database.clone());
        if let Some(pos) = self
            .tabs
            .iter()
            .position(|t| t.kind == TabKind::Data && t.conn == conn && t.table.as_deref() == Some(table.as_str()))
        {
            return self.update(Msg::SelectTab(pos));
        }
        let index = self.data_tabs.len();
        let tab = DataTab::new(index, conn, table.clone(), db.clone());
        self.data_tabs.push(tab);
        self.tabs.push(Tab {
            title: format!("{table} @ {}", self.conn_name(conn)),
            kind: TabKind::Data,
            conn,
            db,
            table: Some(table.clone()),
            index,
        });
        self.active_tab = self.tabs.len() - 1;
        self.page = Page::Data;
        self.active_conn = Some(conn);
        self.load_data(index)
    }

    fn load_data(&mut self, idx: usize) -> Task<Msg> {
        let Some(core) = self.core.clone() else { return Task::none() };
        let Some(tab) = self.data_tabs.get_mut(idx) else { return Task::none() };
        tab.loading = true;
        let conn = tab.conn;
        let table = tab.table.clone();
        let db = tab.db.clone();
        let page = tab.page;
        let size = tab.page_size;
        let sort = tab.sort.clone();
        let filters = tab.filters.clone();
        let search = tab.search.clone();
        let def_task = Task::perform(
            {
                let core = core.clone();
                let table = table.clone();
                let db = db.clone();
                async move {
                    let handle = core.ensure_connected(conn).await.map_err(|e| e.to_string())?;
                    let driver = handle.rel().ok_or("需要关系型连接".to_string())?;
                    driver
                        .describe_table(db.as_deref(), &table)
                        .await
                        .map_err(|e| e.user_message())
                }
            },
            move |r| Msg::DataLoaded(idx, r),
        );
        let rows_task = Task::perform(
            async move { load_rows(core, conn, table, db, page, size, sort, filters, search).await },
            move |r| Msg::DataRows(idx, r),
        );
        Task::batch([def_task, rows_task])
    }

    fn reload_data(&mut self, idx: usize) -> Task<Msg> {
        let Some(core) = self.core.clone() else { return Task::none() };
        let Some(tab) = self.data_tabs.get_mut(idx) else { return Task::none() };
        tab.loading = true;
        let conn = tab.conn;
        let table = tab.table.clone();
        let db = tab.db.clone();
        let page = tab.page;
        let size = tab.page_size;
        let sort = tab.sort.clone();
        let filters = tab.filters.clone();
        let search = tab.search.clone();
        Task::perform(
            async move { load_rows(core, conn, table, db, page, size, sort, filters, search).await },
            move |r| Msg::DataRows(idx, r),
        )
    }

    fn open_monitor(&mut self, conn: ConnId) -> Task<Msg> {
        self.monitor = MonitorState { conn: Some(conn), range: 0, ..Default::default() };
        self.page = Page::Monitor;
        self.active_conn = Some(conn);
        if let Some(pos) = self.tabs.iter().position(|t| t.kind == TabKind::Monitor) {
            self.tabs.remove(pos);
        }
        self.tabs.push(Tab {
            title: format!("监控 · {}", self.conn_name(conn)),
            kind: TabKind::Monitor,
            conn,
            db: None,
            table: None,
            index: 0,
        });
        self.active_tab = self.tabs.len() - 1;
        self.refresh_live_metrics()
    }

    fn open_redis(&mut self, conn: ConnId) -> Task<Msg> {
        self.redis = RedisState { conn: Some(conn), loading: true, ..Default::default() };
        self.page = Page::Redis;
        self.active_conn = Some(conn);
        let Some(core) = self.core.clone() else { return Task::none() };
        Task::perform(
            async move {
                let handle = core.ensure_connected(conn).await.map_err(|e| e.to_string())?;
                let redis = handle.redis().ok_or("驱动不可用".to_string())?;
                redis.info().await.map_err(|e| e.to_string())
            },
            move |r| Msg::RedisLoaded(conn, r),
        )
    }

    fn redis_scan(&mut self) -> Task<Msg> {
        let (Some(core), Some(conn)) = (self.core.clone(), self.redis.conn) else {
            return Task::none();
        };
        let pattern = self.redis.pattern.clone();
        let db = self.redis.db;
        let Some(core2) = self.core.clone() else { return Task::none() };
        let _ = core2;
        Task::perform(
            async move {
                let handle = core.ensure_connected(conn).await.map_err(|e| e.to_string())?;
                let redis = handle.redis().ok_or("驱动不可用".to_string())?;
                let mut keys = Vec::new();
                let mut cursor = 0u64;
                loop {
                    let (next, batch) = redis
                        .scan(db, &pattern, cursor, 200)
                        .await
                        .map_err(|e| e.to_string())?;
                    keys.extend(batch);
                    if next == 0 || keys.len() >= 500 {
                        break;
                    }
                    cursor = next;
                }
                keys.sort();
                Ok(keys)
            },
            |r: Result<Vec<String>, String>| Msg::RedisKeys(r.unwrap_or_default()),
        )
    }

    fn redis_load_key(&mut self, key: String) -> Task<Msg> {
        let (Some(core), Some(conn)) = (self.core.clone(), self.redis.conn) else {
            return Task::none();
        };
        Task::perform(
            async move {
                let handle = core.ensure_connected(conn).await.map_err(|e| e.to_string())?;
                let redis = handle.redis().ok_or("驱动不可用".to_string())?;
                let (kt, fields) = redis.read_key(&key).await.map_err(|e| e.to_string())?;
                let ttl = redis.ttl(&key).await.unwrap_or(-1);
                let mem = redis.memory_usage(&key).await.unwrap_or(0);
                let enc = redis.encoding(&key).await.unwrap_or_else(|_| "-".into());
                Ok((kt.label().to_string(), fields, ttl, mem, enc))
            },
            Msg::RedisKeyLoaded,
        )
    }

    fn redis_save(&mut self) -> Task<Msg> {
        let (Some(core), Some(conn), Some(key)) = (self.core.clone(), self.redis.conn, self.redis.selected.clone()) else {
            return Task::none();
        };
        let value = self.redis.edit_value.clone();
        let kt = match self.redis.key_type.as_str() {
            "HASH" => driver_redis::KeyType::Hash,
            "LIST" => driver_redis::KeyType::List,
            "SET" => driver_redis::KeyType::Set,
            "ZSET" => driver_redis::KeyType::ZSet,
            _ => driver_redis::KeyType::String,
        };
        let field = self.redis.fields.first().map(|(f, _)| f.clone()).unwrap_or_default();
        self.toast("已写入 Redis", Tone::Success);
        Task::perform(
            async move {
                let handle = core.ensure_connected(conn).await.map_err(|e| e.to_string())?;
                let redis = handle.redis().ok_or("驱动不可用".to_string())?;
                redis.write_key(&key, kt, &field, &value).await.map_err(|e| e.to_string())
            },
            |r: Result<(), String>| match r {
                Ok(()) => Msg::RedisScan(String::new()),
                Err(e) => Msg::RedisKeyLoaded(Err(e)),
            },
        )
    }

    fn open_mongo(&mut self, conn: ConnId) -> Task<Msg> {
        self.mongo = MongoState { conn: Some(conn), loading: true, tab: 0, ..Default::default() };
        self.page = Page::Mongo;
        self.active_conn = Some(conn);
        self.mongo_load(None)
    }

    fn mongo_load(&mut self, _db: Option<String>) -> Task<Msg> {
        let (Some(core), Some(conn)) = (self.core.clone(), self.mongo.conn) else {
            return Task::none();
        };
        let db = self.mongo.db.clone();
        Task::perform(
            async move {
                let handle = core.ensure_connected(conn).await.map_err(|e| e.to_string())?;
                let mongo = handle.mongo().ok_or("驱动不可用".to_string())?;
                let dbs = mongo.databases().await.map_err(|e| e.to_string())?;
                let target = db
                    .or_else(|| dbs.first().cloned())
                    .ok_or("没有可用的数据库".to_string())?;
                let colls = mongo
                    .collections(&target)
                    .await
                    .map_err(|e| e.to_string())?
                    .into_iter()
                    .map(|c| (c.name, c.count))
                    .collect::<Vec<_>>();
                Ok((dbs, colls))
            },
            Msg::MongoLoaded,
        )
    }

    fn mongo_load_docs(&mut self) -> Task<Msg> {
        let (Some(core), Some(conn), Some(db), Some(coll)) = (
            self.core.clone(),
            self.mongo.conn,
            self.mongo.db.clone(),
            self.mongo.collection.clone(),
        ) else {
            return Task::none();
        };
        self.mongo.loading = true;
        Task::perform(
            async move {
                let handle = core.ensure_connected(conn).await.map_err(|e| e.to_string())?;
                let mongo = handle.mongo().ok_or("驱动不可用".to_string())?;
                mongo
                    .find(&db, &coll, serde_json::json!({}), 0, 50)
                    .await
                    .map_err(|e| e.to_string())
            },
            Msg::MongoDocs,
        )
    }

    fn mongo_run(&mut self) -> Task<Msg> {
        let (Some(core), Some(conn), Some(db), Some(coll)) = (
            self.core.clone(),
            self.mongo.conn,
            self.mongo.db.clone(),
            self.mongo.collection.clone(),
        ) else {
            return Task::none();
        };
        let pipeline_text = self.mongo.pipeline.clone();
        self.mongo.loading = true;
        Task::perform(
            async move {
                let pipeline: serde_json::Value =
                    serde_json::from_str(&pipeline_text).map_err(|e| format!("聚合管道不是合法 JSON：{e}"))?;
                let handle = core.ensure_connected(conn).await.map_err(|e| e.to_string())?;
                let mongo = handle.mongo().ok_or("驱动不可用".to_string())?;
                mongo
                    .aggregate(&db, &coll, pipeline)
                    .await
                    .map_err(|e| e.to_string())
            },
            Msg::MongoDocs,
        )
    }

    fn open_es(&mut self, conn: ConnId) -> Task<Msg> {
        self.es = EsState { conn: Some(conn), loading: true, ..Default::default() };
        self.page = Page::Es;
        self.active_conn = Some(conn);
        let Some(core) = self.core.clone() else { return Task::none() };
        Task::perform(
            async move {
                let handle = core.ensure_connected(conn).await.map_err(|e| e.to_string())?;
                let es = handle.es().ok_or("驱动不可用".to_string())?;
                let list = es
                    .indices()
                    .await
                    .map_err(|e| e.to_string())?
                    .into_iter()
                    .map(|i| (i.name, i.health, i.docs, i.store))
                    .collect::<Vec<_>>();
                Ok(list)
            },
            Msg::EsLoaded,
        )
    }

    fn es_run(&mut self) -> Task<Msg> {
        let (Some(core), Some(conn), Some(index)) = (self.core.clone(), self.es.conn, self.es.index.clone()) else {
            return Task::none();
        };
        let dsl_text = self.es.dsl.clone();
        self.es.loading = true;
        Task::perform(
            async move {
                let dsl = driver_elasticsearch::EsDriver::validate_dsl(&dsl_text)?;
                let handle = core.ensure_connected(conn).await.map_err(|e| e.to_string())?;
                let es = handle.es().ok_or("驱动不可用".to_string())?;
                let hits = es.search(&index, &dsl).await.map_err(|e| e.to_string())?;
                Ok((hits.total, hits.took_ms, hits.hits, hits.timed_out))
            },
            Msg::EsResult,
        )
    }
}

// ---------------------------------------------------------------------------
// 异步任务辅助
// ---------------------------------------------------------------------------

async fn load_tree_payload(
    core: std::sync::Arc<dbhelper_core::AppCore>,
    id: ConnId,
) -> Result<TreePayload, String> {
    let handle = core.ensure_connected(id).await.map_err(|e| e.to_string())?;
    if let Some(driver) = handle.rel() {
        let databases = driver.list_databases().await.map_err(|e| e.user_message())?;
        let db = databases.first().cloned();
        let objects = driver
            .list_objects(db.as_deref(), &[])
            .await
            .map_err(|e| e.user_message())?;
        let tables: Vec<SchemaObject> = objects
            .iter()
            .filter(|o| matches!(o.kind, ObjectKind::Table | ObjectKind::Collection))
            .cloned()
            .collect();
        // 只把「视图」放进视图分组；索引/触发器/约束不应混进来
        let views: Vec<SchemaObject> = objects
            .iter()
            .filter(|o| matches!(o.kind, ObjectKind::View))
            .cloned()
            .collect();
        Ok(TreePayload { databases, tables, views })
    } else if let Some(mongo) = handle.mongo() {
        let databases = mongo.databases().await.map_err(|e| e.to_string())?;
        let db = databases.first().cloned().unwrap_or_default();
        let colls = mongo.collections(&db).await.map_err(|e| e.to_string())?;
        Ok(TreePayload {
            databases,
            tables: colls
                .into_iter()
                .map(|c| SchemaObject {
                    name: c.name,
                    kind: ObjectKind::Collection,
                    schema: None,
                    row_estimate: Some(c.count),
                    comment: None,
                })
                .collect(),
            views: Vec::new(),
        })
    } else if let Some(redis) = handle.redis() {
        let info = redis.info().await.map_err(|e| e.to_string())?;
        Ok(TreePayload {
            databases: info.keyspace.iter().map(|k| format!("db{}", k.db)).collect(),
            tables: Vec::new(),
            views: Vec::new(),
        })
    } else if let Some(es) = handle.es() {
        let indices = es.indices().await.map_err(|e| e.to_string())?;
        Ok(TreePayload {
            databases: vec!["索引".into()],
            tables: indices
                .into_iter()
                .map(|i| SchemaObject {
                    name: i.name,
                    kind: ObjectKind::Index,
                    schema: None,
                    row_estimate: Some(i.docs),
                    comment: Some(i.health),
                })
                .collect(),
            views: Vec::new(),
        })
    } else {
        Err("不支持的连接类型".into())
    }
}

async fn load_rows(
    core: std::sync::Arc<dbhelper_core::AppCore>,
    conn: ConnId,
    table: String,
    _db: Option<String>,
    page: usize,
    size: usize,
    sort: Option<(String, bool)>,
    filters: Vec<FilterChip>,
    search: String,
) -> Result<(Vec<Row>, u64), String> {
    let handle = core.ensure_connected(conn).await.map_err(|e| e.to_string())?;
    let driver = handle.rel().ok_or("需要关系型连接")?;
    let mut where_parts: Vec<String> = filters
        .iter()
        .map(|f| format!("\"{}\" {} '{}'", f.column, f.op, f.value.replace('\'', "''")))
        .collect();
    if !search.trim().is_empty() {
        where_parts.push(format!("CAST(\"{}\" AS TEXT) LIKE '%{}%'", "name", search.replace('\'', "''")));
    }
    let where_sql = if where_parts.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", where_parts.join(" AND "))
    };
    let order_sql = sort
        .map(|(c, asc)| format!(" ORDER BY \"{c}\" {}", if asc { "ASC" } else { "DESC" }))
        .unwrap_or_default();
    let count_sql = format!("SELECT COUNT(*) FROM \"{table}\"{where_sql}");
    let total: u64 = driver
        .query(&count_sql, 1, Duration::from_secs(20))
        .await
        .map_err(|e| e.user_message())?
        .rows
        .first()
        .and_then(|r| r.first())
        .map(|v| v.display().parse().unwrap_or(0))
        .unwrap_or(0);
    let sql = format!(
        "SELECT * FROM \"{table}\"{where_sql}{order_sql} LIMIT {size} OFFSET {}",
        page * size
    );
    let out = driver
        .query(&sql, size as u32, Duration::from_secs(30))
        .await
        .map_err(|e| e.user_message())?;
    Ok((out.rows, total))
}

fn build_edit_fields(tab: &DataTab, row_index: usize) -> Vec<EditField> {
    let Some(row) = tab.rows.get(row_index) else { return Vec::new() };
    tab.columns
        .iter()
        .enumerate()
        .map(|(i, col)| {
            let value = row.get(i).cloned().unwrap_or(Value::Null);
            let text = match &value {
                Value::Null => String::new(),
                Value::Bool(b) => if *b { "1" } else { "0" }.to_string(),
                other => other.display(),
            };
            let is_pk = tab
                .def
                .as_ref()
                .and_then(|d| d.columns.iter().find(|c| c.name == col.name).map(|c| c.is_pk))
                .unwrap_or(false);
            EditField {
                name: col.name.clone(),
                type_name: col.type_name.clone(),
                original: text.clone(),
                value: text,
                is_pk,
            }
        })
        .collect()
}

fn default_es_dsl(index: &str) -> String {
    format!(
        "{{\n  \"query\": {{ \"bool\": {{ \"must\": [\n    {{ \"match\": {{ \"message\": \"timeout\" }} }},\n    {{ \"range\": {{ \"@timestamp\": {{ \"gte\": \"now-15m\" }} }} }}\n  ] }} }},\n  \"size\": 50,\n  \"sort\": [ {{ \"@timestamp\": {{ \"order\": \"desc\" }} }} ]\n}}"
    )
    .replace("{index}", index)
}

/// 无边框窗口的最大化/还原：`toggle_maximize` 在 decorations=false 时不生效，
/// 这里通过 `window::run` 拿到 HWND 后直接调用 ShowWindow。
fn maximize_window(id: iced::window::Id, maximize: bool) -> Task<Msg> {
    #[cfg(windows)]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        const SW_MAXIMIZE: i32 = 3;
        const SW_RESTORE: i32 = 9;
        extern "system" {
            fn ShowWindow(hwnd: *mut core::ffi::c_void, cmd: i32) -> i32;
        }
        iced::window::run(id, move |window| {
            if let Ok(handle) = window.window_handle() {
                if let RawWindowHandle::Win32(win32) = handle.as_raw() {
                    let hwnd = win32.hwnd.get() as *mut core::ffi::c_void;
                    unsafe {
                        ShowWindow(hwnd, if maximize { SW_MAXIMIZE } else { SW_RESTORE });
                    }
                }
            }
        })
        .map(|_| Msg::Noop)
    }
    #[cfg(not(windows))]
    {
        let _ = (id, maximize);
        Task::none()
    }
}
