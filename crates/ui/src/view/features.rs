//! 新功能页面：对象设计器 / BI 仪表板 / 迁移同步 / 服务器管理 / 导入向导。

use crate::icons::Icon;
use crate::kit::{self, Ctx};
use crate::state::{ChartCard, ChartKind, DesignerState, TransferMode};
use crate::theme::{radius, size as ts};
use crate::{App, Msg};
use dbhelper_models::ConnId;
use iced::widget::canvas::{self, Canvas, Frame, Geometry, Path, Stroke, Text};
use iced::widget::{column, container, pick_list, row, scrollable, Space};
use iced::{Alignment, Border, Color, Element, Length, Padding, Pixels, Point, Rectangle, Renderer, Theme};

// ===========================================================================
// 对象设计器
// ===========================================================================

pub fn designer_view<'a>(app: &'a App, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;
    let d = &app.designer;

    let header = container(
        row![
            kit::icon_button(ctx, Icon::Close, 13.0, t.text_secondary, Msg::DesignerClose),
            kit::txt_bold(
                ctx,
                if d.is_new {
                    format!("新建表 · {}", d.table)
                } else {
                    format!("设计表 · {}", d.table)
                },
                ts::H2,
                t.text
            ),
            kit::hspace(),
            kit::secondary(ctx, "新增列", Msg::DesignerAddCol),
            kit::primary(ctx, if d.saving { "保存中…" } else { "保存" }, Msg::DesignerSave),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .padding(Padding { top: 10.0, right: 16.0, bottom: 10.0, left: 16.0 }),
    )
    .width(Length::Fill);

    let tabs = container(kit::segmented(
        ctx,
        &[
            ("列设计", d.tab == 0),
            ("索引 DDL", d.tab == 1),
            ("触发器 DDL", d.tab == 2),
            ("DDL 预览", d.tab == 3),
        ],
        |i| Msg::DesignerTab(i),
    ))
    .padding(Padding { top: 8.0, right: 16.0, bottom: 8.0, left: 16.0 });

    let body: Element<Msg> = match d.tab {
        0 => designer_columns(d, ctx),
        1 => ddl_editor(ctx, &d.index_ddl, "index", "CREATE INDEX idx_name ON table (col);"),
        2 => ddl_editor(ctx, &d.trigger_ddl, "trigger", "CREATE TRIGGER ... BEGIN ... END;"),
        _ => ddl_editor(ctx, &d.ddl_preview, "preview", "-- 保存后将执行的 DDL"),
    };

    let mut content = column![header, tabs, body].width(Length::Fill);
    if let Some(err) = &d.error {
        content = content.push(banner(ctx, err, t.error, t.error_soft));
    }
    if let Some(ddl) = &d.saved {
        content = content.push(banner(ctx, &format!("已执行：\n{ddl}"), t.success, t.success_soft));
    }

    scrollable(content).height(Length::Fill).into()
}

fn designer_columns<'a>(d: &DesignerState, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;
    let Some(def) = &d.def else {
        return kit::empty_state(ctx, Icon::Table, "未加载表结构", "从连接树选择表后点「设计表」进入", None);
    };

    let head = row![
        kit::txt(ctx, "#", ts::MICRO, t.text_tertiary),
        container(kit::txt(ctx, "列名", ts::MICRO, t.text_tertiary)).width(Length::FillPortion(2)),
        container(kit::txt(ctx, "类型", ts::MICRO, t.text_tertiary)).width(Length::FillPortion(2)),
        kit::txt(ctx, "可空", ts::MICRO, t.text_tertiary),
        kit::txt(ctx, "主键", ts::MICRO, t.text_tertiary),
        container(kit::txt(ctx, "默认值", ts::MICRO, t.text_tertiary)).width(Length::FillPortion(2)),
        container(kit::txt(ctx, "备注", ts::MICRO, t.text_tertiary)).width(Length::FillPortion(2)),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .padding(Padding { top: 4.0, right: 12.0, bottom: 4.0, left: 12.0 });

    let mut rows = column![].spacing(4);
    for (i, col) in def.columns.iter().enumerate() {
        rows = rows.push(col_row(ctx, i, col.name.clone(), col.data_type.clone(), col.nullable, col.is_pk, col.default.clone().unwrap_or_default(), col.comment.clone().unwrap_or_default(), def.columns.len()));
    }

    column![head, rows]
        .padding(Padding { top: 8.0, right: 16.0, bottom: 16.0, left: 16.0 })
        .width(Length::Fill)
        .into()
}

#[allow(clippy::too_many_arguments)]
fn col_row<'a>(
    ctx: Ctx,
    idx: usize,
    name: String,
    dtype: String,
    nullable: bool,
    pk: bool,
    default: String,
    comment: String,
    total: usize,
) -> Element<'a, Msg> {
    let t = ctx.t;
    row![
        kit::txt(ctx, format!("{}", idx + 1), ts::MICRO, t.text_tertiary),
        container(kit::input(ctx, "列名", &name, move |v| Msg::DesignerColField(
            idx,
            "name".into(),
            v
        )))
        .width(Length::FillPortion(2)),
        container(kit::input(ctx, "类型", &dtype, move |v| Msg::DesignerColField(
            idx,
            "type".into(),
            v
        )))
        .width(Length::FillPortion(2)),
        kit::switch(ctx, nullable, move |v| Msg::DesignerColField(
            idx,
            "nullable".into(),
            v.to_string()
        )),
        kit::switch(ctx, pk, move |v| Msg::DesignerColField(
            idx,
            "pk".into(),
            v.to_string()
        )),
        container(kit::input(ctx, "默认值", &default, move |v| Msg::DesignerColField(
            idx,
            "default".into(),
            v
        )))
        .width(Length::FillPortion(2)),
        container(kit::input(ctx, "备注", &comment, move |v| Msg::DesignerColField(
            idx,
            "comment".into(),
            v
        )))
        .width(Length::FillPortion(2)),
        if total > 1 {
            kit::ghost(ctx, "删除", Msg::DesignerDelCol(idx))
        } else {
            Space::new().width(Length::Fixed(0.0)).height(Length::Fixed(0.0)).into()
        },
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .padding(Padding { top: 3.0, right: 12.0, bottom: 3.0, left: 12.0 })
    .into()
}

fn ddl_editor<'a>(ctx: Ctx, value: &str, which: &'static str, placeholder: &str) -> Element<'a, Msg> {
    let t = ctx.t;
    container(
        column![
            kit::txt(ctx, placeholder, ts::MICRO, t.text_tertiary),
            kit::input_mono(ctx, placeholder, value, move |v| Msg::DesignerDdlField(
                which.into(),
                v
            )),
        ]
        .spacing(6),
    )
    .padding(Padding { top: 12.0, right: 16.0, bottom: 16.0, left: 16.0 })
    .width(Length::Fill)
    .into()
}

// ===========================================================================
// BI 仪表板
// ===========================================================================

pub fn dash_view<'a>(app: &'a App, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;
    let d = &app.dash;

    let mut page_tabs = row![].spacing(4).align_y(Alignment::Center);
    for (i, p) in d.pages.iter().enumerate() {
        page_tabs = page_tabs.push(kit::underline_tab(ctx, &p.name, i == d.active, Msg::DashPick(i)));
    }
    page_tabs = page_tabs.push(kit::ghost(ctx, "+ 新页", Msg::DashAddPage));

    let toolbar = container(
        row![
            page_tabs,
            kit::hspace(),
            kit::txt(
                ctx,
                if d.auto_refresh {
                    format!("自动刷新 {}s", d.refresh_secs)
                } else {
                    "自动刷新已关闭".into()
                },
                ts::MICRO,
                t.text_tertiary
            ),
            kit::switch(ctx, d.auto_refresh, |_| Msg::DashToggleAuto),
            kit::secondary(ctx, "平铺", Msg::DashToggleTiled),
            kit::secondary(ctx, "全部运行", Msg::DashRunAll),
            kit::secondary(ctx, "+ 图表", Msg::DashAddChart(d.active)),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .padding(Padding { top: 8.0, right: 16.0, bottom: 8.0, left: 16.0 }),
    )
    .width(Length::Fill);

    let mut params_row = row![].spacing(10).align_y(Alignment::Center);
    params_row = params_row.push(kit::txt(ctx, "查询参数 :name →", ts::MICRO, t.text_tertiary));
    for (name, value) in &d.params {
        let n = name.clone();
        params_row = params_row.push(
            row![
                kit::txt(ctx, format!(":{name}"), ts::MICRO, t.text_secondary),
                kit::input(ctx, name, value, move |v| Msg::DashParamEdit(n.clone(), v)),
            ]
            .spacing(4)
            .align_y(Alignment::Center),
        );
    }
    let next_param = format!("p{}", d.params.len() + 1);
    params_row = params_row.push(kit::ghost(ctx, "+ 参数", Msg::DashParamEdit(next_param, String::new())));
    let params_bar = container(params_row)
        .padding(Padding { top: 6.0, right: 16.0, bottom: 6.0, left: 16.0 })
        .width(Length::Fill);

    let mut grid = column![].spacing(10);
    if let Some(p) = d.pages.get(d.active) {
        let mut cards = row![].spacing(10);
        for (i, chart) in p.charts.iter().enumerate() {
            cards = cards.push(chart_card(ctx, i, chart, d.tiled));
            if d.tiled && (i + 1) % 2 == 0 {
                grid = grid.push(cards);
                cards = row![].spacing(10);
            }
        }
        grid = grid.push(cards);
    }

    scrollable(
        column![
            toolbar,
            params_bar,
            grid.padding(Padding { top: 8.0, right: 16.0, bottom: 16.0, left: 16.0 })
        ]
        .width(Length::Fill),
    )
    .height(Length::Fill)
    .into()
}

fn chart_card<'a>(ctx: Ctx, idx: usize, chart: &ChartCard, tiled: bool) -> Element<'a, Msg> {
    let t = ctx.t;
    let width = if tiled { Length::FillPortion(1) } else { Length::Fill };

    let kind_opts: Vec<String> = ChartKind::ALL.iter().map(|k| k.label().to_string()).collect();
    let current = chart.kind.label().to_string();

    let controls = row![
        kit::input(ctx, "标题", &chart.title, move |v| Msg::DashChartTitle(idx, v)),
        pick_list(kind_opts, Some(current), move |label| {
            let kind = ChartKind::ALL
                .iter()
                .find(|k| k.label() == label)
                .copied()
                .unwrap_or(ChartKind::Bar);
            Msg::DashChartKind(idx, kind)
        })
        .text_size(ctx.f(ts::MICRO))
        .font(ctx.ui)
        .width(Length::Fixed(90.0))
        .style(kit::select_style(ctx))
        .menu_style(kit::select_menu_style(ctx)),
        kit::secondary(ctx, "运行", Msg::DashRun(idx)),
        kit::ghost(ctx, "删除", Msg::DashDelChart(idx)),
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    let sql_input = kit::input_mono(ctx, "SELECT 类目, 数值 FROM …", &chart.sql, move |v| {
        Msg::DashChartSql(idx, v)
    });

    let chart_canvas = mini_chart(ctx, chart);

    let mut body = column![controls, sql_input, chart_canvas].spacing(8);
    if let Some(err) = &chart.error {
        body = body.push(kit::txt(ctx, err.clone(), ts::MICRO, t.error));
    }

    container(body)
        .padding(12)
        .width(width)
        .style(move |_: &Theme| container::Style {
            background: Some(t.bg.into()),
            border: Border { color: t.hairline, width: 1.0, radius: radius::MD.into() },
            ..Default::default()
        })
        .into()
}

/// 轻量自绘图表（柱 / 线 / 面积 / 饼），纯 canvas、无外部依赖。
fn mini_chart<'a>(ctx: Ctx, chart: &ChartCard) -> Element<'a, Msg> {
    struct MiniChart {
        values: Vec<f64>,
        kind: ChartKind,
        ctx: Ctx,
        empty: bool,
    }

    impl canvas::Program<Msg> for MiniChart {
        type State = ();

        fn draw(
            &self,
            _state: &Self::State,
            renderer: &Renderer,
            _theme: &Theme,
            bounds: Rectangle,
            _cursor: iced::mouse::Cursor,
        ) -> Vec<Geometry> {
            let mut frame = Frame::new(renderer, bounds.size());
            let t = self.ctx.t;
            if self.empty {
                frame.fill_text(Text {
                    content: "运行查询后显示图表".into(),
                    position: Point::new(bounds.width / 2.0 - 56.0, bounds.height / 2.0),
                    color: t.text_tertiary,
                    size: Pixels(12.0),
                    ..Default::default()
                });
                return vec![frame.into_geometry()];
            }
            let max = self.values.iter().cloned().fold(f64::MIN, f64::max).max(1.0);
            let n = self.values.len().max(1);
            let pad = 24.0f32;
            let w = (bounds.width - pad * 2.0).max(10.0);
            let h = (bounds.height - pad * 2.0).max(10.0);

            let colors = [t.accent, t.success, t.warning, t.error, t.accent_hover, t.text_secondary];

            match self.kind {
                ChartKind::Pie => {
                    let total: f64 = self.values.iter().sum::<f64>().max(1e-9);
                    let center = Point::new(bounds.width / 2.0, bounds.height / 2.0);
                    let r = h.min(w) / 2.0;
                    let mut acc = -std::f32::consts::FRAC_PI_2;
                    for (i, v) in self.values.iter().enumerate() {
                        let frac = (*v as f32 / total as f32) * std::f32::consts::TAU;
                        let segs = 24;
                        let path = Path::new(|b| {
                            b.move_to(center);
                            for s in 0..=segs {
                                let a = acc + frac * (s as f32 / segs as f32);
                                b.line_to(Point::new(center.x + r * a.cos(), center.y + r * a.sin()));
                            }
                            b.close();
                        });
                        frame.fill(&path, colors[i % colors.len()]);
                        acc += frac;
                    }
                }
                ChartKind::Bar => {
                    let slot = w / n as f32;
                    let bw = (slot * 0.7).max(2.0);
                    for (i, v) in self.values.iter().enumerate() {
                        let bh = (*v as f32 / max as f32) * h;
                        let x = pad + i as f32 * slot + (slot - bw) / 2.0;
                        frame.fill_rectangle(
                            Point::new(x, pad + h - bh),
                            iced::Size::new(bw, bh),
                            t.accent,
                        );
                    }
                }
                ChartKind::Line | ChartKind::Area => {
                    let step = w / (n.saturating_sub(1).max(1)) as f32;
                    let pt = |i: usize, v: &f64| Point::new(pad + i as f32 * step, pad + h - (*v as f32 / max as f32) * h);
                    if self.kind == ChartKind::Area {
                        let area = Path::new(|b| {
                            b.move_to(Point::new(pad, pad + h));
                            for (i, v) in self.values.iter().enumerate() {
                                b.line_to(pt(i, v));
                            }
                            b.line_to(Point::new(pad + step * (n.saturating_sub(1).max(1)) as f32, pad + h));
                            b.close();
                        });
                        frame.fill(&area, crate::theme::rgba(0x2383E2, 0.18));
                    }
                    let line = Path::new(|b| {
                        for (i, v) in self.values.iter().enumerate() {
                            let p = pt(i, v);
                            if i == 0 {
                                b.move_to(p);
                            } else {
                                b.line_to(p);
                            }
                        }
                    });
                    frame.stroke(&line, Stroke::default().with_width(2.0).with_color(t.accent));
                }
            }
            vec![frame.into_geometry()]
        }
    }

    Canvas::new(MiniChart {
        values: chart.values.clone(),
        kind: chart.kind,
        ctx,
        empty: chart.values.is_empty(),
    })
    .width(Length::Fill)
    .height(Length::Fixed(180.0))
    .into()
}

// ===========================================================================
// 迁移与同步
// ===========================================================================

/// 下拉选项：连接显示名 → ConnId。
#[derive(Debug, Clone, PartialEq, Eq)]
struct ConnOpt {
    id: ConnId,
    label: String,
}

impl std::fmt::Display for ConnOpt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

pub fn transfer_view<'a>(app: &'a App, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;
    let s = &app.transfer;

    let mode_seg = kit::segmented(
        ctx,
        &[
            ("结构同步", s.mode == TransferMode::Structure),
            ("数据传输", s.mode == TransferMode::Data),
            ("数据同步", s.mode == TransferMode::Sync),
            ("跨库复制", s.mode == TransferMode::Copy),
        ],
        |i| Msg::TransferModePick(i),
    );

    let opts: Vec<ConnOpt> = app
        .conns
        .iter()
        .map(|c| ConnOpt {
            id: c.id,
            label: format!("{} · {}", c.name, c.kind.short()),
        })
        .collect();
    let src_sel = s.source_conn.and_then(|id| opts.iter().find(|o| o.id == id).cloned());
    let dst_sel = s.target_conn.and_then(|id| opts.iter().find(|o| o.id == id).cloned());

    let src_pick = pick_list(opts.clone(), src_sel, |o: ConnOpt| Msg::TransferSource(o.id))
        .text_size(ctx.f(ts::SMALL))
        .font(ctx.ui)
        .width(Length::Fixed(220.0))
        .style(kit::select_style(ctx))
        .menu_style(kit::select_menu_style(ctx));
    let dst_pick = pick_list(opts, dst_sel, |o: ConnOpt| Msg::TransferTarget(o.id))
        .text_size(ctx.f(ts::SMALL))
        .font(ctx.ui)
        .width(Length::Fixed(220.0))
        .style(kit::select_style(ctx))
        .menu_style(kit::select_menu_style(ctx));

    let form = column![
        kit::txt_bold(ctx, "迁移与同步", ts::H1, t.text),
        kit::txt(ctx, "选择模式、源与目标连接，执行结构 / 数据迁移。", ts::SMALL, t.text_secondary),
        Space::new().width(Length::Fill).height(Length::Fixed(12.0)),
        mode_seg,
        Space::new().width(Length::Fill).height(Length::Fixed(10.0)),
        row![
            kit::txt(ctx, "源连接", ts::SMALL, t.text_secondary),
            src_pick,
            kit::txt(ctx, "目标连接", ts::SMALL, t.text_secondary),
            dst_pick,
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        Space::new().width(Length::Fill).height(Length::Fixed(8.0)),
        row![
            kit::txt(ctx, "表名", ts::SMALL, t.text_secondary),
            kit::input(ctx, "要迁移的表", &s.table, Msg::TransferTable),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        Space::new().width(Length::Fill).height(Length::Fixed(12.0)),
        row![
            kit::primary(ctx, if s.running { "执行中…" } else { "开始迁移" }, Msg::TransferRun),
            kit::ghost(ctx, "清空日志", Msg::TransferLog(String::new())),
        ]
        .spacing(8),
    ]
    .spacing(4)
    .padding(Padding { top: 16.0, right: 20.0, bottom: 12.0, left: 20.0 });

    let mut log_col = column![].spacing(2);
    for line in &s.log {
        log_col = log_col.push(kit::mono(ctx, line.clone(), ts::MICRO, t.text_secondary));
    }
    let log = container(log_col)
        .padding(12)
        .width(Length::Fill)
        .style(move |_: &Theme| container::Style {
            background: Some(t.soft.into()),
            border: Border { color: t.hairline, width: 1.0, radius: radius::SM.into() },
            ..Default::default()
        });

    let mut content = column![form, log].spacing(8);
    if let Some(e) = &s.error {
        content = content.push(banner(ctx, e, t.error, t.error_soft));
    }
    scrollable(content).height(Length::Fill).into()
}

// ===========================================================================
// 服务器管理
// ===========================================================================

pub fn admin_view<'a>(app: &'a App, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;
    let a = &app.admin;

    let tabs = container(kit::segmented(
        ctx,
        &[
            ("用户权限", a.tab == 0),
            ("命令监控", a.tab == 1),
            ("聚合管道", a.tab == 2),
            ("Pub-Sub", a.tab == 3),
            ("模式分析", a.tab == 4),
        ],
        |i| Msg::AdminTab(i),
    ))
    .padding(Padding { top: 12.0, right: 16.0, bottom: 8.0, left: 16.0 });

    let mut controls = row![].spacing(8).align_y(Alignment::Center);
    match a.tab {
        2 => {
            controls = controls.push(
                kit::input_mono(ctx, "[{ \"$match\": { \"status\": \"active\" } }]", &a.pipeline, |v| {
                    Msg::AdminPipeline(v)
                }),
            );
        }
        3 => {
            controls = controls.push(kit::input(ctx, "频道 pattern（可空）", &a.pubsub_channel, |v| {
                Msg::AdminPubSubChannel(v)
            }));
            controls = controls.push(kit::secondary(
                ctx,
                if a.pubsub_live { "停止订阅" } else { "开始订阅" },
                Msg::AdminPubSubToggle,
            ));
        }
        _ => {}
    }
    controls = controls.push(kit::hspace());
    controls = controls.push(kit::primary(ctx, if a.running { "执行中…" } else { "执行" }, Msg::AdminRun));
    let controls_bar = container(controls)
        .padding(Padding { top: 6.0, right: 16.0, bottom: 8.0, left: 16.0 })
        .width(Length::Fill);

    let mut content = column![tabs, controls_bar];
    if let Some(e) = &a.error {
        content = content.push(banner(ctx, e, t.error, t.error_soft));
    }
    content = content.push(admin_table(ctx, &a.columns, &a.rows));
    if a.tab == 3 && !a.pubsub_messages.is_empty() {
        let mut msg_col = column![].spacing(2);
        for m in &a.pubsub_messages {
            msg_col = msg_col.push(kit::mono(ctx, m.clone(), ts::MICRO, t.text_secondary));
        }
        content = content.push(
            container(msg_col).padding(12).width(Length::Fill).style(move |_: &Theme| container::Style {
                background: Some(t.soft.into()),
                border: Border { color: t.hairline, width: 1.0, radius: radius::SM.into() },
                ..Default::default()
            }),
        );
    }
    scrollable(content).height(Length::Fill).into()
}

fn admin_table<'a>(ctx: Ctx, columns: &[String], rows: &[Vec<String>]) -> Element<'a, Msg> {
    let t = ctx.t;
    if columns.is_empty() {
        return kit::empty_state(ctx, Icon::Chart, "暂无数据", "点击「执行」加载信息", None);
    }
    let mut head = row![].spacing(10);
    for c in columns {
        head = head.push(container(kit::txt(ctx, c.clone(), ts::MICRO, t.text_secondary)).width(Length::FillPortion(1)));
    }
    let mut body = column![].spacing(3);
    for r in rows.iter().take(200) {
        let mut line = row![].spacing(10);
        for cell in r {
            line = line.push(container(kit::txt(ctx, cell.clone(), ts::MICRO, t.text)).width(Length::FillPortion(1)));
        }
        body = body.push(line);
    }
    container(column![head, body].spacing(6))
        .padding(12)
        .width(Length::Fill)
        .into()
}

// ===========================================================================
// 导入向导
// ===========================================================================

pub fn import_panel<'a>(app: &'a App, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;
    let im = &app.import;

    let mut col = column![
        kit::txt_bold(ctx, "导入数据", ts::H2, t.text),
        kit::txt(ctx, "支持 CSV / JSON / TSV / XLSX，导入前预览 100 行。", ts::SMALL, t.text_secondary),
        Space::new().width(Length::Fill).height(Length::Fixed(10.0)),
        row![
            kit::secondary(ctx, "选择文件…", Msg::ImportOpen),
            kit::txt(ctx, im.path.clone(), ts::MICRO, t.text_tertiary),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        Space::new().width(Length::Fill).height(Length::Fixed(8.0)),
        row![
            kit::txt(ctx, "目标表", ts::SMALL, t.text_secondary),
            kit::input(ctx, "表名", &im.table, Msg::ImportTable),
            kit::primary(ctx, if im.running { "导入中…" } else { "开始导入" }, Msg::ImportRun),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    ]
    .spacing(4);

    if !im.columns.is_empty() {
        let mut head = row![].spacing(8);
        for c in &im.columns {
            head = head.push(kit::txt(ctx, c.clone(), ts::MICRO, t.text_secondary));
        }
        let mut rows_col = column![].spacing(2);
        for r in im.preview.iter().take(20) {
            let mut line = row![].spacing(8);
            for v in r {
                line = line.push(kit::txt(ctx, v.display(), ts::MICRO, t.text));
            }
            rows_col = rows_col.push(line);
        }
        col = col.push(Space::new().width(Length::Fill).height(Length::Fixed(8.0)));
        col = col.push(kit::txt(
            ctx,
            format!("预览（{} 列 × {} 行）", im.columns.len(), im.preview.len()),
            ts::MICRO,
            t.text_tertiary,
        ));
        col = col.push(
            container(column![head, rows_col].spacing(4))
                .padding(8)
                .width(Length::Fill)
                .style(move |_: &Theme| container::Style {
                    background: Some(t.soft.into()),
                    border: Border { color: t.hairline, width: 1.0, radius: radius::SM.into() },
                    ..Default::default()
                }),
        );
    }
    if let Some(e) = &im.error {
        col = col.push(banner(ctx, e, t.error, t.error_soft));
    }
    if let Some(r) = &im.result {
        col = col.push(banner(ctx, r, t.success, t.success_soft));
    }
    col.padding(16).width(Length::Fill).into()
}

// ===========================================================================
// 公共
// ===========================================================================

pub fn banner<'a>(ctx: Ctx, msg: &str, fg: Color, bg: Color) -> Element<'a, Msg> {
    container(kit::txt(ctx, msg.to_string(), ts::SMALL, fg))
        .padding(10)
        .width(Length::Fill)
        .style(move |_: &Theme| container::Style {
            background: Some(bg.into()),
            border: Border::default().rounded(radius::XS),
            ..Default::default()
        })
        .into()
}
