//! ER 设计器 + 服务器监控。

use crate::icons::{icon, Icon};
use crate::kit::{self, Ctx};
use crate::state::ErNode;
use crate::theme::{radius, size as ts, space, Tone};
use crate::widgets::chart::LineChart;
use crate::{App, Msg};
use iced::widget::canvas::{self, Canvas, Frame, Geometry, Path, Stroke, Text};
use iced::widget::{column, container, row, scrollable, Space};
use iced::{
    mouse, Alignment, Border, Color, Element, Length, Padding, Point, Rectangle, Renderer, Theme,
};

// ---------------------------------------------------------------------------
// ER 图
// ---------------------------------------------------------------------------

pub fn view<'a>(app: &'a App, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;
    let toolbar = container(
        row![
            kit::segmented(
                ctx,
                &[("ER 关系图", app.er.mode == 0), ("表结构", app.er.mode == 1)],
                |i| Msg::ErMode(i)
            ),
            Space::new().width(Length::Fixed(8.0)).height(Length::Fixed(0.0)),
            kit::secondary(ctx, "+ 添加表", Msg::ErAddTable),
            kit::ghost(ctx, "自动布局", Msg::ErAutoLayout),
            kit::ghost(ctx, "导出 DDL", Msg::ErExportDdl),
            kit::hspace(),
            kit::ghost(ctx, "缩放 −", Msg::ErZoom(-0.1)),
            kit::txt(ctx, format!("{:.0}%", app.er.zoom * 100.0), ts::MICRO, t.text_secondary),
            kit::ghost(ctx, "缩放 +", Msg::ErZoom(0.1)),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .padding(Padding { top: 6.0, right: 12.0, bottom: 6.0, left: 12.0 }),
    )
    .width(Length::Fill)
    .style(move |_: &Theme| container::Style {
        border: Border { color: t.hairline, width: 1.0, radius: 0.0.into() },
        ..Default::default()
    });

    if app.er.mode == 1 {
        return column![toolbar, table_structure(app, ctx)].height(Length::Fill).into();
    }

    if let Some(err) = &app.er.error {
        if !err.is_empty() {
            return column![
                toolbar,
                kit::empty_state(
                    ctx,
                    Icon::Chart,
                    "尚未载入当前连接的表结构",
                    "点击「自动布局」或从左侧选择连接以载入",
                    Some(kit::secondary(ctx, "载入 ER 图", Msg::ErAutoLayout))
                )
            ]
            .height(Length::Fill)
            .into();
        }
    }

    let body = Canvas::new(ErCanvas {
        nodes: &app.er.nodes,
        selected: app.er.selected,
        zoom: app.er.zoom,
        ctx,
    })
    .width(Length::Fill)
    .height(Length::Fill);

    let legend = container(
        kit::txt(
            ctx,
            "PK 主键 · FK 外键 · 虚线为待确认关系",
            ts::MICRO,
            t.text_tertiary,
        ),
    )
    .padding(Padding { top: 5.0, right: 8.0, bottom: 5.0, left: 8.0 })
    .style(move |_: &Theme| container::Style {
        background: Some(t.bg.into()),
        border: Border { color: t.hairline, width: 1.0, radius: radius::XS.into() },
        ..Default::default()
    });

    column![
        toolbar,
        iced::widget::stack![container(body).padding(0), container(legend)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Start)
            .align_y(Alignment::End)
            .padding(14)],
    ]
    .height(Length::Fill)
    .into()
}

struct ErCanvas<'a> {
    nodes: &'a [ErNode],
    selected: Option<usize>,
    zoom: f32,
    ctx: Ctx,
}

#[derive(Default)]
struct ErDrag {
    last: Option<Point>,
    index: Option<usize>,
}

impl<'a> canvas::Program<Msg> for ErCanvas<'a> {
    type State = ErDrag;

    fn update(
        &self,
        state: &mut ErDrag,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Msg>> {
        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let pos = cursor.position_in(bounds)?;
                let hit = self.nodes.iter().position(|n| {
                    let x = n.x * self.zoom;
                    let y = n.y * self.zoom;
                    pos.x >= x && pos.x <= x + n.w * self.zoom && pos.y >= y && pos.y <= y + n.h * self.zoom
                });
                state.last = Some(pos);
                state.index = hit;
                match hit {
                    Some(i) => Some(canvas::Action::publish(Msg::ErDragStart(i, Point::ORIGIN))),
                    None => None,
                }
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let pos = cursor.position_in(bounds)?;
                let last = state.last?;
                let delta = Point::new((pos.x - last.x) / self.zoom, (pos.y - last.y) / self.zoom);
                state.last = Some(pos);
                if let Some(i) = state.index {
                    return Some(canvas::Action::publish(Msg::ErDrag(i, delta)));
                }
                None
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.index.take().is_some() {
                    state.last = None;
                    return Some(canvas::Action::publish(Msg::ErDragEnd));
                }
                None
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &ErDrag,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let ctx = self.ctx;
        let t = ctx.t;
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), t.bg);

        // 网格点阵
        let step = 24.0;
        let mut x = 0.0;
        while x < bounds.width {
            let mut y = 0.0;
            while y < bounds.height {
                frame.fill_rectangle(Point::new(x, y), iced::Size::new(1.0, 1.0), crate::theme::rgba(0x37352F, 0.05));
                y += step;
            }
            x += step;
        }

        // 连线
        for node in self.nodes {
            for (col, fk) in node.table.foreign_keys() {
                if let Some(target) = self.nodes.iter().find(|n| n.table.name == fk.ref_table) {
                    let from = Point::new(
                        (node.x + node.w) * self.zoom,
                        (node.y + 34.0 + col.ordinal as f32 * 22.0 + 11.0) * self.zoom,
                    );
                    let to_y = (target.y + 34.0 + 22.0 * self.zoom) * self.zoom;
                    let to = Point::new(target.x * self.zoom, to_y.max(target.y * self.zoom + 20.0));
                    let path = Path::new(|b| {
                        b.move_to(from);
                        let mid = (from.x + to.x) / 2.0;
                        b.bezier_curve_to(
                            Point::new(mid, from.y),
                            Point::new(mid, to.y),
                            to,
                        );
                    });
                    frame.stroke(
                        &path,
                        Stroke::default().with_color(t.hairline_strong).with_width(1.0),
                    );
                    let head = Path::new(|b| {
                        b.move_to(Point::new(to.x - 5.0, to.y - 3.0));
                        b.line_to(to);
                        b.line_to(Point::new(to.x - 5.0, to.y + 3.0));
                    });
                    frame.stroke(&head, Stroke::default().with_color(t.text_secondary).with_width(1.0));
                }
            }
        }

        // 节点
        for (i, node) in self.nodes.iter().enumerate() {
            let x = node.x * self.zoom;
            let y = node.y * self.zoom;
            let w = node.w * self.zoom;
            let h = node.h * self.zoom;
            let selected = self.selected == Some(i);
            let rect = Path::rounded_rectangle(
                Point::new(x, y),
                iced::Size::new(w, h),
                6.0.into(),
            );
            frame.fill(&rect, t.bg);
            frame.stroke(
                &rect,
                Stroke::default()
                    .with_color(if selected { t.accent } else { t.hairline_strong })
                    .with_width(if selected { 2.0 } else { 1.0 }),
            );

            frame.fill_text(Text {
                content: node.table.name.clone(),
                position: Point::new(x + 30.0, y + 9.0),
                color: t.text,
                size: iced::Pixels(ctx.f(12.0)),
                font: ctx.ui_medium(),
                ..Default::default()
            });
            frame.fill_text(Text {
                content: format!("{} 列", node.table.columns.len()),
                position: Point::new(x + w - 12.0, y + 10.0),
                color: t.text_tertiary,
                size: iced::Pixels(ctx.f(11.0)),
                font: ctx.ui,
                align_x: Alignment::End.into(),
                ..Default::default()
            });
            frame.fill_rectangle(
                Point::new(x, y + 30.0 * self.zoom),
                iced::Size::new(w, 1.0),
                t.hairline,
            );

            for (ci, col) in node.table.columns.iter().enumerate().take(8) {
                let cy = y + 34.0 + ci as f32 * 22.0 * self.zoom;
                let tag = if col.is_pk {
                    "PK"
                } else if col.fk.is_some() {
                    "FK"
                } else {
                    ""
                };
                if !tag.is_empty() {
                    frame.fill_text(Text {
                        content: tag.to_string(),
                        position: Point::new(x + 10.0, cy + 2.0),
                        color: if col.is_pk { t.warning } else { t.accent },
                        size: iced::Pixels(ctx.f(9.0)),
                        font: ctx.ui_bold(),
                        ..Default::default()
                    });
                }
                frame.fill_text(Text {
                    content: col.name.clone(),
                    position: Point::new(x + 34.0, cy + 1.0),
                    color: t.text,
                    size: iced::Pixels(ctx.f(11.0)),
                    font: ctx.mono,
                    ..Default::default()
                });
                frame.fill_text(Text {
                    content: col.data_type.clone(),
                    position: Point::new(x + w - 10.0, cy + 1.0),
                    color: t.text_tertiary,
                    size: iced::Pixels(ctx.f(10.0)),
                    font: ctx.mono,
                    align_x: Alignment::End.into(),
                    ..Default::default()
                });
            }
        }

        vec![frame.into_geometry()]
    }
}

fn table_structure<'a>(app: &'a App, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;
    let Some(node) = app.er.selected.and_then(|i| app.er.nodes.get(i)) else {
        return kit::empty_state(
            ctx,
            Icon::Table,
            "选择一张表",
            "在 ER 图中点击表卡片查看结构",
            None,
        );
    };
    let mut rows = column![].spacing(0);
    for col in &node.table.columns {
        rows = rows.push(
            container(
                row![
                    kit::txt(ctx, col.name.clone(), ts::SMALL, t.text),
                    kit::hspace(),
                    kit::txt(ctx, col.data_type.clone(), ts::SMALL, t.text_secondary),
                    Space::new().width(Length::Fixed(12.0)).height(Length::Fixed(0.0)),
                    if col.is_pk {
                        kit::badge(ctx, "PK", Tone::Warning)
                    } else if col.fk.is_some() {
                        kit::badge(ctx, "FK", Tone::Accent)
                    } else {
                        kit::badge(ctx, if col.nullable { "NULL" } else { "NOT NULL" }, Tone::Neutral)
                    },
                ]
                .align_y(Alignment::Center),
            )
            .padding(Padding { top: 7.0, right: 14.0, bottom: 7.0, left: 14.0 })
            .style(move |_: &Theme| container::Style {
                border: Border { color: t.hairline, width: 0.0, radius: 0.0.into() },
                ..Default::default()
            }),
        );
    }
    column![
        container(kit::txt_bold(ctx, node.table.name.clone(), ts::H2, t.text))
            .padding(Padding { top: 14.0, right: 14.0, bottom: 8.0, left: 14.0 }),
        rows,
    ]
    .into()
}

// ---------------------------------------------------------------------------
// 监控
// ---------------------------------------------------------------------------

pub fn monitor_view<'a>(app: &'a App, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;
    let m = &app.metrics;
    let header = container(
        row![
            kit::txt(ctx, "时间范围", ts::MICRO, t.text_secondary),
            kit::segmented(
                ctx,
                &[("5 分钟", app.monitor.range == 0), ("1 小时", app.monitor.range == 1), ("24 小时", app.monitor.range == 2)],
                |i| Msg::MonitorRange(i)
            ),
            Space::new().width(Length::Fixed(8.0)).height(Length::Fixed(0.0)),
            kit::txt(
                ctx,
                format!("自动刷新 5s · 上次更新 {}", dbhelper_utils::textfmt::clock(app.monitor.last_update)),
                ts::MICRO,
                t.text_tertiary
            ),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .padding(Padding { top: 7.0, right: 12.0, bottom: 7.0, left: 12.0 }),
    )
    .width(Length::Fill);

    let qps_now = m.qps.back().copied().unwrap_or(0.0);
    let avg = if m.qps.is_empty() {
        0.0
    } else {
        m.qps.iter().sum::<f32>() / m.qps.len() as f32
    };
    let cards = row![
        metric_card(
            ctx,
            "QPS",
            format!("{:.0}", qps_now.max(0.0)),
            format!("均值 {:.0} · 峰值 {:.0}", avg, m.peak_qps),
            Tone::Neutral,
        ),
        metric_card(
            ctx,
            "活跃连接",
            format!("{} / {}", m.connections, 200),
            format!("{:.0}% 连接池使用率", (m.connections as f32 / 200.0 * 100.0).min(100.0)),
            Tone::Neutral,
        ),
        metric_card(
            ctx,
            "慢查询（5 分钟内）",
            format!("{}", m.recent.iter().filter(|q| q.elapsed_secs > 0.5).count()),
            if m.recent.iter().any(|q| q.elapsed_secs > 0.5) {
                "超过阈值 0.5s，建议查看".into()
            } else {
                "全部低于阈值".into()
            },
            if m.recent.iter().any(|q| q.elapsed_secs > 0.5) { Tone::Warning } else { Tone::Neutral },
        ),
    ]
    .spacing(12);

    let qps: Vec<f32> = m.qps.iter().copied().collect();
    let conns: Vec<f32> = m.conns.iter().copied().collect();
    let charts = row![
        chart_card(
            ctx,
            &format!("QPS · 峰值 {:.0} · 均值 {:.0}", m.peak_qps, avg),
            LineChart::new(&qps, t.accent, 150.0, Some(m.peak_qps), ctx).view()
        ),
        chart_card(
            ctx,
            &format!("连接数 · 当前 {} / 上限 200", m.connections),
            LineChart::new(&conns, t.success, 150.0, None, ctx).view()
        ),
    ]
    .spacing(12);

    let sessions = sessions_panel(app, ctx);
    let slow = slow_panel(app, ctx);

    column![
        header,
        container(
            column![
                cards,
                Space::new().width(Length::Fixed(0.0)).height(Length::Fixed(10.0)),
                charts,
                Space::new().width(Length::Fixed(0.0)).height(Length::Fixed(10.0)),
                row![sessions, slow].spacing(12).height(Length::Fixed(260.0)),
            ]
            .spacing(0)
        )
        .padding(Padding { top: 10.0, right: 12.0, bottom: 12.0, left: 12.0 })
        .height(Length::Fill),
    ]
    .height(Length::Fill)
    .into()
}

fn metric_card<'a>(ctx: Ctx, label: &str, value: String, sub: String, tone: Tone) -> Element<'a, Msg> {
    let t = ctx.t;
    let accent = tone != Tone::Neutral;
    container(
        column![
            kit::txt(ctx, label.to_string(), ts::MICRO, t.text_secondary),
            kit::txt_bold(ctx, value, ts::PAGE, t.text),
            kit::txt(ctx, sub, ts::MICRO, if accent { t.tone(tone) } else { t.text_tertiary }),
        ]
        .spacing(3)
        .width(Length::Fill),
    )
    .padding(Padding { top: 10.0, right: 14.0, bottom: 10.0, left: 14.0 })
    .width(Length::FillPortion(1))
    .style(move |_: &Theme| container::Style {
        background: Some(if accent { t.tone_soft(tone) } else { t.bg }.into()),
        border: Border {
            color: if accent { t.tone(tone) } else { t.hairline },
            width: 1.0,
            radius: radius::MD.into(),
        },
        ..Default::default()
    })
    .into()
}

fn chart_card<'a>(ctx: Ctx, title: &str, chart: Element<'a, Msg>) -> Element<'a, Msg> {
    let t = ctx.t;
    container(
        column![
            kit::txt(ctx, title.to_string(), ts::MICRO, t.text_secondary),
            Space::new().width(Length::Fixed(0.0)).height(Length::Fixed(6.0)),
            chart,
        ]
        .spacing(0)
        .width(Length::Fill),
    )
    .padding(Padding { top: 10.0, right: 12.0, bottom: 8.0, left: 12.0 })
    .width(Length::FillPortion(1))
    .style(move |_: &Theme| container::Style {
        background: Some(t.bg.into()),
        border: Border { color: t.hairline, width: 1.0, radius: radius::MD.into() },
        ..Default::default()
    })
    .into()
}

fn sessions_panel<'a>(app: &'a App, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;
    let mut rows = column![].spacing(0);
    if app.monitor.sessions.is_empty() {
        rows = rows.push(
            container(kit::txt(
                ctx,
                "当前驱动未提供会话列表。SQLite 为嵌入式引擎，无并发会话；连接 MySQL / PostgreSQL 后可查看活动会话。",
                ts::MICRO,
                t.text_tertiary,
            ))
            .padding(14),
        );
    }
    for s in app.monitor.sessions.iter().take(6) {
        let state_tone = if s.state.contains("active") || s.state.contains("执行") {
            Tone::Accent
        } else if s.state.contains("wait") || s.state.contains("锁") {
            Tone::Warning
        } else {
            Tone::Neutral
        };
        rows = rows.push(
            container(
                row![
                    kit::txt(ctx, s.id.clone(), ts::MICRO, t.text_secondary),
                    Space::new().width(Length::Fixed(8.0)).height(Length::Fixed(0.0)),
                    kit::badge(ctx, s.state.clone(), state_tone),
                    Space::new().width(Length::Fixed(8.0)).height(Length::Fixed(0.0)),
                    kit::mono(ctx, s.sql.chars().take(60).collect::<String>(), ts::MICRO, t.text),
                    kit::hspace(),
                    kit::danger_link(ctx, "终止", Msg::MonitorKill(s.id.clone())),
                ]
                .spacing(4)
                .align_y(Alignment::Center),
            )
            .padding(Padding { top: 5.0, right: 12.0, bottom: 5.0, left: 12.0 }),
        );
    }
    container(
        column![
            container(
                row![
                    kit::txt_bold(ctx, format!("活动会话 · {}", app.monitor.sessions.len()), ts::SMALL, t.text),
                    kit::hspace(),
                    kit::danger_link(ctx, "终止会话", Msg::Noop),
                ]
                .align_y(Alignment::Center)
                .padding(Padding { top: 8.0, right: 12.0, bottom: 8.0, left: 12.0 }),
            ),
            kit::hline(ctx),
            scrollable(rows).height(Length::Fill).style(crate::view::shell::scroll_style(ctx)),
        ]
        .height(Length::Fill),
    )
    .width(Length::FillPortion(3))
    .height(Length::Fill)
    .style(move |_: &Theme| container::Style {
        background: Some(t.bg.into()),
        border: Border { color: t.hairline, width: 1.0, radius: radius::MD.into() },
        ..Default::default()
    })
    .into()
}

fn slow_panel<'a>(app: &'a App, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;
    let mut rows = column![].spacing(0);
    let slow: Vec<_> = app.metrics.recent.iter().take(4).collect();
    if slow.is_empty() {
        rows = rows.push(container(kit::txt(ctx, "暂无慢查询记录", ts::MICRO, t.text_tertiary)).padding(14));
    }
    for q in slow {
        rows = rows.push(
            container(
                column![
                    row![
                        kit::txt(ctx, format!("{:.3} s", q.elapsed_secs), ts::MICRO, t.text_secondary),
                        Space::new().width(Length::Fixed(6.0)).height(Length::Fixed(0.0)),
                        kit::txt(ctx, format!("· {}", q.session_id), ts::MICRO, t.text_tertiary),
                    ],
                    kit::mono(ctx, q.sql.chars().take(64).collect::<String>(), ts::MICRO, t.text),
                ]
                .spacing(2),
            )
            .padding(Padding { top: 5.0, right: 12.0, bottom: 5.0, left: 12.0 }),
        );
    }
    container(
        column![
            container(
                row![
                    kit::txt_bold(ctx, "慢查询 Top 4", ts::SMALL, t.text),
                    kit::hspace(),
                    kit::link(ctx, "查看日志", Msg::Noop),
                ]
                .align_y(Alignment::Center)
                .padding(Padding { top: 8.0, right: 12.0, bottom: 8.0, left: 12.0 }),
            ),
            kit::hline(ctx),
            scrollable(rows).height(Length::Fill).style(crate::view::shell::scroll_style(ctx)),
        ]
        .height(Length::Fill),
    )
    .width(Length::FillPortion(2))
    .height(Length::Fill)
    .style(move |_: &Theme| container::Style {
        background: Some(t.bg.into()),
        border: Border { color: t.hairline, width: 1.0, radius: radius::MD.into() },
        ..Default::default()
    })
    .into()
}

pub fn _icon() -> Element<'static, Msg> {
    icon(Icon::Chart, 12.0, Color::BLACK)
}
