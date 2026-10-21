//! 界面装配：外壳 + 当前页 + 弹层。

pub mod data;
pub mod dialogs;
pub mod er;
pub mod nosql;
pub mod query;
pub mod settings;
pub mod shell;

use crate::kit::{self, Ctx};
use crate::state::{Modal, Page, TabKind};
use crate::theme::{radius, size as ts, space};
use crate::{App, Msg};
use dbhelper_models::DbKind;
use iced::widget::{column, container, row, stack, Space};
use iced::{Border, Color, Element, Length, Padding, Theme};

pub fn view(app: &App) -> Element<'_, Msg> {
    let t0 = std::time::Instant::now();
    let el = view_inner(app);
    crate::perf::record_view(t0.elapsed());
    el
}

fn view_inner(app: &App) -> Element<Msg> {
    let ctx = app.ctx();
    let t = ctx.t;

    if let Some(err) = &app.boot_error {
        return container(
            column![
                kit::txt_bold(ctx, "DBHelper 启动失败", ts::H1, t.error),
                kit::txt(ctx, err.clone(), ts::BODY, t.text_secondary),
            ]
            .spacing(8),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(move |_: &Theme| container::Style {
            background: Some(t.bg.into()),
            ..Default::default()
        })
        .into();
    }

    let main: Element<Msg> = match app.page {
        Page::Connections => connections(app, ctx),
        Page::Query => query::view(app, ctx),
        Page::Data => data::view(app, ctx),
        Page::Er => er::view(app, ctx),
        Page::Redis | Page::Mongo | Page::Es => nosql::view(app, ctx),
        Page::Monitor => er::monitor_view(app, ctx),
        Page::Settings => settings::view(app, ctx),
    };

    let show_tabs = !matches!(app.page, Page::Connections);
    // 与原型一致：侧栏从面包屑下沿一直延伸到状态栏，页签只覆盖右侧主区域
    let mut right = column![];
    if show_tabs {
        right = right.push(shell::tabbar(app, ctx));
    }
    right = right.push(
        container(main)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_: &Theme| container::Style { background: Some(t.bg.into()), ..Default::default() }),
    );

    let layout = column![
        shell::titlebar(app, ctx),
        shell::breadcrumb(app, ctx),
        row![
            container(shell::sidebar(app, ctx))
                .height(Length::Fill)
                .style(move |_: &Theme| container::Style {
                    background: Some(t.surface.into()),
                    border: Border { color: t.hairline, width: 1.0, radius: 0.0.into() },
                    ..Default::default()
                }),
            container(right).width(Length::Fill).height(Length::Fill),
        ]
        .height(Length::Fill),
        shell::statusbar(app, ctx),
    ];

    let base = container(layout)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_: &Theme| container::Style {
            background: Some(t.bg.into()),
            ..Default::default()
        });

    let mut layers = stack![base];
    if !matches!(app.modal, Modal::None) {
        layers = layers.push(dialogs::modal(app, ctx));
    }
    if !app.toasts.is_empty() {
        layers = layers.push(shell::toasts(app, ctx));
    }
    layers.into()
}

// ---------------------------------------------------------------------------
// 连接管理
// ---------------------------------------------------------------------------

fn connections<'a>(app: &'a App, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;

    let center = column![
        kit::txt_bold(ctx, "连接数据库", 24.0, t.text),
        kit::txt(ctx, "选择一个数据库类型开始，或从最近的连接继续", ts::BODY, t.text_secondary),
        Space::new().width(Length::Fixed(0.0)).height(Length::Fixed(18.0)),
        wizard_card(app, ctx),
    ]
    .spacing(6)
    .align_x(iced::Alignment::Center)
    .width(Length::Fixed(460.0));

    let main = container(center)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .padding(Padding { top: 0.0, right: 24.0, bottom: 40.0, left: 24.0 });

    let right = recent_panel(app, ctx);

    row![
        main,
        container(right)
            .width(Length::Fixed(380.0))
            .height(Length::Fill)
            .style(move |_: &Theme| container::Style {
                border: Border { color: t.hairline, width: 1.0, radius: 0.0.into() },
                ..Default::default()
            })
    ]
    .height(Length::Fill)
    .into()
}

fn wizard_card<'a>(app: &'a App, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;
    let selected = app.wizard.as_ref().map(|w| w.kind).unwrap_or(DbKind::Mysql);

    let mut grid = column![].spacing(8);
    for chunk in DbKind::ALL.chunks(4) {
        let mut r = row![].spacing(8);
        for kind in chunk {
            r = r.push(kind_tile(ctx, *kind, *kind == selected));
        }
        grid = grid.push(r);
    }

    let body = column![
        kit::txt_bold(ctx, "新建连接", 15.0, t.text),
        Space::new().width(Length::Fixed(0.0)).height(Length::Fixed(3.0)),
        kit::txt(
            ctx,
            "第 1 步 / 共 3 步 · 选择数据库类型",
            ts::SMALL,
            t.text_secondary
        ),
        Space::new().width(Length::Fixed(0.0)).height(Length::Fixed(12.0)),
        grid,
        Space::new().width(Length::Fixed(0.0)).height(Length::Fixed(12.0)),
        kit::txt(
            ctx,
            "凭据经 DPAPI 加密后存入本地 Vault；支持 SSH 隧道与 SSL 证书。",
            ts::SMALL,
            t.text_tertiary
        ),
        Space::new().width(Length::Fixed(0.0)).height(Length::Fixed(14.0)),
        row![
            kit::link(ctx, "使用高级配置", Msg::WizardStart),
            kit::hspace(),
            kit::secondary(ctx, "取消", Msg::Noop),
            kit::primary(ctx, "下一步", Msg::WizardStart),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center),
    ];

    container(body)
        .padding(Padding { top: 16.0, right: 16.0, bottom: 14.0, left: 16.0 })
        .width(Length::Fill)
        .style(move |_: &Theme| container::Style {
            background: Some(t.bg.into()),
            border: Border { color: t.hairline, width: 1.0, radius: radius::MD.into() },
            shadow: iced::Shadow::default(),
            ..Default::default()
        })
        .into()
}

fn kind_tile<'a>(ctx: Ctx, kind: DbKind, active: bool) -> Element<'a, Msg> {
    let t = ctx.t;
    let accent = kind_color(kind);
    iced::widget::button(
        column![
            kit::icon(kind_icon(kind), 17.0, accent),
            kit::txt(
                ctx,
                kind.label(),
                ts::BODY,
                if active { t.accent } else { t.text }
            ),
        ]
        .spacing(5)
        .align_x(iced::Alignment::Center),
    )
    .width(Length::Fixed(101.0))
    .height(Length::Fixed(54.0))
    .padding(Padding { top: 8.0, right: 4.0, bottom: 7.0, left: 4.0 })
    .style(move |_: &Theme, status| iced::widget::button::Style {
        background: Some(
            if active {
                t.accent_soft
            } else if matches!(status, iced::widget::button::Status::Hovered) {
                t.hover
            } else {
                t.bg
            }
            .into(),
        ),
        text_color: t.text,
        border: Border {
            color: if active { t.accent } else { t.hairline },
            width: if active { 1.5 } else { 1.0 },
            radius: radius::MD.into(),
        },
        shadow: iced::Shadow::default(),
        snap: true,
    })
    .on_press(Msg::WizardKind(kind))
    .into()
}

/// 数据库类型图标色（与原型一致：关系库蓝 / Oracle·SQLServer·Redis 红 / Mongo 绿 / Elastic 琥珀）。
pub fn kind_color(kind: DbKind) -> iced::Color {
    match kind {
        DbKind::Mysql | DbKind::Postgres | DbKind::Sqlite => crate::theme::rgb(0x3A7FC9),
        DbKind::Oracle | DbKind::SqlServer | DbKind::Redis => crate::theme::rgb(0xB2403A),
        DbKind::Mongodb => crate::theme::rgb(0x3B9A67),
        DbKind::Elasticsearch => crate::theme::rgb(0xE2A93B),
    }
}

pub fn kind_icon(kind: DbKind) -> crate::icons::Icon {
    use crate::icons::Icon;
    match kind {
        DbKind::Mysql | DbKind::Postgres | DbKind::Oracle | DbKind::SqlServer | DbKind::Sqlite => Icon::Database,
        DbKind::Redis => Icon::Key,
        DbKind::Mongodb => Icon::Doc,
        DbKind::Elasticsearch => Icon::Chart,
    }
}

fn recent_panel<'a>(app: &'a App, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;
    let mut recent = column![].spacing(6);
    for conn in &app.conn_ui.recent {
        recent = recent.push(
            iced::widget::button(
                row![
                    kit::icon(kind_icon(conn.kind), 18.0, kind_color(conn.kind)),
                    column![
                        kit::txt_bold(ctx, conn.name.clone(), ts::BODY, t.text),
                        kit::txt(
                            ctx,
                            format!("{} · {}", conn.kind.label(), conn.endpoint_short()),
                            ts::SMALL,
                            t.text_tertiary
                        ),
                    ]
                    .spacing(2)
                    .width(Length::Fill),
                ]
                .spacing(10)
                .align_y(iced::Alignment::Center)
                .padding(Padding { top: 8.0, right: 8.0, bottom: 8.0, left: 8.0 }),
            )
            .padding(0)
            .width(Length::Fill)
            .style(move |_: &Theme, status| iced::widget::button::Style {
                background: if matches!(status, iced::widget::button::Status::Hovered) {
                    Some(t.hover.into())
                } else {
                    None
                },
                text_color: t.text,
                border: Border::default().rounded(radius::XS),
                ..Default::default()
            })
            .on_press(Msg::OpenConnection(conn.id)),
        );
    }

    let mut groups = column![].spacing(2);
    let mut counts: Vec<(String, usize)> = Vec::new();
    for conn in &app.conns {
        let g = if conn.group.is_empty() { "未分组".to_string() } else { conn.group.clone() };
        match counts.iter_mut().find(|(n, _)| n == &g) {
            Some((_, c)) => *c += 1,
            None => counts.push((g, 1)),
        }
    }
    for (name, count) in counts {
        groups = groups.push(
            row![
                kit::icon(crate::icons::Icon::Folder, 12.0, t.text_secondary),
                kit::txt(ctx, name, ts::SMALL, t.text),
                kit::hspace(),
                kit::txt(ctx, count.to_string(), ts::MICRO, t.text_tertiary),
            ]
            .spacing(7)
            .align_y(iced::Alignment::Center)
            .padding(Padding { top: 5.0, right: 4.0, bottom: 5.0, left: 4.0 }),
        );
    }

    container(
        column![
            row![
                kit::txt(ctx, "最近连接", ts::SMALL, t.text_secondary),
                kit::hspace(),
                kit::link(ctx, "管理全部", Msg::GoConnections),
            ]
            .align_y(iced::Alignment::Center),
            Space::new().width(Length::Fixed(0.0)).height(Length::Fixed(6.0)),
            recent,
            Space::new().width(Length::Fixed(0.0)).height(Length::Fixed(12.0)),
            kit::txt(ctx, "分组", ts::SMALL, t.text_secondary),
            Space::new().width(Length::Fixed(0.0)).height(Length::Fixed(4.0)),
            groups,
        ]
        .spacing(0),
    )
    .padding(Padding { top: 12.0, right: 12.0, bottom: 12.0, left: 12.0 })
    .height(Length::Fill)
    .into()
}

pub fn _space() -> f32 {
    space::S
}
