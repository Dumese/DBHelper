//! 组件套件：按钮、输入框、开关、分段控件、标签、卡片、表格头等。
//!
//! 所有视觉参数来自 [`crate::theme`]，保证 10 个界面使用同一套令牌。

pub use crate::icons::icon;
use crate::icons::Icon;
use crate::i18n::Tr;
use crate::theme::{bold, medium, radius, size as ts, space, Tone, Tokens};
use dbhelper_models::Language;
use iced::widget::{mouse_area, button, checkbox as iced_checkbox, container, row, slider, text, text_input, toggler, Space};
use iced::{
    Alignment, Border, Color, Element, Font, Length, Padding, Shadow, Theme,
};

#[derive(Clone, Copy)]
pub struct Ctx {
    pub t: Tokens,
    /// 当前语言的文案字典；视图用 `ctx.tr.xxx()` 取代硬编码中文字面量。
    pub tr: Tr,
    pub ui: Font,
    pub mono: Font,
    pub scale: f32,
}

impl Ctx {
    pub fn new(t: Tokens, lang: Language, ui_font: &str, mono_font: &str, serif: bool, scale: f32) -> Self {
        Self {
            t,
            tr: Tr::new(lang),
            ui: crate::theme::ui_font(ui_font, serif),
            mono: crate::theme::mono_font(mono_font),
            scale,
        }
    }

    pub fn f(&self, base: f32) -> f32 {
        (base * self.scale).round()
    }

    pub fn ui(&self) -> Font {
        self.ui
    }
    pub fn ui_bold(&self) -> Font {
        bold(self.ui)
    }
    pub fn ui_medium(&self) -> Font {
        medium(self.ui)
    }
}

// ---------------------------------------------------------------------------
// 文本
// ---------------------------------------------------------------------------

pub fn txt<'a, M: Clone + 'a>(ctx: Ctx, s: impl Into<String>, size: f32, color: Color) -> Element<'a, M> {
    text(s.into())
        .size(ctx.f(size))
        .font(ctx.ui)
        .color(color)
        .into()
}

pub fn txt_bold<'a, M: Clone + 'a>(ctx: Ctx, s: impl Into<String>, size: f32, color: Color) -> Element<'a, M> {
    text(s.into())
        .size(ctx.f(size))
        .font(ctx.ui_bold())
        .color(color)
        .into()
}

pub fn mono<'a, M: Clone + 'a>(ctx: Ctx, s: impl Into<String>, size: f32, color: Color) -> Element<'a, M> {
    text(s.into())
        .size(ctx.f(size))
        .font(ctx.mono)
        .color(color)
        .into()
}

/// 大写微标签（原型中的 CONNECTIONS / 分组标题风格）。
pub fn micro<'a, M: Clone + 'a>(ctx: Ctx, s: impl Into<String>, color: Color) -> Element<'a, M> {
    text(s.into().to_uppercase())
        .size(ctx.f(ts::MICRO))
        .font(ctx.ui_medium())
        .color(color)
        .into()
}

// ---------------------------------------------------------------------------
// 容器
// ---------------------------------------------------------------------------

pub fn plain<'a, M: Clone + 'a>(content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    container(content).into()
}

pub fn padded<'a, M: Clone + 'a>(content: impl Into<Element<'a, M>>, all: f32) -> Element<'a, M> {
    container(content).padding(all).into()
}

/// 白底卡片：1px 细边框 + 8px 圆角，无重阴影（原型规范）。
pub fn card<'a, M: Clone + 'a>(ctx: Ctx, content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    container(content)
        .padding(Padding { top: space::L, right: space::L, bottom: space::L, left: space::L })
        .width(Length::Fill)
        .style(move |_: &Theme| container::Style {
            background: Some(ctx.t.bg.into()),
            border: Border { color: ctx.t.hairline, width: 1.0, radius: radius::MD.into() },
            ..Default::default()
        })
        .into()
}

pub fn soft_card<'a, M: Clone + 'a>(ctx: Ctx, content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    container(content)
        .padding(Padding { top: 10.0, right: 12.0, bottom: 10.0, left: 12.0 })
        .width(Length::Fill)
        .style(move |_: &Theme| container::Style {
            background: Some(ctx.t.soft.into()),
            border: Border { color: ctx.t.hairline, width: 1.0, radius: radius::SM.into() },
            ..Default::default()
        })
        .into()
}

pub fn hline<'a, M: Clone + 'a>(ctx: Ctx) -> Element<'a, M> {
    container(Space::new().width(Length::Fill).height(Length::Fixed(1.0)))
        .width(Length::Fill)
        .height(Length::Fixed(1.0))
        .style(move |_: &Theme| container::Style {
            background: Some(ctx.t.hairline.into()),
            ..Default::default()
        })
        .into()
}

/// 固定尺寸间隔（不受父容器对齐影响）。
pub fn gap<'a, M: 'a>(w: f32, h: f32) -> Element<'a, M> {
    container(Space::new())
        .width(Length::Fixed(w))
        .height(Length::Fixed(h))
        .into()
}

/// 水平弹性空白。
pub fn hspace<'a, M: Clone + 'a>() -> Element<'a, M> {
    Space::new().width(Length::Fill).height(Length::Shrink).into()
}

pub fn vline<'a, M: Clone + 'a>(ctx: Ctx, height: f32) -> Element<'a, M> {
    container(Space::new().width(Length::Fixed(1.0)).height(Length::Fixed(height)))
        .width(Length::Fixed(1.0))
        .style(move |_: &Theme| container::Style {
            background: Some(ctx.t.hairline.into()),
            ..Default::default()
        })
        .into()
}

// ---------------------------------------------------------------------------
// 按钮
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BtnKind {
    Primary,
    Secondary,
    Ghost,
    DangerLink,
    Link,
}

fn btn_style(kind: BtnKind, ctx: Ctx, status: button::Status) -> button::Style {
    let t = ctx.t;
    let hovered = matches!(status, button::Status::Hovered);
    let pressed = matches!(status, button::Status::Pressed);
    let disabled = matches!(status, button::Status::Disabled);

    let (bg, fg, border, shadow) = match kind {
        BtnKind::Primary => {
            let bg = if disabled {
                t.hairline_strong
            } else if pressed {
                t.accent_pressed
            } else if hovered {
                t.accent_hover
            } else {
                t.accent
            };
            (bg, Color::WHITE, Color::TRANSPARENT, 1.0)
        }
        BtnKind::Secondary => {
            let bg = if pressed {
                t.selected
            } else if hovered {
                t.hover
            } else {
                t.bg
            };
            (bg, if disabled { t.text_tertiary } else { t.text }, t.hairline_strong, 0.0)
        }
        BtnKind::Ghost => {
            let bg = if pressed {
                t.selected
            } else if hovered {
                t.hover
            } else {
                Color::TRANSPARENT
            };
            (bg, t.text, Color::TRANSPARENT, 0.0)
        }
        BtnKind::Link | BtnKind::DangerLink => {
            let bg = if pressed {
                t.selected
            } else if hovered {
                t.hover
            } else {
                Color::TRANSPARENT
            };
            let fg = match kind {
                BtnKind::DangerLink => t.error,
                _ => t.accent,
            };
            (bg, if disabled { t.text_tertiary } else { fg }, Color::TRANSPARENT, 0.0)
        }
    };

    button::Style {
        background: Some(bg.into()),
        text_color: fg,
        border: Border { color: border, width: if border == Color::TRANSPARENT { 0.0 } else { 1.0 }, radius: radius::SM.into() },
        shadow: Shadow { color: Color { a: 0.05 * shadow, ..Color::BLACK }, offset: iced::Vector::new(0.0, 1.0), blur_radius: 2.0 },
        snap: true,
    }
}

pub fn button_kind<'a, M: Clone + 'a>(
    ctx: Ctx,
    label: impl Into<String>,
    kind: BtnKind,
    msg: M,
) -> Element<'a, M> {
    let f = ctx.f(ts::BODY);
    let (pt, pb, pl, pr) = match kind {
        BtnKind::Primary => (6.0, 6.0, 14.0, 14.0),
        BtnKind::Secondary => (5.0, 5.0, 12.0, 12.0),
        _ => (4.0, 4.0, 8.0, 8.0),
    };
    button(
        text(label.into())
            .size(f)
            .font(ctx.ui_medium())
            .color(match kind {
                BtnKind::Primary => Color::WHITE,
                BtnKind::DangerLink => ctx.t.error,
                BtnKind::Link => ctx.t.accent,
                _ => ctx.t.text,
            }),
    )
    .padding(Padding { top: pt, right: pr, bottom: pb, left: pl })
    .style(move |_: &Theme, status| btn_style(kind, ctx, status))
    .on_press(msg)
    .into()
}

pub fn primary<'a, M: Clone + 'a>(ctx: Ctx, label: impl Into<String>, msg: M) -> Element<'a, M> {
    button_kind(ctx, label, BtnKind::Primary, msg)
}

pub fn secondary<'a, M: Clone + 'a>(ctx: Ctx, label: impl Into<String>, msg: M) -> Element<'a, M> {
    button_kind(ctx, label, BtnKind::Secondary, msg)
}

pub fn ghost<'a, M: Clone + 'a>(ctx: Ctx, label: impl Into<String>, msg: M) -> Element<'a, M> {
    button_kind(ctx, label, BtnKind::Ghost, msg)
}

pub fn link<'a, M: Clone + 'a>(ctx: Ctx, label: impl Into<String>, msg: M) -> Element<'a, M> {
    button_kind(ctx, label, BtnKind::Link, msg)
}

pub fn danger_link<'a, M: Clone + 'a>(ctx: Ctx, label: impl Into<String>, msg: M) -> Element<'a, M> {
    button_kind(ctx, label, BtnKind::DangerLink, msg)
}

/// 纯图标按钮（工具条 / 标题栏）。
pub fn icon_button<'a, M: Clone + 'a>(
    ctx: Ctx,
    ic: Icon,
    size: f32,
    color: Color,
    msg: M,
) -> Element<'a, M> {
    button(icon(ic, ctx.f(size), color))
        .padding(Padding { top: 4.0, right: 4.0, bottom: 4.0, left: 4.0 })
        .style(move |_: &Theme, status| {
            let bg = match status {
                button::Status::Hovered => ctx.t.hover,
                button::Status::Pressed => ctx.t.selected,
                _ => Color::TRANSPARENT,
            };
            button::Style {
                background: Some(bg.into()),
                text_color: ctx.t.text,
                border: Border::default().rounded(radius::XS),
                ..Default::default()
            }
        })
        .on_press(msg)
        .into()
}

/// 带图标 + 文字的主按钮（执行 / 下一步）。
/// 主按钮 + 右侧快捷键提示（执行 F5）。
pub fn primary_with_hint<'a, M: Clone + 'a>(
    ctx: Ctx,
    ic: Icon,
    label: impl Into<String>,
    hint: &str,
    msg: M,
) -> Element<'a, M> {
    button(
        row![
            icon(ic, ctx.f(11.0), Color::WHITE),
            text(label.into())
                .size(ctx.f(ts::SMALL))
                .font(ctx.ui_medium())
                .color(Color::WHITE),
            txt(ctx, hint.to_string(), ts::MICRO, Color { a: 0.72, ..Color::WHITE }),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    )
    .padding(Padding { top: 5.0, right: 10.0, bottom: 5.0, left: 10.0 })
    .height(Length::Fixed(28.0))
    .style(move |_: &Theme, status| btn_style(BtnKind::Primary, ctx, status))
    .on_press(msg)
    .into()
}

pub fn primary_icon<'a, M: Clone + 'a>(
    ctx: Ctx,
    ic: Icon,
    label: impl Into<String>,
    msg: M,
) -> Element<'a, M> {
    let f = ctx.f(ts::BODY);
    button(
        row![icon(ic, ctx.f(12.0), Color::WHITE), txt(ctx, label, ts::BODY, Color::WHITE)]
            .spacing(6)
            .align_y(Alignment::Center),
    )
    .padding(Padding { top: 6.0, right: 14.0, bottom: 6.0, left: 12.0 })
    .style(move |_: &Theme, status| btn_style(BtnKind::Primary, ctx, status))
    .on_press(msg)
    .into()
}

// ---------------------------------------------------------------------------
// 输入
// ---------------------------------------------------------------------------

fn input_style(ctx: Ctx, status: text_input::Status) -> text_input::Style {
    let t = ctx.t;
    let focused = matches!(status, text_input::Status::Focused { .. });
    text_input::Style {
        background: t.bg.into(),
        border: Border {
            color: if focused { t.accent } else { t.hairline_strong },
            width: 1.0,
            radius: radius::SM.into(),
        },
        icon: t.text_secondary,
        placeholder: t.text_tertiary,
        value: t.text,
        selection: crate::theme::rgba(0x2383E2, 0.25),
    }
}

pub fn input<'a, M: Clone + 'a>(
    ctx: Ctx,
    placeholder: &str,
    value: &str,
    on_input: impl Fn(String) -> M + 'a,
) -> Element<'a, M> {
    text_input(placeholder, value)
        .on_input(on_input)
        .size(ctx.f(ts::BODY))
        .font(ctx.ui)
        .padding(Padding { top: 5.0, right: 8.0, bottom: 5.0, left: 8.0 })
        .style(move |_: &Theme, status| input_style(ctx, status))
        .into()
}

pub fn input_mono<'a, M: Clone + 'a>(
    ctx: Ctx,
    placeholder: &str,
    value: &str,
    on_input: impl Fn(String) -> M + 'a,
) -> Element<'a, M> {
    text_input(placeholder, value)
        .on_input(on_input)
        .size(ctx.f(ts::SMALL))
        .font(ctx.mono)
        .padding(Padding { top: 5.0, right: 8.0, bottom: 5.0, left: 8.0 })
        .style(move |_: &Theme, status| input_style(ctx, status))
        .into()
}

/// 下拉框（关闭态）：白底、1px 描边、6px 圆角。
pub fn select_style<'a>(
    ctx: Ctx,
) -> impl Fn(&Theme, iced::widget::pick_list::Status) -> iced::widget::pick_list::Style + 'a {
    move |_, status| {
        use iced::widget::pick_list;
        let border_color = match status {
            pick_list::Status::Hovered | pick_list::Status::Opened { .. } => ctx.t.hairline_strong,
            _ => ctx.t.hairline,
        };
        pick_list::Style {
            text_color: ctx.t.text,
            placeholder_color: ctx.t.text_tertiary,
            handle_color: ctx.t.text_secondary,
            background: ctx.t.bg.into(),
            border: Border { color: border_color, width: 1.0, radius: radius::SM.into() },
        }
    }
}

/// 下拉框展开层：白底、圆角 6、无阴影（tiny-skia 大阴影有渲染缺陷）。
pub fn select_menu_style<'a>(
    ctx: Ctx,
) -> impl Fn(&Theme) -> iced::overlay::menu::Style + 'a {
    move |_| iced::overlay::menu::Style {
        text_color: ctx.t.text,
        background: ctx.t.bg.into(),
        border: Border { color: ctx.t.hairline_strong, width: 1.0, radius: radius::SM.into() },
        selected_text_color: ctx.t.text,
        selected_background: ctx.t.hover.into(),
        shadow: Shadow::default(),
    }
}

/// 侧栏搜索框：灰底、无边框、左侧放大镜。
pub fn search_field<'a, M: Clone + 'a>(
    ctx: Ctx,
    placeholder: &str,
    value: &str,
    on_input: impl Fn(String) -> M + 'a,
) -> Element<'a, M> {
    container(
        row![
            icon(Icon::Search, ctx.f(13.0), ctx.t.text_tertiary),
            text_input(placeholder, value)
                .on_input(on_input)
                .size(ctx.f(ts::SMALL))
                .font(ctx.ui)
                .padding(Padding { top: 0.0, right: 0.0, bottom: 0.0, left: 0.0 })
                .style(move |_: &Theme, _status| text_input::Style {
                    background: Color::TRANSPARENT.into(),
                    border: Border::default(),
                    icon: ctx.t.text_secondary,
                    placeholder: ctx.t.text_tertiary,
                    value: ctx.t.text,
                    selection: crate::theme::rgba(0x2383E2, 0.25),
                })
                .width(Length::Fill),
        ]
        .spacing(5)
        .align_y(Alignment::Center),
    )
    .padding(Padding { top: 4.0, right: 8.0, bottom: 4.0, left: 8.0 })
    .width(Length::Fill)
    .height(Length::Fixed(28.0))
    .style(move |_: &Theme| container::Style {
        background: Some(ctx.t.hover.into()),
        border: Border::default().rounded(radius::SM),
        ..Default::default()
    })
    .into()
}

// ---------------------------------------------------------------------------
// 交互控件
// ---------------------------------------------------------------------------

pub fn switch<'a, M: Clone + 'a>(
    ctx: Ctx,
    on: bool,
    on_toggle: impl Fn(bool) -> M + 'a,
) -> Element<'a, M> {
    toggler(on)
        .on_toggle(on_toggle)
        .size(ctx.f(18.0))
        .style(move |_: &Theme, _status| toggler::Style {
            background: if on { ctx.t.accent } else { ctx.t.hairline_strong }.into(),
            background_border_width: 0.0,
            background_border_color: Color::TRANSPARENT,
            foreground: if on { Color::WHITE } else { ctx.t.bg }.into(),
            foreground_border_width: 0.0,
            foreground_border_color: Color::TRANSPARENT,
            border_radius: Some(10.0.into()),
            padding_ratio: 0.16,
            text_color: None,
        })
        .into()
}

pub fn checkbox<'a, M: Clone + 'a>(
    ctx: Ctx,
    label: impl Into<String>,
    checked: bool,
    on_toggle: impl Fn(bool) -> M + 'a,
) -> Element<'a, M> {
    iced_checkbox(checked)
        .label(label.into())
        .on_toggle(on_toggle)
        .size(ctx.f(16.0))
        .text_size(ctx.f(ts::BODY))
        .font(ctx.ui)
        .style(move |_: &Theme, status| iced_checkbox::Style {
            background: if checked { ctx.t.accent } else { ctx.t.bg }.into(),
            border: Border {
                color: if checked { ctx.t.accent } else { ctx.t.hairline_strong },
                width: 1.0,
                radius: radius::XS.into(),
            },
            text_color: Some(if matches!(status, iced_checkbox::Status::Disabled { .. }) {
                ctx.t.text_tertiary
            } else {
                ctx.t.text
            }),
            icon_color: Color::WHITE,
        })
        .into()
}

/// 分段控件（浅色底槽 + 选中项白底）。
pub fn segmented<'a, M: Clone + 'a>(
    ctx: Ctx,
    options: &[(&str, bool)],
    on_pick: impl Fn(usize) -> M + 'a,
) -> Element<'a, M> {
    let mut row = iced::widget::Row::new().spacing(2).align_y(Alignment::Center);
    for (i, (label, active)) in options.iter().enumerate() {
        let active = *active;
        let msg = on_pick(i);
        // 用 mouse_area 而不是 button：分段控件的按钮在部分布局下命中区会偏移，
        // mouse_area 直接以容器矩形作为热区，稳定可靠。
        let chip = container(
            text((*label).to_string())
                .size(ctx.f(ts::SMALL))
                .font(if active { ctx.ui_medium() } else { ctx.ui })
                .color(if active { ctx.t.text } else { ctx.t.text_secondary }),
        )
        .padding(Padding { top: 3.0, right: 10.0, bottom: 3.0, left: 10.0 })
        .style(move |_: &Theme| container::Style {
            background: Some(if active { ctx.t.bg } else { Color::TRANSPARENT }.into()),
            border: Border {
                color: if active { ctx.t.hairline_strong } else { Color::TRANSPARENT },
                width: if active { 1.0 } else { 0.0 },
                radius: radius::XS.into(),
            },
            ..Default::default()
        });
        row = row.push(mouse_area(chip).on_press(msg));
    }
    container(row)
        .padding(2)
        .style(move |_: &Theme| container::Style {
            background: Some(ctx.t.hover.into()),
            border: Border::default().rounded(radius::SM),
            ..Default::default()
        })
        .into()
}

/// 状态徽标（已支付 / 待支付 …）。
pub fn badge<'a, M: Clone + 'a>(ctx: Ctx, label: impl Into<String>, tone: Tone) -> Element<'a, M> {
    container(
        text(label.into())
            .size(ctx.f(ts::MICRO))
            .font(ctx.ui_medium())
            .color(ctx.t.tone(tone)),
    )
    .padding(Padding { top: 1.0, right: 6.0, bottom: 1.0, left: 6.0 })
    .style(move |_: &Theme| container::Style {
        background: Some(ctx.t.tone_soft(tone).into()),
        border: Border::default().rounded(3.0),
        ..Default::default()
    })
    .into()
}

/// 过滤条件芯片：`status = paid ×`
pub fn filter_chip<'a, M: Clone + 'a>(
    ctx: Ctx,
    label: impl Into<String>,
    msg: M,
) -> Element<'a, M> {
    button(
        row![
            txt(ctx, label, ts::MICRO, ctx.t.text),
            icon(Icon::Close, ctx.f(9.0), ctx.t.text_secondary)
        ]
        .spacing(5)
        .align_y(Alignment::Center),
    )
    .padding(Padding { top: 2.0, right: 6.0, bottom: 2.0, left: 7.0 })
    .style(move |_: &Theme, status| {
        let bg = match status {
            button::Status::Hovered => ctx.t.selected,
            _ => ctx.t.hover,
        };
        button::Style {
            background: Some(bg.into()),
            text_color: ctx.t.text,
            border: Border { color: ctx.t.hairline, width: 1.0, radius: radius::XS.into() },
            shadow: Shadow::default(),
            snap: true,
        }
    })
    .on_press(msg)
    .into()
}

/// 计数页签（Redis 五大数据类型 / 结果集页签）。
pub fn counter_tab<'a, M: Clone + 'a>(
    ctx: Ctx,
    label: &str,
    count: &str,
    active: bool,
    msg: M,
) -> Element<'a, M> {
    button(
        row![
            txt(
                ctx,
                label.to_string(),
                ts::SMALL,
                if active { ctx.t.text } else { ctx.t.text_secondary }
            ),
            txt(
                ctx,
                count.to_string(),
                ts::SMALL,
                if active { ctx.t.text_secondary } else { ctx.t.text_tertiary }
            ),
        ]
        .spacing(5)
        .align_y(Alignment::Center),
    )
    .padding(Padding { top: 7.0, right: 10.0, bottom: 7.0, left: 10.0 })
    .style(move |_: &Theme, status| {
        let bg = if active {
            ctx.t.bg
        } else {
            match status {
                button::Status::Hovered => ctx.t.hover,
                _ => Color::TRANSPARENT,
            }
        };
        button::Style {
            background: Some(bg.into()),
            text_color: ctx.t.text,
            border: Border { color: if active { ctx.t.hairline_strong } else { Color::TRANSPARENT }, width: 1.0, radius: radius::XS.into() },
            shadow: Shadow::default(),
            snap: true,
        }
    })
    .on_press(msg)
    .into()
}

/// 底部蓝色下划线的页签（Redis 元信息 / 命令历史 / 编辑）。
pub fn underline_tab<'a, M: Clone + 'a>(
    ctx: Ctx,
    label: &str,
    active: bool,
    msg: M,
) -> Element<'a, M> {
    let mut col = iced::widget::Column::new().spacing(6).align_x(Alignment::Center);
    col = col.push(
        button(
            text(label.to_string())
                .size(ctx.f(ts::SMALL))
                .font(if active { ctx.ui_medium() } else { ctx.ui })
                .color(if active { ctx.t.accent } else { ctx.t.text_secondary }),
        )
        .padding(Padding { top: 2.0, right: 2.0, bottom: 2.0, left: 2.0 })
        .style(move |_: &Theme, status| button::Style {
            background: Some(
                if matches!(status, button::Status::Hovered) { ctx.t.hover } else { Color::TRANSPARENT }
                    .into(),
            ),
            text_color: ctx.t.text,
            border: Border::default().rounded(radius::XS),
            shadow: Shadow::default(),
            snap: true,
        })
        .on_press(msg),
    );
    col = col.push(
        container(Space::new().width(Length::Fill).height(Length::Fixed(2.0)))
            .width(Length::Fill)
            .style(move |_: &Theme| container::Style {
                background: Some(if active { ctx.t.accent } else { Color::TRANSPARENT }.into()),
                ..Default::default()
            }),
    );
    col.into()
}

pub fn slider_row<'a, M: Clone + 'a>(
    ctx: Ctx,
    value: f32,
    range: std::ops::RangeInclusive<f32>,
    on_change: impl Fn(f32) -> M + 'a,
) -> Element<'a, M> {
    slider(range, value, on_change)
        .step(1.0)
        .style(move |_: &Theme, status| slider::Style {
            rail: iced::widget::slider::Rail {
                backgrounds: (
                    if matches!(status, slider::Status::Dragged | slider::Status::Hovered) {
                        ctx.t.accent_hover
                    } else {
                        ctx.t.accent
                    }
                    .into(),
                    ctx.t.hairline.into(),
                ),
                width: 4.0,
                border: Border::default().rounded(2.0),
            },
            handle: iced::widget::slider::Handle {
                shape: iced::widget::slider::HandleShape::Circle { radius: 6.0 },
                background: ctx.t.bg.into(),
                border_width: 2.0,
                border_color: ctx.t.accent,
            },
        })
        .into()
}

/// 空状态占位（错误 / 无数据 / 未连接）。
pub fn empty_state<'a, M: Clone + 'a>(
    ctx: Ctx,
    ic: Icon,
    title: &str,
    desc: &str,
    action: Option<Element<'a, M>>,
) -> Element<'a, M> {
    let mut col = iced::widget::Column::new()
        .spacing(space::S)
        .align_x(Alignment::Center)
        .width(Length::Fill);
    col = col.push(icon(ic, 28.0, ctx.t.text_tertiary));
    col = col.push(txt(ctx, title.to_string(), ts::H3, ctx.t.text));
    col = col.push(
        text(desc.to_string())
            .size(ctx.f(ts::SMALL))
            .font(ctx.ui)
            .color(ctx.t.text_secondary)
            .align_x(Alignment::Center),
    );
    if let Some(action) = action {
        col = col.push(Space::new().width(Length::Fill).height(Length::Fixed(4.0)));
        col = col.push(action);
    }
    container(col)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .padding(24)
        .into()
}

/// 键值行（设置页：左标题+说明，右控件）。
pub fn setting_row<'a, M: Clone + 'a>(
    ctx: Ctx,
    title: &str,
    desc: &str,
    control: impl Into<Element<'a, M>>,
) -> Element<'a, M> {
    let control: Element<'a, M> = control.into();
    container(
        row![
            iced::widget::column![
                txt(ctx, title.to_string(), ts::BODY, ctx.t.text),
                txt(ctx, desc.to_string(), ts::MICRO, ctx.t.text_secondary),
            ]
            .spacing(2)
            .width(Length::Fill),
            control
        ]
        .align_y(Alignment::Center)
        .spacing(space::L),
    )
    .padding(Padding { top: 10.0, right: 0.0, bottom: 10.0, left: 0.0 })
    .width(Length::Fill)
    .into()
}


/// 侧栏计数：超过 1000 用 k/M 缩写。
pub fn count_label(n: usize) -> String {
    if n == 0 {
        String::new()
    } else if n < 10_000 {
        n.to_string()
    } else {
        dbhelper_utils::textfmt::compact_count(n as u64)
    }
}
