//! 数据表格：列宽计算、表头、行渲染。
//!
//! 行渲染保持「无纵向边框 + 底部 1px 细线」的 Notion 风格；
//! 数值列右对齐，NULL 以浅灰斜体占位。

use crate::kit::{self, Ctx};
use crate::theme::{radius, size as ts, space, Tone};
use dbhelper_models::{ColumnMeta, Row, Value};
use iced::widget::{button, container, row, text};
use iced::{Alignment, Border, Color, Element, Length, Padding, Theme};

pub const ROW_H: f32 = 26.0;
pub const HEADER_H: f32 = 32.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Right,
}

/// 只有「度量列」（数量/金额）右对齐；主键与外键保持左对齐，与原型一致。
pub fn right_aligned(col: &ColumnMeta) -> bool {
    let name = col.name.to_ascii_lowercase();
    if name == "id" || name.ends_with("_id") {
        return false;
    }
    is_numeric(&col.type_name)
}

pub fn is_numeric(type_name: &str) -> bool {
    let t = type_name.to_ascii_lowercase();
    ["int", "decimal", "numeric", "float", "double", "real", "number", "money", "serial"]
        .iter()
        .any(|k| t.contains(k))
}

/// 依据列名/类型分配列宽，返回**像素**宽度，总和等于可用宽度。
///
/// 先按语义权重分配，再对每列施以最小宽度；若最小宽度总和已超出可用宽度，
/// 则等比压缩（此时文本由单元格裁剪）。
pub fn widths(columns: &[ColumnMeta], available: f32) -> Vec<f32> {
    if columns.is_empty() {
        return Vec::new();
    }
    let raw: Vec<f32> = columns
        .iter()
        .map(|c| {
            let name = c.name.to_ascii_lowercase();
            let ty = c.type_name.to_ascii_lowercase();
            if name == "id" || name.ends_with("_id") {
                0.8
            } else if ty.contains("decimal") || ty.contains("numeric") {
                1.3
            } else if name.contains("email") {
                2.2
            } else if name.contains("name") || name.contains("title") {
                1.5
            } else if ty.contains("datetime") || ty.contains("timestamp") || name.contains("_at") {
                1.9
            } else if ty.contains("int") || ty.contains("float") || ty.contains("double") {
                0.9
            } else {
                1.4
            }
        })
        .collect();

    let n = raw.len() as f32;
    let avail = available.max(160.0);
    let sum: f32 = raw.iter().sum();
    let min = 88.0_f32.min(avail / n);
    let base = min * n;
    if base >= avail {
        return vec![avail / n; raw.len()];
    }
    let extra = avail - base;
    let mut out: Vec<f32> = raw.iter().map(|w| min + extra * (w / sum)).collect();
    // 修正浮点误差，保证正好铺满
    let total: f32 = out.iter().sum();
    if (total - avail).abs() > 0.5 {
        if let Some(last) = out.last_mut() {
            *last += avail - total;
        }
    }
    out
}

pub fn header<'a, M: Clone + 'a>(
    ctx: Ctx,
    columns: &[ColumnMeta],
    widths: &[f32],
    sort: Option<&(String, bool)>,
    on_sort: Option<Box<dyn Fn(String) -> M + 'a>>,
) -> Element<'a, M> {
    let mut r = row![].spacing(0).height(Length::Fixed(HEADER_H)).align_y(Alignment::Center);
    for (i, col) in columns.iter().enumerate() {
        let w = iced::Length::Fixed(widths.get(i).copied().unwrap_or(120.0));
        let right = right_aligned(col);
        let sort_mark = sort
            .filter(|(name, _)| name == &col.name)
            .map(|(_, asc)| if *asc { " ↑" } else { " ↓" })
            .unwrap_or("");
        let name_color = if sort.is_some_and(|(n, _)| n == &col.name) { ctx.t.text } else { ctx.t.text_secondary };
        // 原型表头：列名（深色）+ 类型（浅灰小字，小写），左对齐；度量列右对齐
        let inner = row![
            text(format!("{}{}", col.name, sort_mark))
                .size(ctx.f(ts::SMALL))
                .font(ctx.ui_medium())
                .color(name_color),
            text(col.type_name.to_lowercase())
                .size(ctx.f(ts::MICRO))
                .font(ctx.ui)
                .color(ctx.t.text_tertiary),
        ]
        .spacing(5)
        .align_y(Alignment::Center);
        let mut cell = container(inner)
            .width(w)
            .padding(Padding { top: 0.0, right: 10.0, bottom: 0.0, left: 10.0 })
            .align_y(Alignment::Center);
        if right {
            cell = cell.align_x(Alignment::End);
        }
        match &on_sort {
            Some(f) => {
                let m = f(col.name.clone());
                r = r.push(
                    button(cell)
                        .padding(0)
                        .width(w)
                        .style(move |_: &Theme, status| button::Style {
                            background: if matches!(status, button::Status::Hovered) {
                                Some(ctx.t.hover.into())
                            } else {
                                None
                            },
                            text_color: ctx.t.text,
                            border: Border::default(),
                            ..Default::default()
                        })
                        .on_press(m),
                );
            }
            None => r = r.push(cell),
        }
        let _ = i;
    }
    container(r)
        .width(Length::Fill)
        .style(move |_: &Theme| container::Style {
            background: Some(ctx.t.soft.into()),
            border: Border { color: ctx.t.hairline, width: 0.0, radius: 0.0.into() },
            ..Default::default()
        })
        .into()
}

pub fn value_text(ctx: Ctx, value: &Value, type_name: &str) -> (String, Color) {
    match value {
        Value::Null => ("NULL".into(), ctx.t.text_tertiary),
        Value::Decimal(s) => (format_decimal(s, type_name), ctx.t.text),
        Value::Float(f) => {
            if let Some(scale) = scale_of(type_name) {
                (format!("{f:.*}", scale as usize), ctx.t.text)
            } else {
                (value.display(), ctx.t.text)
            }
        }
        _ => {
            let text = value.display();
            let color = match value {
                Value::Bool(_) => ctx.t.accent,
                _ => ctx.t.text,
            };
            (text, color)
        }
    }
}

pub fn format_decimal(s: &str, type_name: &str) -> String {
    match scale_of(type_name) {
        Some(scale) => match s.parse::<f64>() {
            Ok(f) => format!("{f:.*}", scale as usize),
            Err(_) => s.to_string(),
        },
        None => s.to_string(),
    }
}

fn scale_of(type_name: &str) -> Option<u32> {
    let lower = type_name.to_ascii_lowercase();
    if !(lower.contains("decimal") || lower.contains("numeric")) {
        return None;
    }
    let start = lower.find('(')?;
    let end = lower[start..].find(')')? + start;
    lower[start + 1..end].split(',').nth(1).and_then(|s| s.trim().parse().ok())
}

pub fn row_view<'a, M: Clone + 'a>(
    ctx: Ctx,
    columns: &[ColumnMeta],
    widths: &[f32],
    data: &Row,
    selected: bool,
    on_press: Option<M>,
) -> Element<'a, M> {
    let mut r = row![].spacing(0).height(Length::Fixed(ROW_H)).align_y(Alignment::Center);
    for (i, col) in columns.iter().enumerate() {
        let w = iced::Length::Fixed(widths.get(i).copied().unwrap_or(120.0));
        let value = data.get(i).cloned().unwrap_or(Value::Null);
        let (label, color) = value_text(ctx, &value, &col.type_name);
        let right = right_aligned(col);
        let mut cell = container(
            text(label)
                .size(ctx.f(ts::SMALL))
                .font(if right { ctx.mono } else { ctx.ui })
                .color(color)
                .wrapping(iced::widget::text::Wrapping::None),
        )
        .width(w)
        .padding(Padding { top: 0.0, right: 10.0, bottom: 0.0, left: 10.0 })
        .align_y(Alignment::Center)
        .clip(true);
        if right {
            cell = cell.align_x(Alignment::End);
        }
        r = r.push(cell);
    }

    let bg = if selected { ctx.t.selected } else { Color::TRANSPARENT };
    let base = container(r)
        .width(Length::Fill)
        .height(Length::Fixed(ROW_H))
        .style(move |_: &Theme| container::Style {
            background: Some(bg.into()),
            border: Border { color: ctx.t.hairline, width: 0.0, radius: 0.0.into() },
            ..Default::default()
        });

    match on_press {
        Some(msg) => button(base)
            .padding(0)
            .width(Length::Fill)
            .style(move |_: &Theme, status| button::Style {
                background: if matches!(status, button::Status::Hovered) && !selected {
                    Some(ctx.t.hover.into())
                } else {
                    None
                },
                text_color: ctx.t.text,
                border: Border::default(),
                ..Default::default()
            })
            .on_press(msg)
            .into(),
        None => base.into(),
    }
}

/// 状态列的中文映射（库内存英文枚举，界面显示中文徽标）。
pub fn status_badge<'a, M: Clone + 'a>(ctx: Ctx, raw: &str) -> Element<'a, M> {
    let (label, tone) = match raw {
        "paid" => ("已支付", Tone::Success),
        "shipped" => ("已发货", Tone::Accent),
        "pending" => ("待支付", Tone::Warning),
        "refunded" => ("已退款", Tone::Error),
        "active" => ("active", Tone::Success),
        other => (other, Tone::Neutral),
    };
    kit::badge(ctx, label, tone)
}

pub fn row_height() -> f32 {
    ROW_H
}

pub fn _radius() -> iced::border::Radius {
    radius::XS.into()
}

pub fn _space() -> f32 {
    space::S
}
