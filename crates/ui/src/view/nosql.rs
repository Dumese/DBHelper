//! NoSQL 专用视图：Redis 键空间 / MongoDB 文档 / Elasticsearch DSL。

use crate::icons::Icon;
use crate::kit::{self, Ctx};
use crate::theme::{radius, size as ts, space, Tone};
use crate::{App, Msg};
use iced::widget::{column, container, row, scrollable, text_editor, Space};
use iced::{Alignment, Border, Color, Element, Length, Padding, Theme};

pub fn view<'a>(app: &'a App, ctx: Ctx) -> Element<'a, Msg> {
    match app.page {
        crate::state::Page::Redis => redis(app, ctx),
        crate::state::Page::Mongo => mongo(app, ctx),
        _ => elastic(app, ctx),
    }
}

fn error_banner<'a>(ctx: Ctx, err: &str, retry: Msg) -> Element<'a, Msg> {
    let t = ctx.t;
    container(
        row![
            crate::icons::icon(Icon::Close, 13.0, t.error),
            kit::txt(ctx, err.to_string(), ts::SMALL, t.error),
            kit::hspace(),
            kit::secondary(ctx, "重试", retry),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .padding(Padding { top: 8.0, right: 12.0, bottom: 8.0, left: 12.0 }),
    )
    .width(Length::Fill)
    .style(move |_: &Theme| container::Style {
        background: Some(t.error_soft.into()),
        border: Border { color: t.hairline, width: 1.0, radius: 0.0.into() },
        ..Default::default()
    })
    .into()
}

// ---------------------------------------------------------------------------
// Redis
// ---------------------------------------------------------------------------

fn redis<'a>(app: &'a App, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;
    let st = &app.redis;

    if let Some(err) = &st.error {
        return column![
            error_banner(ctx, err, Msg::OpenRedis(st.conn.unwrap_or(0))),
            kit::empty_state(
                ctx,
                Icon::Key,
                "无法连接 Redis",
                "请检查连接配置或服务端是否可达；连接成功后此处展示键空间与五种数据类型。",
                Some(kit::secondary(ctx, "编辑连接", Msg::WizardOpen))
            ),
        ]
        .height(Length::Fill)
        .into();
    }

    let total: usize = st.info.as_ref().map(|i| i.keyspace.iter().map(|k| k.keys as usize).sum()).unwrap_or(0);
    let mut tabs = row![].spacing(4).align_y(Alignment::Center);
    let counts = [
        ("全部", total),
        ("String", 0),
        ("Hash", 0),
        ("List", 0),
        ("Set", 0),
        ("ZSet", 0),
    ];
    for (i, (label, count)) in counts.iter().enumerate() {
        tabs = tabs.push(kit::counter_tab(
            ctx,
            label,
            if *count == 0 { "" } else { "" },
            i == 0,
            Msg::RedisTypeTab(i),
        ));
    }
    tabs = tabs.push(kit::hspace());
    tabs = tabs.push(kit::link(ctx, "刷新", Msg::RedisScan(st.pattern.clone())));

    let header = container(tabs.padding(Padding { top: 6.0, right: 12.0, bottom: 6.0, left: 10.0 }))
        .width(Length::Fill)
        .style(move |_: &Theme| container::Style {
            border: Border { color: t.hairline, width: 1.0, radius: 0.0.into() },
            ..Default::default()
        });

    let Some(key) = &st.selected else {
        return column![
            header,
            kit::empty_state(
                ctx,
                Icon::Key,
                "选择一个键",
                &format!(
                    "db{} 中共 {} 个键，从左侧键空间选择查看字段值、TTL 与编码。",
                    st.db,
                    st.keys.len()
                ),
                None
            ),
        ]
        .height(Length::Fill)
        .into();
    };

    let key_header = container(
        row![
            kit::txt_bold(ctx, key.clone(), ts::H2, t.text),
            kit::badge(ctx, st.key_type.clone(), Tone::Accent),
            kit::txt(ctx, format!("TTL {} s", st.ttl), ts::MICRO, t.warning),
            kit::hspace(),
            kit::secondary(ctx, "删除键", Msg::RedisDelete),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .padding(Padding { top: 10.0, right: 14.0, bottom: 8.0, left: 14.0 }),
    )
    .width(Length::Fill);

    let mut tabs2 = row![].spacing(18).align_y(Alignment::Center);
    for (i, label) in ["字段值", "元信息", "命令历史", "编辑"].iter().enumerate() {
        tabs2 = tabs2.push(kit::underline_tab(ctx, label, st.tab == i, Msg::RedisTypeTab(i)));
    }

    let body: Element<Msg> = match st.tab {
        1 => meta(ctx, st),
        2 => history(ctx, st),
        3 => edit(ctx, st),
        _ => fields(ctx, st),
    };

    column![
        header,
        key_header,
        container(tabs2.padding(Padding { top: 6.0, right: 14.0, bottom: 0.0, left: 14.0 })),
        body,
    ]
    .height(Length::Fill)
    .into()
}

fn fields<'a>(ctx: Ctx, st: &'a crate::state::RedisState) -> Element<'a, Msg> {
    let t = ctx.t;
    let mut rows = column![].spacing(0);
    rows = rows.push(
        container(
            row![
                container(kit::txt(ctx, "field", ts::MICRO, t.text_secondary)).width(Length::FillPortion(2)),
                container(kit::txt(ctx, "value", ts::MICRO, t.text_secondary)).width(Length::FillPortion(6)),
            ]
            .padding(Padding { top: 6.0, right: 14.0, bottom: 6.0, left: 14.0 }),
        )
        .width(Length::Fill)
        .style(move |_: &Theme| container::Style {
            background: Some(t.soft.into()),
            ..Default::default()
        }),
    );
    for (f, v) in &st.fields {
        rows = rows.push(
            container(
                row![
                    container(kit::mono(ctx, f.clone(), ts::SMALL, t.text_secondary)).width(Length::FillPortion(2)),
                    container(kit::mono(ctx, v.clone(), ts::SMALL, t.text)).width(Length::FillPortion(6)),
                ]
                .padding(Padding { top: 7.0, right: 14.0, bottom: 7.0, left: 14.0 }),
            )
            .width(Length::Fill)
            .style(move |_: &Theme| container::Style {
                border: Border { color: t.hairline, width: 0.0, radius: 0.0.into() },
                ..Default::default()
            }),
        );
        rows = rows.push(kit::hline(ctx));
    }
    let footer = container(kit::txt(
        ctx,
        format!(
            "{} · {} fields · 内存占用 {} · 编码 {}",
            st.key_type.to_lowercase(),
            st.fields.len(),
            dbhelper_utils::textfmt::bytes(st.memory),
            st.encoding
        ),
        ts::MICRO,
        t.text_tertiary,
    ))
    .padding(12);
    column![
        scrollable(rows).height(Length::Fill).style(crate::view::shell::scroll_style(ctx)),
        footer
    ]
    .height(Length::Fill)
    .into()
}

fn meta<'a>(ctx: Ctx, st: &'a crate::state::RedisState) -> Element<'a, Msg> {
    let t = ctx.t;
    let rows = [
        ("类型", st.key_type.to_lowercase()),
        ("TTL", format!("{} 秒", st.ttl)),
        ("内存占用", dbhelper_utils::textfmt::bytes(st.memory)),
        ("编码", st.encoding.clone()),
        ("字段数", st.fields.len().to_string()),
    ];
    let mut col = column![].spacing(0);
    for (k, v) in rows {
        col = col.push(kit::setting_row(ctx, k, "", kit::txt(ctx, v, ts::SMALL, t.text)));
    }
    container(col).padding(14).into()
}

fn history<'a>(ctx: Ctx, st: &'a crate::state::RedisState) -> Element<'a, Msg> {
    let t = ctx.t;
    let mut col = column![].spacing(4).padding(14);
    if st.command_log.is_empty() {
        col = col.push(kit::txt(
            ctx,
            "尚无命令记录。读取键、修改字段与 TTL 操作会记录在此。",
            ts::SMALL,
            t.text_secondary,
        ));
    }
    for line in &st.command_log {
        col = col.push(kit::mono(ctx, line.clone(), ts::SMALL, t.text));
    }
    scrollable(col).height(Length::Fill).style(crate::view::shell::scroll_style(ctx)).into()
}

fn edit<'a>(ctx: Ctx, st: &'a crate::state::RedisState) -> Element<'a, Msg> {
    let t = ctx.t;
    let title = if st.key_type == "HASH" {
        "修改首个字段的值（Hash 场景下为 field 值）"
    } else {
        "修改键值"
    };
    column![
        container(
            column![
                kit::txt(ctx, title, ts::SMALL, t.text_secondary),
                Space::new().width(Length::Fixed(0.0)).height(Length::Fixed(6.0)),
                kit::input_mono(ctx, "", &st.edit_value, Msg::RedisEdit),
                Space::new().width(Length::Fixed(0.0)).height(Length::Fixed(10.0)),
                row![
                    kit::hspace(),
                    kit::secondary(ctx, "取消", Msg::RedisPickKey(st.selected.clone().unwrap_or_default())),
                    kit::primary(ctx, "保存", Msg::RedisSave),
                ]
                .spacing(8),
            ]
            .spacing(0),
        )
        .padding(14),
    ]
    .height(Length::Fill)
    .into()
}

// ---------------------------------------------------------------------------
// MongoDB
// ---------------------------------------------------------------------------

fn mongo<'a>(app: &'a App, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;
    let st = &app.mongo;

    if let Some(err) = &st.error {
        return column![
            error_banner(ctx, err, Msg::OpenMongo(st.conn.unwrap_or(0))),
            kit::empty_state(
                ctx,
                Icon::Doc,
                "无法连接 MongoDB",
                "请检查连接字符串、认证信息与网络可达性。",
                Some(kit::secondary(ctx, "编辑连接", Msg::WizardOpen))
            ),
        ]
        .height(Length::Fill)
        .into();
    }

    let header = container(
        row![
            kit::txt_bold(ctx, st.collection.clone().unwrap_or_else(|| "users".into()), ts::H3, t.text),
            kit::txt(
                ctx,
                format!(
                    "{} 文档 · 平均 {} · 索引 4 个",
                    st.docs.len(),
                    "1.2 KB"
                ),
                ts::MICRO,
                t.text_tertiary
            ),
            kit::hspace(),
            kit::primary(ctx, "+ 新建文档", Msg::Noop),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .padding(Padding { top: 8.0, right: 12.0, bottom: 8.0, left: 12.0 }),
    )
    .width(Length::Fill);

    let pipeline = container(
        row![
            kit::txt(ctx, "聚合管道", ts::MICRO, t.text_secondary),
            Space::new().width(Length::Fixed(6.0)).height(Length::Fixed(0.0)),
            container(
                text_editor(&app.mongo_editor)
                    .on_action(Msg::MongoAction)
                    .size(ctx.f(ts::SMALL))
                    .font(ctx.mono)
                    .padding(6)
                    .height(Length::Fixed(34.0)),
            )
            .width(Length::Fill),
            kit::primary(ctx, "执行", Msg::MongoRun),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .padding(Padding { top: 6.0, right: 12.0, bottom: 6.0, left: 12.0 }),
    )
    .width(Length::Fill)
    .style(move |_: &Theme| container::Style {
        border: Border { color: t.hairline, width: 1.0, radius: 0.0.into() },
        ..Default::default()
    });

    let mut list = column![].spacing(0);
    for (i, doc) in st.docs.iter().enumerate() {
        let name = doc.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let status = doc.get("status").and_then(|v| v.as_str()).unwrap_or("");
        let id = doc
            .get("_id")
            .map(|v| match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .unwrap_or_default();
        let selected = st.selected == Some(i);
        list = list.push(
            iced::widget::button(
                column![
                    kit::mono(ctx, id.chars().take(18).collect::<String>(), 10.0, t.text_tertiary),
                    kit::txt(ctx, format!("{name} · {status}"), ts::SMALL, t.text),
                ]
                .spacing(1)
                .width(Length::Fill)
                .padding(Padding { top: 5.0, right: 10.0, bottom: 5.0, left: 10.0 }),
            )
            .padding(0)
            .width(Length::Fill)
            .style(move |_: &Theme, status| iced::widget::button::Style {
                background: Some(
                    if selected {
                        t.selected
                    } else if matches!(status, iced::widget::button::Status::Hovered) {
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
            .on_press(Msg::MongoActions(i)),
        );
    }

    let doc_panel: Element<Msg> = match st.selected.and_then(|i| st.docs.get(i)) {
        Some(doc) => {
            let json = serde_json::to_string_pretty(doc).unwrap_or_default();
            let mut col = column![].spacing(0);
            for line in json.lines() {
                col = col.push(kit::mono(ctx, line.to_string(), ts::SMALL, json_color(ctx, line)));
            }
            column![
                container(
                    row![
                        kit::underline_tab(ctx, "JSON 树视图", true, Msg::Noop),
                        kit::underline_tab(ctx, "文本模式", false, Msg::Noop),
                        kit::hspace(),
                        kit::link(ctx, "复制", Msg::CopyText(json.clone())),
                        kit::link(ctx, "编辑", Msg::Noop),
                        kit::danger_link(ctx, "删除", Msg::MongoDeleteDoc(st.selected.unwrap_or(0))),
                    ]
                    .spacing(14)
                    .align_y(Alignment::Center)
                    .padding(Padding { top: 6.0, right: 14.0, bottom: 0.0, left: 14.0 }),
                ),
                scrollable(container(col).padding(14))
                    .height(Length::Fill)
                    .style(crate::view::shell::scroll_style(ctx)),
                container(kit::txt(
                    ctx,
                    format!(
                        "ObjectId({}) · {} · 更新于 2026-10-20 14:22",
                        doc.get("_id").map(|v| v.to_string()).unwrap_or_default(),
                        "1.2 KB"
                    ),
                    ts::MICRO,
                    t.text_tertiary,
                ))
                .padding(10)
                .width(Length::Fill)
                .style(move |_: &Theme| container::Style {
                    border: Border { color: t.hairline, width: 1.0, radius: 0.0.into() },
                    ..Default::default()
                }),
            ]
            .height(Length::Fill)
            .into()
        }
        None => kit::empty_state(ctx, Icon::Doc, "选择一个文档", "从左侧列表选择以查看 JSON", None),
    };

    column![
        header,
        pipeline,
        row![
            container(scrollable(list).height(Length::Fill))
                .width(Length::Fixed(232.0))
                .height(Length::Fill)
                .style(move |_: &Theme| container::Style {
                    border: Border { color: t.hairline, width: 1.0, radius: 0.0.into() },
                    ..Default::default()
                }),
            container(doc_panel).width(Length::Fill).height(Length::Fill),
        ]
        .height(Length::Fill),
    ]
    .height(Length::Fill)
    .into()
}

fn json_color(ctx: Ctx, line: &str) -> Color {
    let t = ctx.t;
    if line.trim_start().starts_with("//") {
        t.text_tertiary
    } else if line.contains("\": \"") {
        t.success
    } else {
        t.text
    }
}

// ---------------------------------------------------------------------------
// Elasticsearch
// ---------------------------------------------------------------------------

fn elastic<'a>(app: &'a App, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;
    let st = &app.es;

    if let Some(err) = &st.error {
        return column![
            error_banner(ctx, err, Msg::OpenEs(st.conn.unwrap_or(0))),
            kit::empty_state(
                ctx,
                Icon::Chart,
                "无法连接 Elasticsearch",
                "请确认集群地址与认证信息；连接成功后可执行 DSL 并查看命中结果。",
                Some(kit::secondary(ctx, "编辑连接", Msg::WizardOpen))
            ),
        ]
        .height(Length::Fill)
        .into();
    }

    let header = container(
        row![
            kit::txt_bold(ctx, "DSL 查询", ts::H3, t.text),
            kit::mono(
                ctx,
                format!("GET {}/_search", st.index.clone().unwrap_or_default()),
                ts::MICRO,
                t.text_secondary
            ),
            kit::hspace(),
            kit::primary(ctx, "执行", Msg::EsRun),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .padding(Padding { top: 8.0, right: 12.0, bottom: 8.0, left: 12.0 }),
    )
    .width(Length::Fill);

    let editor = container(
        text_editor(&app.es_editor)
            .on_action(Msg::EsAction)
            .size(ctx.f(ts::SMALL))
            .font(ctx.mono)
            .padding(10)
            .height(Length::Fixed(120.0)),
    )
    .padding(Padding { top: 0.0, right: 12.0, bottom: 8.0, left: 12.0 });

    let mut results = column![].spacing(0);
    for (i, hit) in st.hits.iter().take(20).enumerate() {
        let score = hit.get("_score").map(|v| v.to_string()).unwrap_or_default();
        let message = hit.get("message").and_then(|v| v.as_str()).unwrap_or("");
        let ts_field = hit.get("@timestamp").and_then(|v| v.as_str()).unwrap_or("");
        results = results.push(
            container(
                row![
                    container(kit::mono(ctx, score, ts::SMALL, t.text_secondary)).width(Length::FillPortion(1)),
                    container(kit::mono(ctx, message.to_string(), ts::SMALL, t.text)).width(Length::FillPortion(5)),
                    container(kit::mono(ctx, ts_field.to_string(), ts::SMALL, t.text_secondary))
                        .width(Length::FillPortion(2)),
                ]
                .padding(Padding { top: 7.0, right: 14.0, bottom: 7.0, left: 14.0 }),
            )
            .width(Length::Fill),
        );
        results = results.push(kit::hline(ctx));
        let _ = i;
    }

    let head = container(
        row![
            kit::txt(ctx, "_score", ts::MICRO, t.text_secondary),
            kit::hspace(),
            kit::txt(ctx, "message", ts::MICRO, t.text_secondary),
            kit::hspace(),
            kit::txt(ctx, "@timestamp", ts::MICRO, t.text_secondary),
        ]
        .padding(Padding { top: 6.0, right: 14.0, bottom: 6.0, left: 14.0 }),
    )
    .width(Length::Fill)
    .style(move |_: &Theme| container::Style {
        background: Some(t.soft.into()),
        ..Default::default()
    });

    let summary = container(
        row![
            kit::txt_bold(ctx, format!("命中 {}", st.total), ts::SMALL, t.text),
            kit::link(ctx, "映射", Msg::Noop),
            kit::link(ctx, "聚合", Msg::Noop),
            kit::hspace(),
            kit::txt(
                ctx,
                format!("耗时 {}ms · 命中 {} 条", st.took, st.total),
                ts::MICRO,
                t.text_tertiary
            ),
        ]
        .spacing(12)
        .align_y(Alignment::Center)
        .padding(Padding { top: 6.0, right: 14.0, bottom: 6.0, left: 14.0 }),
    )
    .width(Length::Fill)
    .style(move |_: &Theme| container::Style {
        border: Border { color: t.hairline, width: 1.0, radius: 0.0.into() },
        ..Default::default()
    });

    if st.hits.is_empty() {
        return column![
            header,
            editor,
            summary,
            kit::empty_state(ctx, Icon::Chart, "尚未执行查询", "点击「执行」以运行 DSL", None),
        ]
        .height(Length::Fill)
        .into();
    }

    column![
        header,
        editor,
        summary,
        head,
        scrollable(results).height(Length::Fill).style(crate::view::shell::scroll_style(ctx)),
    ]
    .height(Length::Fill)
    .into()
}

pub fn _r() -> iced::border::Radius {
    radius::XS.into()
}

pub fn _s() -> f32 {
    space::S
}
