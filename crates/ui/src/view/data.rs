//! 数据网格页：筛选、排序、分页、行内编辑。

use crate::icons::Icon;
use crate::kit::{self, Ctx};
use crate::theme::{radius, size as ts, space, Tone};
use crate::widgets::grid;
use crate::{App, Msg};
use iced::widget::{column, container, row, scrollable, Space};
use iced::{Alignment, Border, Color, Element, Length, Padding, Theme};

pub fn view<'a>(app: &'a App, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;
    let Some(tab) = app.current_data() else {
        return kit::empty_state(ctx, Icon::Table, "没有打开的数据表", "从左侧连接树选择表", None);
    };

    // 工具条
    let mut filters = row![].spacing(6).align_y(Alignment::Center);
    filters = filters.push(
        container(kit::search_field(ctx, "搜索本页数据", &tab.search, Msg::DataSearch))
            .width(Length::Fixed(180.0)),
    );
    for (i, f) in tab.filters.iter().enumerate() {
        filters = filters.push(kit::filter_chip(ctx, f.label(), Msg::DataFilterRemove(i)));
    }
    filters = filters.push(kit::ghost(ctx, "+ 添加筛选", Msg::DataAddFilter));

    let toolbar = container(
        row![
            filters,
            kit::hspace(),
            kit::txt(
                ctx,
                format!(
                    "排序: {}",
                    tab.sort
                        .as_ref()
                        .map(|(c, asc)| format!("{} {}", c, if *asc { "↑" } else { "↓" }))
                        .unwrap_or_else(|| "默认".into())
                ),
                ts::MICRO,
                t.text_secondary
            ),
            kit::icon_button(ctx, Icon::List, 13.0, t.text_secondary, Msg::DataReload),
            kit::icon_button(ctx, Icon::Gear, 13.0, t.text_secondary, Msg::Noop),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .padding(Padding { top: 7.0, right: 12.0, bottom: 7.0, left: 12.0 }),
    )
    .width(Length::Fill)
    .style(move |_: &Theme| container::Style {
        border: Border { color: t.hairline, width: 1.0, radius: 0.0.into() },
        ..Default::default()
    });

    let grid_area = iced::widget::responsive(move |size| data_grid(tab, ctx, size.width));

    let edit_panel = edit_panel(app, ctx);

    column![toolbar, grid_area, edit_panel].height(Length::Fill).into()
}

fn data_grid<'a>(tab: &'a crate::state::DataTab, ctx: Ctx, width: f32) -> Element<'a, Msg> {
    let t = ctx.t;
    let widths = grid::widths(&tab.columns, (width - 16.0).max(200.0));
    let mut rows = column![].spacing(0).width(Length::Fill);
    if tab.loading && tab.rows.is_empty() {
        rows = rows.push(kit::empty_state(ctx, Icon::Clock, "加载中…", "正在读取数据", None));
    } else if let Some(err) = &tab.error {
        rows = rows.push(kit::empty_state(
            ctx,
            Icon::Close,
            "读取失败",
            err,
            Some(kit::secondary(ctx, "重试", Msg::DataReload)),
        ));
    } else {
        for (i, r) in tab.rows.iter().enumerate() {
            let selected = tab.selected == Some(i);
            rows = rows.push(grid::row_view(
                ctx,
                &tab.columns,
                &widths,
                r,
                selected,
                Some(Msg::DataSelect(i)),
            ));
            rows = rows.push(
                container(Space::new().width(Length::Fill).height(Length::Fixed(1.0)))
                    .width(Length::Fill)
                    .style(move |_: &Theme| container::Style {
                        background: Some(t.hairline.into()),
                        ..Default::default()
                    }),
            );
        }
    }

    let start = tab.page * tab.page_size + 1;
    let end = (tab.page * tab.page_size + tab.rows.len()).min(tab.total as usize);
    let footer = container(
        row![
            kit::txt(
                ctx,
                format!("第 {}–{} 行，共 {} 行", start, end.max(start), tab.total),
                ts::MICRO,
                t.text_tertiary
            ),
            kit::hspace(),
            kit::icon_button(ctx, Icon::ChevronRight, 11.0, t.text_tertiary, Msg::DataPage(1)),
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

    let header = grid::header(
        ctx,
        &tab.columns,
        &widths,
        tab.sort.as_ref(),
        Some(Box::new(Msg::DataSort)),
    );

    column![
        header,
        scrollable(rows).height(Length::Fill).style(crate::view::shell::scroll_style(ctx)),
        footer,
    ]
    .height(Length::Fill)
    .into()
}

fn edit_panel<'a>(app: &'a App, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;
    let Some(tab) = app.current_data() else { return Space::new().width(0).height(0).into() };
    let pending = tab.pending();
    let title = match tab.selected {
        Some(i) => {
            let pk = tab
                .rows
                .get(i)
                .and_then(|r| r.first())
                .map(|v| v.display())
                .unwrap_or_default();
            format!("行内编辑 · 第 {} 行 ({})", i + 1, pk)
        }
        None => "行内编辑 · 未选择行".to_string(),
    };

    let mut fields = row![].spacing(12).align_y(Alignment::Center);
    for field in tab.edit.iter().take(4) {
        let msg_name = field.name.clone();
        fields = fields.push(
            column![
                row![
                    kit::txt(ctx, field.name.clone(), ts::MICRO, t.text_secondary),
                    kit::txt(ctx, field.type_name.clone(), ts::MICRO, t.text_tertiary),
                ]
                .spacing(6),
                kit::input_mono(ctx, "", &field.value, move |v| Msg::DataCellPick(msg_name.clone(), v)),
            ]
            .spacing(3)
            .width(Length::Fill),
        );
    }

    container(
        column![
            row![
                kit::txt_bold(ctx, title, ts::SMALL, t.text),
                kit::hspace(),
                if pending > 0 {
                    kit::txt(ctx, format!("{} 个字段待提交", pending), ts::MICRO, t.warning)
                } else {
                    kit::txt(ctx, "无待提交修改", ts::MICRO, t.text_tertiary)
                },
            ]
            .align_y(Alignment::Center),
            Space::new().width(Length::Fixed(0.0)).height(Length::Fixed(6.0)),
            fields,
            Space::new().width(Length::Fixed(0.0)).height(Length::Fixed(6.0)),
            row![
                kit::txt(
                    ctx,
                    "Ctrl+Enter 提交本行 · 支持撤销到上次快照",
                    ts::MICRO,
                    t.text_tertiary
                ),
                kit::hspace(),
                kit::secondary(ctx, "撤销", Msg::DataUndo),
                kit::primary(ctx, "提交更改", Msg::DataCommit),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        ]
        .spacing(0),
    )
    .padding(Padding { top: 10.0, right: 14.0, bottom: 10.0, left: 14.0 })
    .width(Length::Fill)
    .style(move |_: &Theme| container::Style {
        background: Some(t.soft.into()),
        border: Border { color: t.hairline, width: 1.0, radius: 0.0.into() },
        ..Default::default()
    })
    .into()
}

pub fn _tone() -> Tone {
    Tone::Neutral
}

pub fn _radius() -> iced::border::Radius {
    radius::XS.into()
}
