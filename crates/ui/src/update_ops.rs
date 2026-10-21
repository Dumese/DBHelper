//! 复合行为：查询执行、流式接收、行内提交、向导、导出等。

use crate::state::*;
use crate::theme::Tone;
use crate::widgets::editor::{apply_edit, selected_text, EditOp};
use crate::{App, Msg};
use dbhelper_core::registry::Handle;
use dbhelper_models::*;
use dbhelper_utils::sql as sqlutil;
use futures::StreamExt;
use iced::Task;
use iced::keyboard::{key, Key, Modifiers};
use std::time::Duration;

impl App {
    // -----------------------------------------------------------------------
    // 编辑器键盘
    // -----------------------------------------------------------------------

    pub(crate) fn on_editor_key(&mut self, event: iced::keyboard::Event) -> Task<Msg> {
        let iced::keyboard::Event::KeyPressed { key, modifiers, .. } = event else {
            return Task::none();
        };
        // 先取编辑器状态副本，不持有 &mut self，后续可自由改 self.completion 等
        let Some((mut sql, mut caret, mut anchor)) = self
            .current_query()
            .map(|tab| (tab.sql.clone(), tab.caret, tab.anchor))
        else {
            return Task::none();
        };
        let completion_visible = self.completion.visible;
        let completion_selected = self.completion.selected;
        let completion_len = self.completion.items.len();

        let op = match (&key, modifiers.control()) {
            // 补全：Ctrl+Space 呼出，Esc/Enter 交互
            (Key::Character(c), true) if c.eq_ignore_ascii_case(" ") => {
                return self.update(Msg::CompletionShow);
            }
            (Key::Named(key::Named::Escape), _) if completion_visible => {
                return self.update(Msg::CompletionHide);
            }
            (Key::Named(key::Named::Enter), _) if completion_visible => {
                return self.update(Msg::CompletionPick(completion_selected));
            }
            (Key::Named(key::Named::ArrowDown), _) if completion_visible => {
                self.completion.selected = (completion_selected + 1).min(completion_len.saturating_sub(1));
                return Task::none();
            }
            (Key::Named(key::Named::ArrowUp), _) if completion_visible => {
                self.completion.selected = completion_selected.saturating_sub(1);
                return Task::none();
            }
            (Key::Character(c), true) if c.eq_ignore_ascii_case("a") => Some(EditOp::SelectAll),
            (Key::Character(c), true) if c.eq_ignore_ascii_case("c") => {
                if let Some(text) = selected_text(&sql, caret, anchor) {
                    return iced::clipboard::write(text);
                }
                None
            }
            (Key::Character(c), true) if c.eq_ignore_ascii_case("x") => {
                if let Some(text) = selected_text(&sql, caret, anchor) {
                    apply_edit(&mut sql, &mut caret, &mut anchor, EditOp::DeleteSelection);
                    if let Some(tab) = self.current_query_mut() {
                        tab.sql = sql;
                        tab.caret = caret;
                        tab.anchor = anchor;
                    }
                    return iced::clipboard::write(text);
                }
                None
            }
            (Key::Character(c), true) if c.eq_ignore_ascii_case("v") => {
                self.editor_focus = true;
                return iced::clipboard::read().map(Msg::EditorPaste);
            }
            (Key::Character(c), true) if c.eq_ignore_ascii_case("s") => {
                return self.update(Msg::QuerySave);
            }
            (Key::Character(c), true) if c.eq_ignore_ascii_case("l") => Some(EditOp::SelectAll),
            (Key::Named(key::Named::Enter), true) if modifiers.control() => {
                return self.update(Msg::QueryRun);
            }
            (Key::Character(c), false) => Some(EditOp::Insert(c.to_string())),
            (Key::Character(c), _) => Some(EditOp::Insert(c.to_string())),
            (Key::Named(key::Named::Enter), _) => Some(EditOp::Newline),
            (Key::Named(key::Named::Backspace), _) => Some(EditOp::Backspace),
            (Key::Named(key::Named::Delete), _) => Some(EditOp::Delete),
            (Key::Named(key::Named::Tab), _) => Some(EditOp::Tab),
            (Key::Named(key::Named::ArrowLeft), _) => Some(EditOp::MoveLeft(modifiers.shift())),
            (Key::Named(key::Named::ArrowRight), _) => Some(EditOp::MoveRight(modifiers.shift())),
            (Key::Named(key::Named::ArrowUp), _) => Some(EditOp::MoveUp(modifiers.shift())),
            (Key::Named(key::Named::ArrowDown), _) => Some(EditOp::MoveDown(modifiers.shift())),
            (Key::Named(key::Named::Home), _) => Some(EditOp::Home(modifiers.shift())),
            (Key::Named(key::Named::End), _) => Some(EditOp::End(modifiers.shift())),
            _ => None,
        };

        if let Some(op) = op {
            apply_edit(&mut sql, &mut caret, &mut anchor, op);
            if let Some(tab) = self.current_query_mut() {
                tab.sql = sql;
                tab.caret = caret;
                tab.anchor = anchor;
            }
            self.caret_blink = true;
        }
        let _ = Modifiers::default();
        Task::none()
    }

    // -----------------------------------------------------------------------
    // 查询执行
    // -----------------------------------------------------------------------

    pub(crate) fn run_current_query(&mut self) -> Task<Msg> {
        let Some(tab) = self.current_query() else { return Task::none() };
        let idx = tab.index;
        let conn = tab.conn;
        let sql = tab.sql.clone();
        if sql.trim().is_empty() {
            self.toast("请输入 SQL 语句", Tone::Warning);
            return Task::none();
        }
        self.exec_sql(idx, conn, sql)
    }

    pub(crate) fn exec_sql(&mut self, idx: usize, conn: ConnId, sql: String) -> Task<Msg> {
        let Some(core) = self.core.clone() else { return Task::none() };
        if let Some(tab) = self.query_tabs.get_mut(idx) {
            tab.running = true;
            tab.error = None;
            tab.notices.clear();
            tab.columns.clear();
            tab.rows.clear();
            tab.elapsed_ms = 0;
            tab.page = 0;
            tab.started = Some(std::time::Instant::now());
        }
        // 高危语句二次确认
        if let Some(danger) = sqlutil::danger_of(&sql) {
            let preview = sql.chars().take(160).collect::<String>();
            self.confirm = Some(Confirm {
                title: "确认执行高危语句".into(),
                message: danger.summary(),
                detail: Some(preview),
                danger: true,
                action: ConfirmAction::RunSql { conn, sql: sql.clone() },
            });
            let core2 = core.clone();
            let sql2 = sql.clone();
            // 影响行数预估（异步填充到弹层 detail 之后的说明里）
            let estimate = Task::perform(
                async move { core2.estimate_affected(conn, &sql2).await },
                move |n| Msg::EstimateReady(n),
            );
            if let Some(tab) = self.query_tabs.get_mut(idx) {
                tab.running = false;
            }
            return estimate;
        }
        self.start_query(idx, conn, sql)
    }

    pub(crate) fn start_query(&mut self, idx: usize, conn: ConnId, sql: String) -> Task<Msg> {
        let Some(core) = self.core.clone() else { return Task::none() };
        Task::perform(
            async move {
                core.run_query(conn, sql)
                    .await
                    .map(|rx| std::sync::Arc::new(tokio::sync::Mutex::new(Some(rx))))
                    .map_err(|e| e.to_string())
            },
            move |r| Msg::QueryStarted(idx, r),
        )
    }

    pub(crate) fn on_stream(&mut self, idx: usize, event: StreamEvent) -> Task<Msg> {
        let Some(tab) = self.query_tabs.get_mut(idx) else {
            tracing::warn!("ui: 流事件找不到查询标签 idx={idx}");
            return Task::none();
        };
        match event {
            StreamEvent::Columns { columns, read_only } => {
                tab.columns = columns;
                tab.read_only = read_only;
            }
            StreamEvent::Batch(rows) => {
                tab.rows.extend(rows);
            }
            StreamEvent::Done { elapsed_ms, rows_affected, truncated } => {
                tab.running = false;
                tab.elapsed_ms = elapsed_ms;
                tab.truncated = truncated;
                if tab.columns.is_empty() && rows_affected > 0 {
                    tab.notices.push(format!("影响 {rows_affected} 行 · {elapsed_ms} ms"));
                }
            }
            StreamEvent::Failed(e) => {
                tab.running = false;
                tab.error = Some(e);
            }
        }
        Task::none()
    }

    pub(crate) fn export_current(&mut self) -> Task<Msg> {
        let Some(tab) = self.current_query() else { return Task::none() };
        if tab.rows.is_empty() {
            self.toast("当前没有可导出的结果集", Tone::Warning);
            return Task::none();
        }
        Task::perform(
            async {
                rfd::AsyncFileDialog::new()
                    .set_file_name("result.csv")
                    .add_filter("CSV", &["csv"])
                    .save_file()
                    .await
                    .map(|f| f.path().to_string_lossy().to_string())
            },
            Msg::ExportPicked,
        )
    }

    // -----------------------------------------------------------------------
    // 行内提交
    // -----------------------------------------------------------------------

    pub(crate) fn commit_edit(&mut self) -> Task<Msg> {
        let Some(tab) = self.current_data() else { return Task::none() };
        if tab.pending() == 0 {
            self.toast("没有待提交的修改", Tone::Neutral);
            return Task::none();
        }
        let conn = tab.conn;
        let table = tab.table.clone();
        let mut pk: Vec<(String, Value)> = Vec::new();
        for field in tab.edit.iter().filter(|f| f.is_pk) {
            pk.push((field.name.clone(), typed_value(&field.type_name, &field.original)));
        }
        if pk.is_empty() {
            // 无主键时退回第一列
            if let Some(first) = tab.edit.first() {
                pk.push((first.name.clone(), typed_value(&first.type_name, &first.original)));
            }
        }
        let changes: Vec<(String, Value)> = tab
            .edit
            .iter()
            .filter(|f| f.value != f.original)
            .map(|f| (f.name.clone(), typed_value(&f.type_name, &f.value)))
            .collect();
        if changes.is_empty() {
            return Task::none();
        }
        let Some(core) = self.core.clone() else { return Task::none() };
        self.toast("正在提交…", Tone::Neutral);
        Task::perform(
            async move {
                let handle = core.ensure_connected(conn).await.map_err(|e| e.to_string())?;
                let driver = handle.rel().ok_or("需要关系型连接".to_string())?;
                let writer = driver.writer().ok_or("该驱动暂不支持行内编辑，请使用 SQL 编辑器")?;
                writer
                    .update_row(&table, &pk, &changes)
                    .await
                    .map_err(|e| e.user_message())
            },
            Msg::DataCommitted,
        )
    }

    // -----------------------------------------------------------------------
    // 向导
    // -----------------------------------------------------------------------

    pub(crate) fn pick_sqlite_file(&mut self) -> Task<Msg> {
        Task::perform(
            async {
                rfd::AsyncFileDialog::new()
                    .add_filter("SQLite 数据库", &["db", "sqlite", "sqlite3"])
                    .pick_file()
                    .await
                    .map(|f| f.path().to_string_lossy().to_string())
            },
            |r: Option<String>| Msg::WizardDatabase(r.unwrap_or_default()),
        )
    }

    pub(crate) fn wizard_test(&mut self) -> Task<Msg> {
        let Some(w) = self.wizard.clone() else { return Task::none() };
        if let Some(err) = w.validate() {
            if let Some(w) = &mut self.wizard {
                w.error = Some(err);
            }
            return Task::none();
        }
        let cfg = w.to_config(w.edit_id.unwrap_or(0));
        let password = w.password.clone();
        if let Some(w) = &mut self.wizard {
            w.testing = true;
            w.test_result = None;
        }
        let Some(core) = self.core.clone() else { return Task::none() };
        Task::perform(
            async move { core.test_connection(&cfg, &password).await },
            Msg::WizardTested,
        )
    }

    pub(crate) fn wizard_save(&mut self) -> Task<Msg> {
        let Some(w) = self.wizard.clone() else { return Task::none() };
        if let Some(err) = w.validate() {
            if let Some(w) = &mut self.wizard {
                w.error = Some(err);
            }
            return Task::none();
        }
        let Some(core) = self.core.clone() else { return Task::none() };
        if let Some(w) = &mut self.wizard {
            w.saving = true;
        }
        let edit_id = w.edit_id;
        let template = w.to_config(edit_id.unwrap_or(0));
        let password = w.password.clone();
        Task::perform(
            async move {
                let id = match edit_id {
                    Some(id) => id,
                    None => core.store.next_connection_id().await,
                };
                let mut cfg = template;
                cfg.id = id;
                cfg.secret_key = Some(format!("conn:{id}"));
                core.save_connection(&cfg, Some(&password)).await.map_err(|e| e.to_string())
            },
            Msg::WizardSaved,
        )
    }

    // -----------------------------------------------------------------------
    // 导出
    // -----------------------------------------------------------------------

    pub(crate) fn export_ddl(&mut self) -> Task<Msg> {
        let Some(conn) = self.er.conn else { return Task::none() };
        let Some(core) = self.core.clone() else { return Task::none() };
        let tables: Vec<String> = self.er.nodes.iter().map(|n| n.table.name.clone()).collect();
        Task::perform(
            async move {
                let handle = core.ensure_connected(conn).await.map_err(|e| e.to_string())?;
                let driver = handle.rel().ok_or("需要关系型连接".to_string())?;
                let mut out = String::new();
                for table in &tables {
                    if let Ok(def) = driver.describe_table(None, table).await {
                        if let Ok(ddl) = driver.create_table_ddl(&def, None).await {
                            out.push_str(&ddl);
                            out.push_str("\n\n");
                        }
                    }
                }
                Ok::<String, String>(out)
            },
            |r: Result<String, String>| match r {
                Ok(ddl) => Msg::DdlReady(ddl),
                Err(e) => Msg::DdlSaved(Err(e)),
            },
        )
    }

    pub(crate) fn run_export_wizard(&mut self) -> Task<Msg> {
        let Some(core) = self.core.clone() else { return Task::none() };
        let items = self.settings_ui.export_items;
        let settings = self.settings.clone();
        let format = self.settings_ui.export_format;
        let conns = self.conns.clone();
        Task::perform(
            async move {
                let mut payload = serde_json::Map::new();
                if items[0] {
                    let safe: Vec<serde_json::Value> = conns
                        .iter()
                        .map(|c| {
                            serde_json::json!({
                                "name": c.name, "kind": c.kind.label(), "host": c.host,
                                "port": c.port, "database": c.database, "group": c.group,
                                "readonly": c.readonly,
                            })
                        })
                        .collect();
                    payload.insert("connections".into(), serde_json::Value::Array(safe));
                }
                if items[1] {
                    let history = core.store.history(200).await.map_err(|e| e.to_string())?;
                    payload.insert("history".into(), serde_json::to_value(&history).unwrap_or_default());
                }
                if items[2] {
                    payload.insert("settings".into(), serde_json::to_value(&settings).unwrap_or_default());
                }
                if items[3] {
                    let snippets = core.store.snippets().await.map_err(|e| e.to_string())?;
                    payload.insert("snippets".into(), serde_json::to_value(&snippets).unwrap_or_default());
                }
                let json = serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?;
                let path = rfd::AsyncFileDialog::new()
                    .set_file_name(match format {
                        0 => "dbhelper-export.json",
                        1 => "dbhelper-export.csv",
                        _ => "dbhelper-export.sql",
                    })
                    .save_file()
                    .await
                    .map(|f| f.path().to_string_lossy().to_string())
                    .ok_or("已取消".to_string())?;
                std::fs::write(&path, json).map_err(|e| e.to_string())?;
                Ok::<usize, String>(1)
            },
            Msg::ExportDone,
        )
    }
}

/// 依据列类型把编辑框文本转成强类型值。
pub fn typed_value(type_name: &str, text: &str) -> Value {
    let t = type_name.to_ascii_lowercase();
    if text.is_empty() {
        return Value::Null;
    }
    if t.contains("int") {
        if let Ok(i) = text.parse::<i64>() {
            return Value::Int(i);
        }
    }
    if t.contains("decimal") || t.contains("numeric") || t.contains("float") || t.contains("double") || t.contains("real") {
        if let Ok(f) = text.parse::<f64>() {
            return Value::Float(f);
        }
    }
    if t.contains("bool") || t.contains("bit") {
        if text == "1" || text.eq_ignore_ascii_case("true") {
            return Value::Bool(true);
        }
        if text == "0" || text.eq_ignore_ascii_case("false") {
            return Value::Bool(false);
        }
    }
    if t.contains("date") || t.contains("time") {
        return Value::Timestamp(text.to_string());
    }
    Value::Text(text.to_string())
}

impl App {
    pub(crate) fn confirm_run(&mut self) -> Task<Msg> {
        let Some(confirm) = self.confirm.take() else { return Task::none() };
        match confirm.action {
            ConfirmAction::RunSql { conn, sql } => {
                let idx = self
                    .current_query()
                    .map(|t| t.index)
                    .unwrap_or_else(|| {
                        if self.query_tabs.is_empty() {
                            0
                        } else {
                            self.query_tabs.len() - 1
                        }
                    });
                if self.query_tabs.is_empty() {
                    let _ = self.new_query_tab_for(conn);
                }
                self.start_query(idx, conn, sql)
            }
            ConfirmAction::DeleteConnection(id) => {
                let Some(core) = self.core.clone() else { return Task::none() };
                Task::perform(
                    async move {
                        let _ = core.delete_connection(id).await;
                    },
                    |_| Msg::RefreshTree,
                )
            }
            ConfirmAction::DeleteKey(key) => {
                let (Some(core), Some(conn)) = (self.core.clone(), self.redis.conn) else {
                    return Task::none();
                };
                self.redis.selected = None;
                self.redis.fields.clear();
                Task::perform(
                    async move {
                        let handle = core.ensure_connected(conn).await.map_err(|e| e.to_string())?;
                        let redis = handle.redis().ok_or("驱动不可用".to_string())?;
                        redis.delete(&key).await.map_err(|e| e.to_string())?;
                        Ok::<(), String>(())
                    },
                    |_| Msg::RedisScan(String::new()),
                )
            }
            ConfirmAction::KillSession(id) => {
                let (Some(core), Some(conn)) = (self.core.clone(), self.monitor.conn) else {
                    return Task::none();
                };
                Task::perform(
                    async move {
                        let handle = core.ensure_connected(conn).await.map_err(|e| e.to_string())?;
                        let driver = handle.rel().ok_or("驱动不可用".to_string())?;
                        driver.kill_session(&id).await.map_err(|e| e.user_message())
                    },
                    |r: Result<(), String>| match r {
                        Ok(()) => Msg::Noop,
                        Err(e) => Msg::DdlSaved(Err(e)),
                    },
                )
            }
            ConfirmAction::DeleteDoc(_) => Task::none(),
        }
    }
}

// 让 mpsc 接收端可直接转为 iced 流
pub(crate) fn stream_from_receiver(
    idx: usize,
    rx: tokio::sync::mpsc::Receiver<StreamEvent>,
) -> impl futures::Stream<Item = Msg> {
    futures::stream::unfold(rx, move |mut rx| async move {
        rx.recv().await.map(|ev| (Msg::Stream(idx, ev), rx))
    })
}

impl App {
    pub(crate) fn open_query_with(&mut self, conn: ConnId, sql: String) -> Task<Msg> {
        let t = self.new_query_tab_for(conn);
        if let Some(tab) = self.query_tabs.last_mut() {
            tab.sql = sql;
        }
        t
    }
}

impl App {
    pub(crate) fn monitor_tick(&mut self) -> Task<Msg> {
        Task::none()
    }
}

impl App {
    pub(crate) fn handle_handle(&self, handle: &Handle) -> bool {
        matches!(handle, Handle::Rel(_))
    }
}

impl App {
    pub(crate) fn wait(&self, _d: Duration) {}
}

impl App {
    /// 载入当前连接的全部表结构用于 ER 图。
    pub(crate) fn load_er(&mut self, conn: ConnId) -> Task<Msg> {
        let Some(core) = self.core.clone() else { return Task::none() };
        self.er.loading = true;
        self.page = crate::state::Page::Er;
        self.active_conn = Some(conn);
        if !self.tabs.iter().any(|t| t.kind == crate::state::TabKind::Er) {
            self.tabs.push(crate::state::Tab {
                title: "ER 图".into(),
                kind: crate::state::TabKind::Er,
                conn,
                db: None,
                table: None,
                index: 0,
            });
            self.active_tab = self.tabs.len() - 1;
        }
        Task::perform(
            async move {
                let handle = core.ensure_connected(conn).await.map_err(|e| e.to_string())?;
                let driver = handle.rel().ok_or("需要关系型连接".to_string())?;
                let objects = driver
                    .list_objects(None, &[dbhelper_models::ObjectKind::Table])
                    .await
                    .map_err(|e| e.user_message())?;
                let mut defs = Vec::new();
                for obj in objects.into_iter().take(24) {
                    if let Ok(def) = driver.describe_table(None, &obj.name).await {
                        defs.push(def);
                    }
                }
                Ok(defs)
            },
            move |r| Msg::ErLoaded(conn, r),
        )
    }
}
