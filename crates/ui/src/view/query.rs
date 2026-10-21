//! 查询编辑器页：工具栏 + SQL 编辑器 + 结果网格。

use crate::icons::Icon;
use crate::kit::{self, Ctx};
use crate::state::ResultTab;
use crate::theme::{radius, size as ts, space};
use crate::widgets::{editor::Editor, grid};
use crate::{App, Msg};
use iced::widget::{column, container, row, scrollable, text, Space};
use iced::{Alignment, Border, Color, Element, Length, Padding, Theme};

pub fn view<'a>(app: &'a App, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;
    let Some(tab) = app.current_query() else {
        return kit::empty_state(ctx, Icon::Terminal, "没有打开的查询", "点击页签栏的 + 新建查询", None);
    };

    let run_label = if tab.running { "执行中…" } else { "执行" };
    // 原型：主按钮内嵌 F5，其余为描边次按钮，右侧无附加信息
    let toolbar = container(
        row![
            kit::primary_with_hint(ctx, Icon::Play, run_label, "F5", Msg::QueryRun),
            kit::secondary(ctx, "停止", Msg::QueryStop),
            kit::secondary(ctx, "格式化", Msg::QueryFormat),
            kit::secondary(ctx, "保存", Msg::QuerySave),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .padding(Padding { top: 8.0, right: 12.0, bottom: 8.0, left: 10.0 }),
    )
    .width(Length::Fill)
    .style(move |_: &Theme| container::Style {
        border: Border { color: t.hairline, width: 1.0, radius: 0.0.into() },
        ..Default::default()
    });

    let editor = scrollable(
        container(Editor::new(&tab.sql, tab.caret, tab.anchor, ctx, Msg::EditorClick)
            .focused(app.editor_focus)
            .blink(app.caret_blink)
            .min_height(200.0)
            .view())
        .padding(Padding { top: 8.0, right: 12.0, bottom: 8.0, left: 4.0 })
        .width(Length::Fill),
    )
    .height(Length::FillPortion(2))
    .style(crate::view::shell::scroll_style(ctx));

    let results = results_panel(app, tab, ctx);

    column![
        toolbar,
        container(editor).width(Length::Fill).height(Length::FillPortion(2)).style(move |_: &Theme| container::Style {
            background: Some(t.bg.into()),
            ..Default::default()
        }),
        container(Space::new().width(Length::Fill).height(Length::Fixed(1.0))).width(Length::Fill).style(move |_: &Theme| container::Style {
            background: Some(t.hairline.into()),
            ..Default::default()
        }),
        container(results).height(Length::FillPortion(3)),
    ]
    .height(Length::Fill)
    .into()
}

fn results_panel<'a>(app: &'a App, tab: &'a crate::state::QueryTab, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;
    let active = tab.result_tab;

    let header = container(
        row![
            text("结果")
                .size(ctx.f(ts::SMALL))
                .font(ctx.ui_medium())
                .color(t.text),
            text(format!("{} 行 · {} 列", tab.rows.len(), tab.columns.len()))
                .size(ctx.f(ts::SMALL))
                .font(ctx.ui)
                .color(t.text_tertiary),
            Space::new().width(Length::Fixed(10.0)).height(Length::Fixed(0.0)),
            result_tab(ctx, if tab.notices.is_empty() { "消息" } else { "消息 1" }, 1, active == ResultTab::Messages),
            result_tab(ctx, "执行计划", 2, active == ResultTab::Plan),
            kit::hspace(),
            kit::link(ctx, "导出 CSV", Msg::ExportResult),
        ]
        .spacing(10)
        .align_y(Alignment::Center)
        .padding(Padding { top: 0.0, right: 12.0, bottom: 0.0, left: 12.0 }),
    )
    .height(Length::Fixed(34.0))
    .align_y(Alignment::Center)
    .width(Length::Fill)
    .style(move |_: &Theme| container::Style {
        border: Border { color: t.hairline, width: 1.0, radius: 0.0.into() },
        ..Default::default()
    });

    let body: Element<Msg> = if let Some(err) = &tab.error {
        kit::empty_state(
            ctx,
            Icon::Close,
            "执行失败",
            err,
            Some(kit::secondary(ctx, "重试", Msg::QueryRun)),
        )
    } else if tab.running {
        kit::empty_state(
            ctx,
            Icon::Clock,
            "正在执行…",
            "结果会分批返回，界面保持可交互",
            Some(kit::ghost(ctx, "停止", Msg::QueryStop)),
        )
    } else {
        match active {
            ResultTab::Grid => grid_view(tab, ctx),
            ResultTab::Messages => messages(tab, ctx),
            ResultTab::Plan => plan(app, tab, ctx),
        }
    };

    column![header, body].height(Length::Fill).into()
}

fn result_tab<'a>(ctx: Ctx, label: &str, idx: usize, active: bool) -> Element<'a, Msg> {
    let t = ctx.t;
    iced::widget::button(
        kit::txt(
            ctx,
            label.to_string(),
            ts::SMALL,
            if active { t.text } else { t.text_secondary },
        ),
    )
    .padding(Padding { top: 3.0, right: 2.0, bottom: 3.0, left: 2.0 })
    .style(move |_: &Theme, status| iced::widget::button::Style {
        background: if matches!(status, iced::widget::button::Status::Hovered) {
            Some(t.hover.into())
        } else {
            None
        },
        text_color: t.text,
        border: Border::default(),
        ..Default::default()
    })
    .on_press(Msg::ResultTabPick(idx))
    .into()
}

fn grid_view<'a>(tab: &'a crate::state::QueryTab, ctx: Ctx) -> Element<'a, Msg> {
    if tab.columns.is_empty() {
        return kit::empty_state(ctx, Icon::Table, "暂无结果", "执行 SQL 后将在此显示数据", None);
    }
    // 用可用宽度算像素列宽，避免长文本把列撑出面板
    iced::widget::responsive(move |size| grid_body(tab, ctx, size.width)).into()
}

fn grid_body<'a>(tab: &'a crate::state::QueryTab, ctx: Ctx, width: f32) -> Element<'a, Msg> {
    let t = ctx.t;
    let widths = grid::widths(&tab.columns, (width - 16.0).max(200.0));
    let page_rows = tab.page_rows();
    let start = if page_rows.is_empty() { 0 } else { tab.page * tab.page_size + 1 };
    let end = (tab.page * tab.page_size + page_rows.len()).min(tab.rows.len());

    let mut rows = column![].spacing(0).width(Length::Fill);
    for (i, r) in page_rows.iter().enumerate() {
        rows = rows.push(grid::row_view(ctx, &tab.columns, &widths, r, i % 2 == 1, None::<Msg>));
        rows = rows.push(
            container(Space::new().width(Length::Fill).height(Length::Fixed(1.0)))
                .width(Length::Fill)
                .style(move |_: &Theme| container::Style { background: Some(t.hairline.into()), ..Default::default() }),
        );
    }

    let footer = container(
        row![
            kit::txt(
                ctx,
                format!("第 {}–{} 行，共 {} 行 · {}ms", start, end, tab.rows.len(), tab.elapsed_ms),
                ts::MICRO,
                t.text_tertiary
            ),
            kit::hspace(),
            kit::icon_button(ctx, Icon::ChevronRight, 11.0, t.text_tertiary, Msg::Noop),
            kit::txt(
                ctx,
                format!("{} / {}", tab.page + 1, tab.page_count().max(1)),
                ts::MICRO,
                t.text_secondary
            ),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .padding(Padding { top: 4.0, right: 12.0, bottom: 4.0, left: 12.0 }),
    )
    .width(Length::Fill);

    column![
        grid::header(ctx, &tab.columns, &widths, None, None::<Box<dyn Fn(String) -> Msg>>),
        scrollable(rows).height(Length::Fill).style(crate::view::shell::scroll_style(ctx)),
        footer,
    ]
    .height(Length::Fill)
    .into()
}

fn messages<'a>(tab: &'a crate::state::QueryTab, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;
    let mut col = column![].spacing(4).padding(Padding { top: 10.0, right: 14.0, bottom: 10.0, left: 14.0 });
    if tab.notices.is_empty() {
        col = col.push(kit::txt(ctx, "语句执行成功，无额外消息。", ts::SMALL, t.text_secondary));
    }
    for n in &tab.notices {
        col = col.push(kit::mono(ctx, n.clone(), ts::SMALL, t.text));
    }
    col = col.push(kit::txt(
        ctx,
        format!("返回 {} 行 · 耗时 {} ms", tab.rows.len(), tab.elapsed_ms),
        ts::SMALL,
        t.text_secondary,
    ));
    if tab.truncated {
        col = col.push(kit::txt(
            ctx,
            "结果集已达上限被截断，可在连接设置中调整最大返回行数",
            ts::SMALL,
            t.warning,
        ));
    }
    scrollable(col).height(Length::Fill).style(crate::view::shell::scroll_style(ctx)).into()
}

fn plan<'a>(app: &'a App, tab: &'a crate::state::QueryTab, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;
    let mut col = column![].spacing(6).padding(Padding { top: 10.0, right: 14.0, bottom: 10.0, left: 14.0 });
    col = col.push(kit::txt(
        ctx,
        "执行计划由引擎解析后返回；点击下方按钮可对当前语句执行 EXPLAIN。",
        ts::SMALL,
        t.text_secondary,
    ));
    col = col.push(kit::secondary(ctx, "生成执行计划", Msg::QueryRun));
    if let Some(core) = &app.core {
        let _ = core;
    }
    let _ = tab;
    scrollable(col).height(Length::Fill).style(crate::view::shell::scroll_style(ctx)).into()
}
