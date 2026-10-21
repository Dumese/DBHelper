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
            kit::segmented(
                ctx,
                &[
                    ("网格", tab.view_mode == 0),
                    ("表单", tab.view_mode == 1),
                    ("树", tab.view_mode == 2),
                    ("JSON", tab.view_mode == 3),
                ],
                Msg::DataViewMode
            ),
            kit::switch(ctx, tab.mask, |_| Msg::DataMaskToggle),
            kit::txt(ctx, "脱敏", ts::MICRO, t.text_secondary),
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

    let batch_bar = container(
        row![
            kit::txt(ctx, "批量编辑：把", ts::MICRO, t.text_secondary),
            kit::input(ctx, "列名", &tab.batch_field, Msg::DataBatchField),
            kit::txt(ctx, "统一设为", ts::MICRO, t.text_secondary),
            kit::input(ctx, "值（空=NULL）", &tab.batch_value, Msg::DataBatchValue),
            kit::secondary(ctx, "应用到全部行", Msg::DataBatchApply),
            kit::hspace(),
            kit::txt(ctx, format!("{} 行待提交", tab.dirty), ts::MICRO, if tab.dirty > 0 { t.warning } else { t.text_tertiary }),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .padding(Padding { top: 5.0, right: 12.0, bottom: 5.0, left: 12.0 }),
    )
    .width(Length::Fill)
    .style(move |_: &Theme| container::Style {
        background: Some(t.soft.into()),
        border: Border { color: t.hairline, width: 1.0, radius: 0.0.into() },
        ..Default::default()
    });

    let main: Element<Msg> = match tab.view_mode {
        1 => form_view(tab, ctx),
        2 => tree_view(tab, ctx),
        3 => json_view(tab, ctx),
        _ => {
            let grid_area = iced::widget::responsive(move |size| data_grid(tab, ctx, size.width));
            grid_area.into()
        }
    };

    let edit_panel = edit_panel(app, ctx);

    column![toolbar, batch_bar, main, edit_panel].height(Length::Fill).into()
}

/// 敏感列脱敏：手机号 / 邮箱 / 身份证 / 密码类列名打码。
pub fn mask_cell(col: &str, v: &dbhelper_models::Value) -> dbhelper_models::Value {
    use dbhelper_models::Value;
    let key = col.to_lowercase();
    let sensitive = key.contains("phone")
        || key.contains("mobile")
        || key.contains("手机")
        || key.contains("email")
        || key.contains("mail")
        || key.contains("id_card")
        || key.contains("idcard")
        || key.contains("身份证")
        || key.contains("password")
        || key.contains("passwd")
        || key.contains("pwd")
        || key.contains("secret")
        || key.contains("token");
    if !sensitive {
        return v.clone();
    }
    match v {
        Value::Null => Value::Null,
        other => {
            let s = other.display();
            let chars: Vec<char> = s.chars().collect();
            let n = chars.len();
            if n <= 2 {
                return Value::Text("*".repeat(n.max(1)));
            }
            let head: String = chars.iter().take(2).collect();
            let tail: String = chars.iter().skip(n.saturating_sub(2)).collect();
            Value::Text(format!("{head}****{tail}"))
        }
    }
}

fn masked_row(tab: &crate::state::DataTab, r: &dbhelper_models::Row) -> dbhelper_models::Row {
    if !tab.mask {
        return r.clone();
    }
    r.iter()
        .enumerate()
        .map(|(i, v)| {
            let col = tab.columns.get(i).map(|c| c.name.as_str()).unwrap_or("");
            mask_cell(col, v)
        })
        .collect()
}

/// 表单视图：选中行逐字段纵向编辑。
fn form_view<'a>(tab: &'a crate::state::DataTab, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;
    let Some(sel) = tab.selected else {
        return kit::empty_state(ctx, Icon::Doc, "未选择行", "在网格视图中点击一行后回到表单编辑", None);
    };
    let Some(row) = tab.rows.get(sel) else {
        return kit::empty_state(ctx, Icon::Doc, "行不存在", "请重新加载数据", None);
    };
    let row = masked_row(tab, row);

    let mut fields = column![].spacing(10);
    for (i, col) in tab.columns.iter().enumerate() {
        let value = row.get(i).map(|v| v.edit_text()).unwrap_or_default();
        let name = col.name.clone();
        fields = fields.push(
            row![
                container(kit::txt(ctx, format!("{}  {}", col.name, col.type_name), ts::SMALL, t.text_secondary))
                    .width(Length::FillPortion(1)),
                container(kit::input(ctx, "", &value, move |v| Msg::DataCellPick(name.clone(), v)))
                    .width(Length::FillPortion(2)),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        );
    }

    scrollable(
        container(
            column![
                kit::txt_bold(ctx, format!("表单编辑 · 第 {} 行", sel + 1), ts::H3, t.text),
                fields,
                row![
                    kit::secondary(ctx, "撤销", Msg::DataUndo),
                    kit::primary(ctx, "提交更改", Msg::DataCommit),
                ]
                .spacing(8),
            ]
            .spacing(12),
        )
        .padding(16)
        .width(Length::Fill),
    )
    .height(Length::Fill)
    .into()
}

/// 树视图：按「行 → 字段」两级折叠展示。
fn tree_view<'a>(tab: &'a crate::state::DataTab, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;
    let mut tree = column![].spacing(2);
    for (i, r) in tab.rows.iter().enumerate() {
        let r = masked_row(tab, r);
        let expanded = tab.selected == Some(i);
        let head = iced::widget::mouse_area(
            container(
                row![
                    kit::icon(
                        if expanded { crate::icons::Icon::ChevronDown } else { crate::icons::Icon::ChevronRight },
                        10.0,
                        t.text_tertiary
                    ),
                    kit::txt(ctx, format!("行 {}", i + 1), ts::SMALL, if expanded { t.text } else { t.text_secondary }),
                ]
                .spacing(5)
                .align_y(Alignment::Center),
            )
            .padding(Padding { top: 4.0, right: 8.0, bottom: 4.0, left: 8.0 })
            .width(Length::Fill)
            .style(move |_: &Theme| container::Style {
                background: Some(if expanded { t.hover } else { Color::TRANSPARENT }.into()),
                ..Default::default()
            }),
        )
        .on_press(Msg::DataSelect(i));
        tree = tree.push(head);
        if expanded {
            for (j, col) in tab.columns.iter().enumerate() {
                let v = r.get(j).map(|v| v.display()).unwrap_or_default();
                tree = tree.push(
                    row![
                        kit::gap(24.0, 1.0),
                        kit::mono(ctx, col.name.clone(), ts::MICRO, t.text_secondary),
                        kit::mono(ctx, v, ts::MICRO, t.text),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center)
                    .padding(Padding { top: 2.0, right: 8.0, bottom: 2.0, left: 0.0 }),
                );
            }
        }
    }
    scrollable(container(tree).padding(12).width(Length::Fill))
        .height(Length::Fill)
        .style(crate::view::shell::scroll_style(ctx))
        .into()
}

/// JSON 视图：选中行渲染为 JSON。
fn json_view<'a>(tab: &'a crate::state::DataTab, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;
    let Some(sel) = tab.selected else {
        return kit::empty_state(ctx, Icon::Doc, "未选择行", "在网格视图中点击一行后查看 JSON", None);
    };
    let Some(row) = tab.rows.get(sel) else {
        return kit::empty_state(ctx, Icon::Doc, "行不存在", "请重新加载数据", None);
    };
    let row = masked_row(tab, row);
    let mut map = serde_json::Map::new();
    for (i, col) in tab.columns.iter().enumerate() {
        let v = row.get(i);
        let j = match v {
            None | Some(dbhelper_models::Value::Null) => serde_json::Value::Null,
            Some(dbhelper_models::Value::Int(n)) => serde_json::json!(n),
            Some(dbhelper_models::Value::Float(f)) => serde_json::json!(f),
            Some(dbhelper_models::Value::Bool(b)) => serde_json::json!(b),
            Some(other) => serde_json::Value::String(other.display()),
        };
        map.insert(col.name.clone(), j);
    }
    let pretty = serde_json::to_string_pretty(&serde_json::Value::Object(map)).unwrap_or_default();

    scrollable(
        container(kit::mono(ctx, pretty, ts::SMALL, t.text))
            .padding(14)
            .width(Length::Fill)
            .style(move |_: &Theme| container::Style {
                background: Some(t.soft.into()),
                border: Border { color: t.hairline, width: 1.0, radius: radius::SM.into() },
                ..Default::default()
            }),
    )
    .height(Length::Fill)
    .into()
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
            let r_masked = masked_row(tab, r);
            rows = rows.push(grid::row_view(
                ctx,
                &tab.columns,
                &widths,
                &r_masked,
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
