//! 外壳：标题栏、面包屑、页签、侧栏、状态栏。

use crate::icons::{icon, Icon};
use crate::kit::{self, Ctx};
use crate::state::{Page, Sidebar, TabKind};
use crate::theme::{radius, size as ts, space, Tokens, Tone};
use crate::{App, Msg};
use dbhelper_models::*;
use iced::widget::{button, column, container, mouse_area, row, scrollable, stack, text, Space};
use iced::{Alignment, Border, Color, Element, Length, Padding, Theme};

pub fn titlebar<'a>(app: &'a App, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;
    let tr = ctx.tr;
    let title = match app.page {
        Page::Connections => tr.conn_management().to_string(),
        Page::Query => app
            .current_query()
            .map(|q| format!("{} — {}", q.title, app.conn_display(q.conn)))
            .unwrap_or_else(|| tr.query().into()),
        Page::Data => app
            .current_data()
            .map(|d| format!("{} — {}", tr.data(), d.table))
            .unwrap_or_else(|| tr.data().into()),
        Page::Er => format!("{} — {}", tr.design(), tr.er_diagram()),
        Page::Redis => format!("{} — Redis", tr.cache()),
        Page::Mongo => format!("appdb.users — {}", tr.documents()),
        Page::Es => format!("{} — DSL", tr.logs()),
        Page::Monitor => format!("{} — {}", tr.monitor(), tr.server()),
        Page::Settings => tr.settings().into(),
        Page::Designer => format!("表设计 — {}", app.designer.table),
        Page::Dash => "BI 仪表板".into(),
        Page::Transfer => "迁移与同步".into(),
        Page::Admin => "服务器管理".into(),
    };

    let left = row![
        crate::icons::fox_logo(18.0),
        kit::gap(8.0, 1.0),
        text("DBHelper").size(ctx.f(13.0)).font(ctx.ui_bold()).color(t.text),
        kit::gap(10.0, 1.0),
        kit::vline(ctx, 14.0),
        kit::gap(10.0, 1.0),
        text(title).size(ctx.f(13.0)).font(ctx.ui).color(t.text_secondary),
    ]
    .align_y(Alignment::Center);

    let controls = row![
        win_btn(ctx, Icon::Minus, Msg::WindowMinimize, false),
        win_btn(ctx, Icon::Square, Msg::WindowMaximize, false),
        win_btn(ctx, Icon::Close, Msg::WindowClose, true),
    ]
    .spacing(2);

    // 拖拽区只覆盖左侧留白，窗口按钮不再被 mouse_area 吞掉点击
    let drag = mouse_area(
        container(row![left, kit::hspace()])
            .height(Length::Fixed(32.0))
            .align_y(Alignment::Center)
            .padding(Padding { top: 0.0, right: 6.0, bottom: 0.0, left: 12.0 }),
    )
    .on_press(Msg::WindowDrag);

    let bar = container(
        row![drag, controls]
            .align_y(Alignment::Center)
            .padding(Padding { top: 0.0, right: 6.0, bottom: 0.0, left: 0.0 }),
    )
    .height(Length::Fixed(32.0))
    .align_y(Alignment::Center)
    .width(Length::Fill)
    .style(move |_: &Theme| container::Style {
        background: Some(t.surface.into()),
        border: Border { color: t.hairline, width: 1.0, radius: 0.0.into() },
        ..Default::default()
    });

    bar.into()
}

fn win_btn<'a>(ctx: Ctx, ic: Icon, msg: Msg, danger: bool) -> Element<'a, Msg> {
    let t = ctx.t;
    button(icon(ic, 12.0, t.text_secondary))
        .padding(Padding { top: 6.0, right: 9.0, bottom: 6.0, left: 9.0 })
        .style(move |_: &Theme, status| button::Style {
            background: Some(
                match status {
                    button::Status::Hovered if danger => t.error,
                    button::Status::Hovered => t.hover,
                    _ => Color::TRANSPARENT,
                }
                .into(),
            ),
            text_color: t.text,
            border: Border::default().rounded(radius::XS),
            ..Default::default()
        })
        .on_press(msg)
        .into()
}

pub fn breadcrumb<'a>(app: &'a App, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;
    let tr = ctx.tr;
    let mut crumbs: Vec<String> = vec![tr.connections().into()];
    match app.page {
        Page::Connections => crumbs.push(tr.all_connections().into()),
        Page::Settings => {
            crumbs = vec!["DBHelper".into(), tr.settings().into()];
        }
        Page::Query => {
            if let Some(q) = app.current_query() {
                crumbs.push(app.conn_display(q.conn));
                crumbs.push(app.database.clone().unwrap_or_else(|| "mydb".into()));
                crumbs.push(q.title.clone());
            }
        }
        Page::Data => {
            if let Some(d) = app.current_data() {
                crumbs.push(app.conn_display(d.conn));
                crumbs.push(d.db.clone().unwrap_or_else(|| app.database.clone().unwrap_or_default()));
                crumbs.push(tr.table().into());
                crumbs.push(d.table.clone());
                crumbs.push(tr.data().into());
            }
        }
        Page::Er => {
            if let Some(conn) = app.er.conn {
                crumbs.push(app.conn_display(conn));
                crumbs.push(app.database.clone().unwrap_or_else(|| "mydb".into()));
                crumbs.push(tr.design().into());
                crumbs.push(tr.er_diagram().into());
            }
        }
        Page::Redis => {
            if let Some(conn) = app.redis.conn {
                crumbs.push(app.conn_display(conn));
                crumbs.push(format!("db{}", app.redis.db));
                crumbs.push(tr.key().into());
                if let Some(key) = &app.redis.selected {
                    crumbs.push(key.clone());
                }
            }
        }
        Page::Mongo => {
            if let Some(conn) = app.mongo.conn {
                crumbs.push(app.conn_display(conn));
                crumbs.push(app.mongo.db.clone().unwrap_or_default());
                if let Some(c) = &app.mongo.collection {
                    crumbs.push(c.clone());
                    crumbs.push(tr.documents().into());
                }
            }
        }
        Page::Es => {
            if let Some(conn) = app.es.conn {
                crumbs.push(app.conn_display(conn));
                crumbs.push(tr.index().into());
                if let Some(i) = &app.es.index {
                    crumbs.push(i.clone());
                    crumbs.push(tr.query().into());
                }
            }
        }
        Page::Monitor => {
            if let Some(conn) = app.monitor.conn {
                crumbs.push(app.conn_display(conn));
                crumbs.push(tr.monitor().into());
                crumbs.push(tr.server().into());
            }
        }
        Page::Designer => {
            crumbs.push("对象设计器".into());
            crumbs.push(app.designer.table.clone());
        }
        Page::Dash => {
            crumbs.push("BI".into());
            if let Some(p) = app.dash.pages.get(app.dash.active) {
                crumbs.push(p.name.clone());
            }
        }
        Page::Transfer => {
            crumbs.push("迁移与同步".into());
        }
        Page::Admin => {
            if let Some(conn) = app.admin.conn {
                crumbs.push(app.conn_display(conn));
            }
            crumbs.push("服务器管理".into());
        }
    }

    let mut r = row![].spacing(6).align_y(Alignment::Center);
    let n = crumbs.len();
    for (i, c) in crumbs.iter().enumerate() {
        if i == 0 && !matches!(app.page, Page::Connections) {
            r = r.push(
                button(
                    text(c.clone())
                        .size(ctx.f(ts::SMALL))
                        .font(ctx.ui)
                        .color(t.text_secondary),
                )
                .padding(0)
                .style(move |_: &Theme, status| button::Style {
                    background: if matches!(status, button::Status::Hovered) {
                        Some(t.hover.into())
                    } else {
                        None
                    },
                    text_color: t.text_secondary,
                    border: Border::default().rounded(radius::XS),
                    ..Default::default()
                })
                .on_press(Msg::GoConnections),
            );
        } else {
            r = r.push(
                text(c.clone())
                    .size(ctx.f(ts::SMALL))
                    .font(if i + 1 == n { ctx.ui_medium() } else { ctx.ui })
                    .color(if i + 1 == n { t.text } else { t.text_secondary }),
            );
        }
        if i + 1 < n {
            r = r.push(
                text("/")
                    .size(ctx.f(ts::SMALL))
                    .font(ctx.ui)
                    .color(t.text_tertiary),
            );
        }
    }

    let actions: Element<Msg> = row![
        kit::icon_button(ctx, Icon::Refresh, 12.5, t.text_secondary, Msg::RefreshTree),
        kit::icon_button(ctx, Icon::Chart, 12.5, t.text_secondary, Msg::DashOpen(app.active_conn.unwrap_or(0))),
        kit::icon_button(ctx, Icon::Download, 12.5, t.text_secondary, Msg::ImportOpen),
        kit::icon_button(ctx, Icon::Save, 12.5, t.text_secondary, Msg::TransferOpen),
        kit::icon_button(ctx, Icon::Lock, 12.5, t.text_secondary, Msg::AdminOpen(app.active_conn.unwrap_or(0))),
        kit::icon_button(ctx, Icon::More, 12.5, t.text_secondary, Msg::ConnMenu(0)),
    ]
    .spacing(2)
    .into();

    container(
        row![r, kit::hspace(), actions]
            .align_y(Alignment::Center)
            .padding(Padding { top: 0.0, right: 12.0, bottom: 0.0, left: 14.0 }),
    )
    .height(Length::Fixed(40.0))
    .align_y(Alignment::Center)
    .width(Length::Fill)
    .style(move |_: &Theme| container::Style {
        background: Some(t.bg.into()),
        border: Border { color: t.hairline, width: 1.0, radius: 0.0.into() },
        ..Default::default()
    })
    .into()
}

pub fn tabbar<'a>(app: &'a App, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;
    let mut r = row![].spacing(2).align_y(Alignment::Center).height(Length::Fixed(36.0));
    for (i, tab) in app.tabs.iter().enumerate() {
        let active = i == app.active_tab;
        let label = tab.title.clone();
        r = r.push(
            button(
                text(label)
                    .size(ctx.f(ts::SMALL))
                    .font(if active { ctx.ui_medium() } else { ctx.ui })
                    .color(if active { t.text } else { t.text_secondary }),
            )
            .padding(Padding { top: 0.0, right: 10.0, bottom: 0.0, left: 10.0 })
            .height(Length::Fixed(24.0))
            .style(move |_: &Theme, status| button::Style {
                background: Some(
                    if active {
                        t.hover
                    } else if matches!(status, button::Status::Hovered) {
                        t.soft
                    } else {
                        Color::TRANSPARENT
                    }
                    .into(),
                ),
                text_color: t.text,
                border: Border::default().rounded(radius::SM),
                ..Default::default()
            })
            .on_press(Msg::SelectTab(i)),
        );
    }
    r = r.push(
        button(icon(Icon::Plus, 12.0, t.text_secondary))
            .padding(Padding { top: 4.0, right: 6.0, bottom: 4.0, left: 6.0 })
            .height(Length::Fixed(24.0))
            .style(move |_: &Theme, status| button::Style {
                background: if matches!(status, button::Status::Hovered) {
                    Some(t.hover.into())
                } else {
                    None
                },
                text_color: t.text,
                border: Border::default().rounded(radius::SM),
                ..Default::default()
            })
            .on_press(Msg::NewQueryTab),
    );

    container(r)
        .height(Length::Fixed(36.0))
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .padding(Padding { top: 0.0, right: 8.0, bottom: 0.0, left: 4.0 })
        .style(move |_: &Theme| container::Style {
            background: Some(t.bg.into()),
            border: Border { color: t.hairline, width: 1.0, radius: 0.0.into() },
            ..Default::default()
        })
        .into()
}

pub fn sidebar<'a>(app: &'a App, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;
    let body: Element<Msg> = match app.page {
        Page::Redis => keyspace_tree(app, ctx),
        Page::Mongo => collections_tree(app, ctx),
        Page::Es => indices_tree(app, ctx),
        Page::Settings => settings_nav(app, ctx),
        _ => connection_tree(app, ctx),
    };

    column![
        body,
        kit::hspace(),
        vault_footer(app, ctx)
    ]
        .width(Length::Fixed(240.0))
        .height(Length::Fill)
        .into()
}

fn sidebar_header<'a>(ctx: Ctx, title: &str, add: Option<Msg>, more: Option<Msg>) -> Element<'a, Msg> {
    let t = ctx.t;
    let mut r = row![
        text(title.to_string())
            .size(ctx.f(ts::SMALL))
            .font(ctx.ui_medium())
            .color(t.text_secondary)
    ]
    .align_y(Alignment::Center);
    r = r.push(kit::hspace());
    if let Some(m) = add {
        r = r.push(kit::icon_button(ctx, Icon::Plus, 13.0, t.text_secondary, m));
    }
    if let Some(m) = more {
        r = r.push(kit::icon_button(ctx, Icon::More, 13.0, t.text_secondary, m));
    }
    container(r)
        .padding(Padding { top: 10.0, right: 10.0, bottom: 6.0, left: 14.0 })
        .width(Length::Fill)
        .into()
}

fn connection_tree<'a>(app: &'a App, ctx: Ctx) -> Element<'a, Msg> {
    let tr = ctx.tr;
    let mut list = column![].spacing(0).width(Length::Fill);

    // 与原型一致：连接为扁平列表（分组只出现在右栏），无分组头
    let filter = app.search.to_lowercase();
    for conn in &app.conns {
        if !filter.is_empty() && !conn.name.to_lowercase().contains(&filter) {
            continue;
        }
        let key = format!("conn:{}", conn.id);
        let open = app.expanded.contains(&key);
        let connected = app.connected.contains(&conn.id);
        let selected = app.active_conn == Some(conn.id) && matches!(app.page, Page::Connections);
        list = list.push(conn_row(ctx, conn, open, connected, selected));

        if !open {
            continue;
        }
        let dbs = app.tree.databases.get(&conn.id).cloned().unwrap_or_default();
        for db in dbs {
            let dbkey = format!("db:{}:{}", conn.id, db);
            let dbopen = app.expanded.contains(&dbkey);
            let tables = app
                .tree
                .tables
                .get(&(conn.id, db.clone()))
                .cloned()
                .unwrap_or_default();
            let views = app
                .tree
                .views
                .get(&(conn.id, db.clone()))
                .cloned()
                .unwrap_or_default();
            list = list.push(db_row(ctx, conn.id, &db, dbopen, dbkey.clone()));
            if !dbopen {
                continue;
            }
            let tkey = format!("tables:{}:{}", conn.id, db);
            let topen = app.expanded.contains(&tkey);
            list = list.push(group_count_row(ctx, tr.table(), tables.len(), topen, tkey.clone(), Icon::Table));
            if topen {
                for obj in &tables {
                    let sel = app.selected_node.as_deref() == Some(&format!("t:{}:{}", conn.id, obj.name));
                    list = list.push(obj_row(
                        ctx,
                        conn.id,
                        &obj.name,
                        Icon::Table,
                        sel,
                        Msg::OpenTable(conn.id, obj.name.clone()),
                        format!("t:{}:{}", conn.id, obj.name),
                    ));
                }
            }
            if !views.is_empty() {
                let vkey = format!("views:{}:{}", conn.id, db);
                let vopen = app.expanded.contains(&vkey);
                list = list.push(group_count_row(ctx, tr.view(), views.len(), vopen, vkey.clone(), Icon::Eye));
                if vopen {
                    for obj in &views {
                        list = list.push(obj_row(
                            ctx,
                            conn.id,
                            &obj.name,
                            Icon::Eye,
                            false,
                            Msg::OpenTable(conn.id, obj.name.clone()),
                            format!("v:{}:{}", conn.id, obj.name),
                        ));
                    }
                }
            }
        }
    }

    let search = kit::search_field(ctx, tr.search_conns(), &app.search, Msg::SearchChanged);

    column![
        sidebar_header(
            ctx,
            tr.connections(),
            Some(Msg::WizardOpen),
            Some(Msg::ConnMenu(-1)),
        ),
        container(search).padding(Padding { top: 0.0, right: 10.0, bottom: 6.0, left: 10.0 }),
        scrollable(list).height(Length::Fill).style(scroll_style(ctx)),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn group_row<'a>(ctx: Ctx, name: &str, count: usize, open: bool, key: String) -> Element<'a, Msg> {
    let t = ctx.t;
    button(
        row![
            if open {
                icon(Icon::ChevronDown, 10.0, t.text_tertiary)
            } else {
                icon(Icon::ChevronRight, 10.0, t.text_tertiary)
            },
            text(name.to_string()).size(ctx.f(11.0)).font(ctx.ui_medium()).color(t.text_tertiary),
        ]
        .spacing(4)
        .align_y(Alignment::Center),
    )
    .padding(Padding { top: 5.0, right: 8.0, bottom: 2.0, left: 10.0 })
    .width(Length::Fill)
    .style(move |_: &Theme, status| button::Style {
        background: if matches!(status, button::Status::Hovered) { Some(t.hover.into()) } else { None },
        text_color: t.text,
        border: Border::default(),
        ..Default::default()
    })
    .on_press(Msg::ToggleNode(key))
    .into()
}

fn conn_row<'a>(
    ctx: Ctx,
    conn: &ConnectionConfig,
    open: bool,
    connected: bool,
    selected: bool,
) -> Element<'a, Msg> {
    let t = ctx.t;
    let accent = crate::view::kind_color(conn.kind);
    // 与原型一致：关系型只显示名称，其它类型「名称 · 类型」
    let label = if conn.kind.is_relational() {
        conn.name.clone()
    } else {
        format!("{} · {}", conn.name, conn.kind.short())
    };
    let label_msg = if conn.kind.is_relational() {
        Msg::ToggleNode(format!("conn:{}", conn.id))
    } else {
        match conn.kind {
            DbKind::Redis => Msg::OpenRedis(conn.id),
            DbKind::Mongodb => Msg::OpenMongo(conn.id),
            _ => Msg::OpenEs(conn.id),
        }
    };
    let name_font = if selected || open { ctx.ui_medium() } else { ctx.ui };
    let bg = if selected { t.selected } else { Color::TRANSPARENT };

    let main = button(
        row![
            icon(if open { Icon::ChevronDown } else { Icon::ChevronRight }, 10.0, t.text_tertiary),
            icon(Icon::Database, 13.0, accent),
            text(label).size(ctx.f(ts::SMALL)).font(name_font).color(t.text),
            kit::hspace(),
        ]
        .spacing(5)
        .align_y(Alignment::Center)
        .padding(Padding { top: 0.0, right: 2.0, bottom: 0.0, left: 8.0 }),
    )
    .padding(0)
    .height(Length::Fixed(28.0))
    .width(Length::Fill)
    .style(move |_: &Theme, status| button::Style {
        background: Some(
            if selected {
                bg
            } else if matches!(status, button::Status::Hovered) {
                t.hover
            } else {
                Color::TRANSPARENT
            }
            .into(),
        ),
        text_color: t.text,
        border: Border::default(),
        ..Default::default()
    })
    .on_press(label_msg);

    // 已连接指示点即「服务器监控」入口（视觉与原型一致，不增加额外行）
    let dot_slot: Element<'a, Msg> = if connected {
        let dot = container(Space::new().width(Length::Fixed(5.0)).height(Length::Fixed(5.0)))
            .style(move |_: &Theme| container::Style {
                background: Some(t.success.into()),
                border: Border::default().rounded(4.0),
                ..Default::default()
            });
        if conn.kind.is_relational() {
            button(dot)
                .padding(Padding { top: 6.0, right: 10.0, bottom: 6.0, left: 2.0 })
                .style(move |_: &Theme, status| button::Style {
                    background: if matches!(status, button::Status::Hovered) {
                        Some(t.hover.into())
                    } else {
                        None
                    },
                    text_color: t.text,
                    border: Border::default(),
                    ..Default::default()
                })
                .on_press(Msg::OpenMonitor(conn.id))
                .into()
        } else {
            container(dot)
                .padding(Padding { top: 6.0, right: 12.0, bottom: 6.0, left: 2.0 })
                .into()
        }
    } else {
        Space::new().width(Length::Fixed(12.0)).height(Length::Fixed(1.0)).into()
    };

    row![main, dot_slot]
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .into()
}

fn db_row<'a>(ctx: Ctx, conn_id: ConnId, name: &str, open: bool, key: String) -> Element<'a, Msg> {
    let t = ctx.t;
    let chevron = button(
        icon(if open { Icon::ChevronDown } else { Icon::ChevronRight }, 10.0, t.text_tertiary),
    )
    .padding(2)
    .style(move |_: &Theme, _status| button::Style {
        background: None,
        text_color: t.text,
        border: Border::default(),
        ..Default::default()
    })
    .on_press(Msg::ToggleNode(key));

    let label = button(
        row![
            icon(Icon::Database, 12.0, t.text_secondary),
            text(name.to_string()).size(ctx.f(ts::SMALL)).font(ctx.ui_medium()).color(t.text),
            kit::hspace(),
        ]
        .spacing(5)
        .align_y(Alignment::Center)
        .padding(Padding { top: 0.0, right: 10.0, bottom: 0.0, left: 2.0 }),
    )
    .padding(0)
    .height(Length::Fixed(26.0))
    .width(Length::Fill)
    .style(move |_: &Theme, status| button::Style {
        background: if matches!(status, button::Status::Hovered) { Some(t.hover.into()) } else { None },
        text_color: t.text,
        border: Border::default(),
        ..Default::default()
    })
    .on_press(Msg::LoadEr(conn_id));

    row![chevron, label]
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .into()
}

fn group_count_row<'a>(
    ctx: Ctx,
    label: &str,
    count: usize,
    open: bool,
    key: String,
    ic: Icon,
) -> Element<'a, Msg> {
    let t = ctx.t;
    button(
        row![
            icon(if open { Icon::ChevronDown } else { Icon::ChevronRight }, 10.0, t.text_tertiary),
            icon(ic, 12.0, t.text_secondary),
            text(label.to_string()).size(ctx.f(ts::SMALL)).font(ctx.ui_medium()).color(t.text),
            kit::hspace(),
            text(kit::count_label(count)).size(ctx.f(ts::MICRO)).font(ctx.ui).color(t.text_tertiary),
        ]
        .spacing(5)
        .align_y(Alignment::Center)
        .padding(Padding { top: 0.0, right: 10.0, bottom: 0.0, left: 34.0 }),
    )
    .padding(0)
    .height(Length::Fixed(26.0))
    .width(Length::Fill)
    .style(move |_: &Theme, status| button::Style {
        background: if matches!(status, button::Status::Hovered) { Some(t.hover.into()) } else { None },
        text_color: t.text,
        border: Border::default(),
        ..Default::default()
    })
    .on_press(Msg::ToggleNode(key))
    .into()
}

fn obj_row<'a>(
    ctx: Ctx,
    conn: ConnId,
    name: &str,
    ic: Icon,
    selected: bool,
    msg: Msg,
    _key: String,
) -> Element<'a, Msg> {
    let t = ctx.t;
    row![
        button(
            row![
                Space::new().width(Length::Fixed(14.0)).height(Length::Fixed(0.0)),
                icon(ic, 12.0, t.text_secondary),
                text(name.to_string()).size(ctx.f(ts::SMALL)).font(ctx.ui).color(t.text),
                kit::hspace(),
            ]
            .spacing(6)
            .align_y(Alignment::Center)
            .padding(Padding { top: 0.0, right: 10.0, bottom: 0.0, left: 42.0 }),
        )
        .padding(0)
        .height(Length::Fixed(26.0))
        .width(Length::Fill)
        .style(move |_: &Theme, status| button::Style {
            background: Some(
                if selected {
                    t.selected
                } else if matches!(status, button::Status::Hovered) {
                    t.hover
                } else {
                    Color::TRANSPARENT
                }
                .into(),
            ),
            text_color: t.text,
            border: Border::default(),
            ..Default::default()
        })
        .on_press(msg),
        // 对象设计器快捷入口
        kit::icon_button(ctx, Icon::Doc, 11.0, t.text_tertiary, Msg::DesignerOpen(conn, name.to_string())),
    ]
    .align_y(Alignment::Center)
    .width(Length::Fill)
    .into()
}

fn action_row<'a>(ctx: Ctx, label: &str, ic: Icon, msg: Msg) -> Element<'a, Msg> {
    let t = ctx.t;
    button(
        row![
            Space::new().width(Length::Fixed(14.0)).height(Length::Fixed(0.0)),
            icon(ic, 12.0, t.text_tertiary),
            text(label.to_string()).size(ctx.f(ts::SMALL)).font(ctx.ui).color(t.text_secondary),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .padding(Padding { top: 0.0, right: 10.0, bottom: 0.0, left: 42.0 }),
    )
    .padding(0)
    .height(Length::Fixed(24.0))
    .width(Length::Fill)
    .style(move |_: &Theme, status| button::Style {
        background: if matches!(status, button::Status::Hovered) { Some(t.hover.into()) } else { None },
        text_color: t.text,
        border: Border::default(),
        ..Default::default()
    })
    .on_press(msg)
    .into()
}

fn keyspace_tree<'a>(app: &'a App, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;
    let tr = ctx.tr;
    let mut list = column![].spacing(0);
    let info = app.redis.info.clone();
    let dbs = info.as_ref().map(|i| i.keyspace.clone()).unwrap_or_default();
    let total: u64 = dbs.iter().map(|d| d.keys).sum();
    list = list.push(
        row![
            icon(Icon::Database, 13.0, t.error),
            text(format!("cache - {}", info.map(|i| i.version).unwrap_or_else(|| "Redis".into())))
                .size(ctx.f(ts::SMALL))
                .font(ctx.ui_medium())
                .color(t.text),
        ]
        .spacing(6)
        .padding(Padding { top: 6.0, right: 10.0, bottom: 6.0, left: 12.0 }),
    );
    for row_db in dbs {
        let active = row_db.db == app.redis.db;
        list = list.push(
            button(
                row![
                    icon(Icon::Folder, 12.0, t.text_secondary),
                    text(format!("db{}", row_db.db)).size(ctx.f(ts::SMALL)).font(ctx.ui).color(t.text),
                    kit::hspace(),
                    text(kit::count_label(row_db.keys as usize))
                        .size(ctx.f(ts::MICRO))
                        .font(ctx.ui)
                        .color(t.text_tertiary),
                ]
                .spacing(6)
                .align_y(Alignment::Center)
                .padding(Padding { top: 0.0, right: 10.0, bottom: 0.0, left: 18.0 }),
            )
            .padding(0)
            .height(Length::Fixed(26.0))
            .width(Length::Fill)
            .style(move |_: &Theme, status| button::Style {
                background: Some(
                    if active {
                        t.selected
                    } else if matches!(status, button::Status::Hovered) {
                        t.hover
                    } else {
                        Color::TRANSPARENT
                    }
                    .into(),
                ),
                text_color: t.text,
                border: Border::default(),
                ..Default::default()
            })
            .on_press(Msg::RedisPickDb(row_db.db)),
        );
        if active {
            for key in &app.redis.keys {
                let sel = app.redis.selected.as_deref() == Some(key.as_str());
                list = list.push(
                    button(
                        row![
                            icon(Icon::Key, 11.0, t.warning),
                            text(key.clone()).size(ctx.f(ts::SMALL)).font(ctx.mono).color(t.text),
                        ]
                        .spacing(6)
                        .align_y(Alignment::Center)
                        .padding(Padding { top: 0.0, right: 10.0, bottom: 0.0, left: 32.0 }),
                    )
                    .padding(0)
                    .height(Length::Fixed(25.0))
                    .width(Length::Fill)
                    .style(move |_: &Theme, status| button::Style {
                        background: Some(
                            if sel {
                                t.selected
                            } else if matches!(status, button::Status::Hovered) {
                                t.hover
                            } else {
                                Color::TRANSPARENT
                            }
                            .into(),
                        ),
                        text_color: t.text,
                        border: Border::default(),
                        ..Default::default()
                    })
                    .on_press(Msg::RedisPickKey(key.clone())),
                );
            }
        }
    }
    let _ = total;

    let search = kit::search_field(ctx, tr.search_keys(), &app.redis.pattern, Msg::RedisScan);

    column![
        sidebar_header(ctx, tr.keyspace(), None, Some(Msg::ConnMenu(-1))),
        container(search).padding(Padding { top: 0.0, right: 10.0, bottom: 6.0, left: 10.0 }),
        scrollable(list).height(Length::Fill).style(scroll_style(ctx)),
    ]
    .height(Length::Fill)
    .into()
}

fn collections_tree<'a>(app: &'a App, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;
    let tr = ctx.tr;
    let mut list = column![].spacing(0);
    list = list.push(
        row![
            icon(Icon::Database, 13.0, t.success),
            text("analytics - MongoDB").size(ctx.f(ts::SMALL)).font(ctx.ui_medium()).color(t.text),
        ]
        .spacing(6)
        .padding(Padding { top: 6.0, right: 10.0, bottom: 6.0, left: 12.0 }),
    );
    list = list.push(
        row![
            icon(Icon::Folder, 12.0, t.text_secondary),
            text(app.mongo.db.clone().unwrap_or_else(|| "appdb".into()))
                .size(ctx.f(ts::SMALL))
                .font(ctx.ui)
                .color(t.text),
        ]
        .spacing(6)
        .padding(Padding { top: 4.0, right: 10.0, bottom: 4.0, left: 18.0 }),
    );
    for (name, count) in &app.mongo.collections {
        let sel = app.mongo.collection.as_deref() == Some(name.as_str());
        list = list.push(
            button(
                row![
                    icon(Icon::Doc, 11.0, t.text_secondary),
                    text(name.clone()).size(ctx.f(ts::SMALL)).font(ctx.ui).color(t.text),
                    kit::hspace(),
                    text(kit::count_label(*count as usize))
                        .size(ctx.f(ts::MICRO))
                        .font(ctx.ui)
                        .color(t.text_tertiary),
                ]
                .spacing(6)
                .align_y(Alignment::Center)
                .padding(Padding { top: 0.0, right: 10.0, bottom: 0.0, left: 32.0 }),
            )
            .padding(0)
            .height(Length::Fixed(26.0))
            .width(Length::Fill)
            .style(move |_: &Theme, status| button::Style {
                background: Some(
                    if sel {
                        t.selected
                    } else if matches!(status, button::Status::Hovered) {
                        t.hover
                    } else {
                        Color::TRANSPARENT
                    }
                    .into(),
                ),
                text_color: t.text,
                border: Border::default(),
                ..Default::default()
            })
            .on_press(Msg::MongoPickColl(name.clone())),
        );
    }

    column![
        sidebar_header(ctx, tr.collections(), None, Some(Msg::ConnMenu(-1))),
        scrollable(list).height(Length::Fill).style(scroll_style(ctx)),
    ]
    .height(Length::Fill)
    .into()
}

fn indices_tree<'a>(app: &'a App, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;
    let tr = ctx.tr;
    let mut list = column![].spacing(0);
    list = list.push(
        row![
            icon(Icon::Database, 13.0, t.warning),
            text("logs - Elasticsearch").size(ctx.f(ts::SMALL)).font(ctx.ui_medium()).color(t.text),
        ]
        .spacing(6)
        .padding(Padding { top: 6.0, right: 10.0, bottom: 6.0, left: 12.0 }),
    );
    for (name, health, docs, _store) in &app.es.indices {
        let sel = app.es.index.as_deref() == Some(name.as_str());
        list = list.push(
            button(
                row![
                    icon(Icon::Chart, 11.0, if health == "green" { t.success } else { t.warning }),
                    text(name.clone()).size(ctx.f(ts::SMALL)).font(ctx.ui).color(t.text),
                    kit::hspace(),
                    text(kit::count_label(*docs as usize))
                        .size(ctx.f(ts::MICRO))
                        .font(ctx.ui)
                        .color(t.text_tertiary),
                ]
                .spacing(6)
                .align_y(Alignment::Center)
                .padding(Padding { top: 0.0, right: 10.0, bottom: 0.0, left: 18.0 }),
            )
            .padding(0)
            .height(Length::Fixed(26.0))
            .width(Length::Fill)
            .style(move |_: &Theme, status| button::Style {
                background: Some(
                    if sel {
                        t.selected
                    } else if matches!(status, button::Status::Hovered) {
                        t.hover
                    } else {
                        Color::TRANSPARENT
                    }
                    .into(),
                ),
                text_color: t.text,
                border: Border::default(),
                ..Default::default()
            })
            .on_press(Msg::EsPickIndex(name.clone())),
        );
    }

    column![
        sidebar_header(ctx, tr.index(), None, Some(Msg::ConnMenu(-1))),
        scrollable(list).height(Length::Fill).style(scroll_style(ctx)),
    ]
    .height(Length::Fill)
    .into()
}

fn settings_nav<'a>(app: &'a App, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;
    let tr = ctx.tr;
    let items = [tr.display_config(), tr.normal_config(), tr.file_config()];
    let mut list = column![].spacing(2);
    for (i, label) in items.iter().enumerate() {
        let active = app.settings_ui.section == i;
        list = list.push(
            button(
                text(label.to_string())
                    .size(ctx.f(ts::SMALL))
                    .font(if active { ctx.ui_medium() } else { ctx.ui })
                    .color(if active { t.accent } else { t.text }),
            )
            .padding(Padding { top: 6.0, right: 10.0, bottom: 6.0, left: 10.0 })
            .width(Length::Fill)
            .style(move |_: &Theme, status| button::Style {
                background: Some(
                    if active {
                        t.accent_soft
                    } else if matches!(status, button::Status::Hovered) {
                        t.hover
                    } else {
                        Color::TRANSPARENT
                    }
                    .into(),
                ),
                text_color: t.text,
                border: Border::default().rounded(radius::XS),
                ..Default::default()
            })
            .on_press(Msg::SettingsSection(i)),
        );
    }
    column![
        sidebar_header(ctx, tr.settings(), None, None),
        container(list).padding(Padding { top: 0.0, right: 10.0, bottom: 0.0, left: 10.0 }),
        kit::hspace(),
        container(
            text("DBHelper v2.4.1")
                .size(ctx.f(11.0))
                .font(ctx.ui)
                .color(t.text_tertiary)
        )
        .padding(Padding { top: 8.0, right: 12.0, bottom: 10.0, left: 14.0 }),
    ]
    .height(Length::Fill)
    .into()
}

fn vault_footer<'a>(app: &'a App, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;
    let tr = ctx.tr;
    let label = if app.core.is_some() {
        tr.vault_encrypted().to_string()
    } else {
        tr.initializing().into()
    };
    let bar = container(
        row![
            icon(Icon::Shield, 12.0, t.text_secondary),
            text(label).size(ctx.f(11.0)).font(ctx.ui).color(t.text_secondary),
            kit::hspace(),
            kit::icon_button(
                ctx,
                if app.settings.theme == ThemeMode::Dark { Icon::Moon } else { Icon::Sun },
                12.0,
                t.text_secondary,
                Msg::SetTheme(if app.settings.theme == ThemeMode::Dark {
                    ThemeMode::Light
                } else {
                    ThemeMode::Dark
                })
            ),
            kit::icon_button(ctx, Icon::Gear, 12.0, t.text_secondary, Msg::OpenSettings),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .padding(Padding { top: 6.0, right: 8.0, bottom: 6.0, left: 12.0 }),
    )
    .width(Length::Fill)
    .style(move |_: &Theme| container::Style {
        border: Border { color: t.hairline, width: 1.0, radius: 0.0.into() },
        ..Default::default()
    });
    mouse_area(bar).on_press(Msg::OpenSettings).into()
}

pub fn statusbar<'a>(app: &'a App, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;
    let tr = ctx.tr;
    let memory = app.metrics.rss_mb.max(1.0);
    let conns = app.conns.len();
    // 与原型一致：左侧运行指标，右侧 Vault 状态
    let left = format!(
        "{} {:.0} MB · {} {} · {}",
        tr.memory(),
        memory,
        conns,
        tr.connections_count(),
        tr.encrypted()
    );
    let right = app
        .core
        .as_ref()
        .map(|c| format!("{} · {} {}", c.vault_status(), conns, tr.connections_count()))
        .unwrap_or_else(|| tr.initializing().into());

    container(
        row![
            text(left).size(ctx.f(ts::MICRO)).font(ctx.ui).color(t.text_secondary),
            kit::hspace(),
            container(Space::new().width(Length::Fixed(6.0)).height(Length::Fixed(6.0))).style(move |_: &Theme| container::Style {
                background: Some(t.success.into()),
                border: Border::default().rounded(3.0),
                ..Default::default()
            }),
            text(right).size(ctx.f(ts::MICRO)).font(ctx.ui).color(t.text_secondary),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .padding(Padding { top: 0.0, right: 12.0, bottom: 0.0, left: 12.0 }),
    )
    .height(Length::Fixed(24.0))
    .align_y(Alignment::Center)
    .width(Length::Fill)
    .style(move |_: &Theme| container::Style {
        background: Some(t.surface.into()),
        border: Border { color: t.hairline, width: 1.0, radius: 0.0.into() },
        ..Default::default()
    })
    .into()
}

pub fn scroll_style(ctx: Ctx) -> impl Fn(&Theme, scrollable::Status) -> scrollable::Style + 'static {
    move |_: &Theme, status| {
        let t = ctx.t;
        let _ = status;
        let rail = scrollable::Rail {
            background: None,
            border: Border::default(),
            scroller: scrollable::Scroller {
                background: Color { a: 0.35, ..t.text_tertiary }.into(),
                border: Border::default().rounded(4.0),
            },
        };
        scrollable::Style {
            container: container::Style::default(),
            vertical_rail: rail,
            horizontal_rail: rail,
            gap: None,
            auto_scroll: scrollable::AutoScroll {
                background: t.bg.into(),
                border: Border::default().rounded(radius::MD),
                shadow: iced::Shadow::default(),
                icon: t.text_secondary,
            },
        }
    }
}

pub fn toasts<'a>(app: &'a App, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;
    let mut col = column![].spacing(6).align_x(Alignment::End);
    for (i, toast) in app.toasts.iter().enumerate() {
        let tone = toast.tone;
        col = col.push(
            container(
                row![
                    icon(
                        match tone {
                            Tone::Success => Icon::Check,
                            Tone::Error => Icon::Close,
                            Tone::Warning => Icon::Clock,
                            _ => Icon::Terminal,
                        },
                        12.0,
                        t.tone(tone)
                    ),
                    text(toast.text.clone())
                        .size(ctx.f(ts::SMALL))
                        .font(ctx.ui)
                        .color(t.text),
                    kit::icon_button(ctx, Icon::Close, 10.0, t.text_tertiary, Msg::ToastClose(i)),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .padding(Padding { top: 8.0, right: 10.0, bottom: 8.0, left: 12.0 })
            .style(move |_: &Theme| container::Style {
                background: Some(t.bg.into()),
                border: Border { color: t.hairline_strong, width: 1.0, radius: radius::MD.into() },
                shadow: iced::Shadow {
                    color: Color { a: 0.08, ..Color::BLACK },
                    offset: iced::Vector::new(0.0, 4.0),
                    blur_radius: 12.0,
                },
                ..Default::default()
            }),
        );
    }
    container(col)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::End)
        .align_y(Alignment::End)
        .padding(16)
        .into()
}

pub fn _stack<'a>(a: Element<'a, Msg>, b: Element<'a, Msg>) -> Element<'a, Msg> {
    stack![a, b].into()
}
