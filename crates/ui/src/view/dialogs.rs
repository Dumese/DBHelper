//! 弹层：新建连接向导（3 步）、导出向导（3 步）、高危确认。

use crate::icons::Icon;
use crate::kit::{self, Ctx};
use crate::state::{Confirm, Modal, Wizard};
use crate::theme::{radius, size as ts, space, Tone};
use crate::view::kind_icon;
use crate::{App, Msg};
use dbhelper_models::{DbKind, SslMode};
use iced::widget::{column, container, row, scrollable, Space};
use iced::{Alignment, Border, Color, Element, Length, Padding, Theme};

pub fn modal<'a>(app: &'a App, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;
    let content: Element<Msg> = match &app.modal {
        Modal::Wizard => wizard(app, ctx),
        Modal::Export => export_wizard(app, ctx),
        Modal::Confirm(c) => confirm(ctx, c),
        _ => Space::new().width(0).height(0).into(),
    };

    container(
        container(content)
            .width(Length::Fixed(if matches!(app.modal, Modal::Export) { 560.0 } else { 620.0 }))
            .style(move |_: &Theme| container::Style {
                background: Some(t.bg.into()),
                border: Border { color: t.hairline, width: 1.0, radius: radius::LG.into() },
                // 注：tiny-skia 后端对大半径阴影绘制异常（会盖住内容），
                // 这里改用描边 + 遮罩表达层级。
                shadow: iced::Shadow::default(),
                ..Default::default()
            }),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .style(move |_: &Theme| container::Style {
        background: Some(t.overlay.into()),
        ..Default::default()
    })
    .into()
}

// ---------------------------------------------------------------------------
// 新建连接向导
// ---------------------------------------------------------------------------

fn wizard<'a>(app: &'a App, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;
    let Some(w) = &app.wizard else { return Space::new().width(0).height(0).into() };

    let header = container(
        column![
            row![
                kit::txt_bold(ctx, "新建连接", ts::H2, t.text),
                kit::hspace(),
                kit::icon_button(ctx, Icon::Close, 13.0, t.text_secondary, Msg::WizardClose),
            ]
            .align_y(Alignment::Center),
            kit::txt(
                ctx,
                format!("第 {} 步 / 共 3 步 · {}", w.step + 1, step_name(w.step)),
                ts::MICRO,
                t.text_tertiary
            ),
        ]
        .spacing(2),
    )
    .padding(Padding { top: 14.0, right: 14.0, bottom: 10.0, left: 16.0 });

    let body: Element<Msg> = match w.step {
        0 => step_kind(w, ctx),
        1 => step_form(w, ctx),
        _ => step_test(w, ctx),
    };

    let footer = container(
        row![
            if w.step > 0 {
                kit::ghost(ctx, "上一步", Msg::WizardStep(-1))
            } else {
                kit::ghost(ctx, "使用高级配置", Msg::Noop)
            },
            kit::hspace(),
            kit::secondary(ctx, "取消", Msg::WizardClose),
            if w.step < 2 {
                kit::primary(ctx, "下一步", Msg::WizardStep(1))
            } else {
                kit::primary(ctx, if w.saving { "保存中…" } else { "保存连接" }, Msg::WizardSave)
            },
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .padding(Padding { top: 10.0, right: 14.0, bottom: 12.0, left: 16.0 }),
    )
    .width(Length::Fill)
    .style(move |_: &Theme| container::Style {
        border: Border { color: t.hairline, width: 1.0, radius: 0.0.into() },
        ..Default::default()
    });

    // 校验错误横幅：所有步骤统一显示（此前只在表单步渲染，第 0 步报错无任何反馈）
    let error_banner: Element<Msg> = match &w.error {
        Some(err) => container(kit::txt(ctx, err.clone(), ts::SMALL, t.error))
            .padding(8)
            .width(Length::Fill)
            .style(move |_: &Theme| container::Style {
                background: Some(t.error_soft.into()),
                border: Border::default().rounded(radius::XS),
                ..Default::default()
            })
            .into(),
        None => Space::new().width(0).height(0).into(),
    };

    column![header, error_banner, body, footer].into()
}

fn step_name(step: i32) -> &'static str {
    match step {
        0 => "选择数据库类型",
        1 => "填写连接信息",
        _ => "测试并保存",
    }
}

fn step_kind<'a>(w: &'a Wizard, ctx: Ctx) -> Element<'a, Msg> {
    let mut grid = column![].spacing(8).padding(Padding { top: 4.0, right: 16.0, bottom: 12.0, left: 16.0 });
    for chunk in DbKind::ALL.chunks(4) {
        let mut r = row![].spacing(8);
        for kind in chunk {
            let active = w.kind == *kind;
            let accent = crate::theme::rgb(
                u32::from_str_radix(kind.accent_hex().trim_start_matches('#'), 16).unwrap_or(0x787774),
            );
            let t = ctx.t;
            r = r.push(
                iced::widget::button(
                    column![
                        kit::icon(kind_icon(*kind), 16.0, accent),
                        kit::txt(ctx, kind.label(), ts::SMALL, t.text),
                    ]
                    .spacing(4)
                    .align_x(Alignment::Center),
                )
                .width(Length::Fixed(140.0))
                .height(Length::Fixed(62.0))
                .padding(8)
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
                        width: 1.0,
                        radius: radius::SM.into(),
                    },
                    ..Default::default()
                })
                .on_press(Msg::WizardKind(*kind)),
            );
        }
        grid = grid.push(r);
    }
    grid.into()
}

fn field<'a>(ctx: Ctx, label: &str, value: &str, on: impl Fn(String) -> Msg + 'a) -> Element<'a, Msg> {
    column![
        kit::txt(ctx, label.to_string(), ts::MICRO, ctx.t.text_secondary),
        kit::input(ctx, "", value, on),
    ]
    .spacing(4)
    .width(Length::Fill)
    .into()
}

fn step_form<'a>(w: &'a Wizard, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;
    let mut fields = column![].spacing(10).padding(Padding { top: 12.0, right: 16.0, bottom: 12.0, left: 16.0 });
    fields = fields.push(field(ctx, "连接名称", &w.name, Msg::WizardName));
    if w.kind.is_file_based() {
        fields = fields.push(
            column![
                kit::txt(ctx, "数据库文件", ts::MICRO, t.text_secondary),
                row![
                    kit::input(ctx, "选择 .db 文件", &w.database, Msg::WizardDatabase),
                    kit::secondary(ctx, "浏览…", Msg::WizardBrowseFile),
                ]
                .spacing(8),
            ]
            .spacing(4),
        );
    } else {
        fields = fields.push(
            row![
                column![
                    kit::txt(ctx, "主机", ts::MICRO, t.text_secondary),
                    kit::input(ctx, "127.0.0.1", &w.host, Msg::WizardHost),
                ]
                .spacing(4)
                .width(Length::FillPortion(3)),
                column![
                    kit::txt(ctx, "端口", ts::MICRO, t.text_secondary),
                    kit::input(ctx, "3306", &w.port, Msg::WizardPort),
                ]
                .spacing(4)
                .width(Length::Fixed(90.0)),
            ]
            .spacing(10),
        );
        fields = fields.push(field(ctx, "数据库", &w.database, Msg::WizardDatabase));
        fields = fields.push(field(ctx, "用户名", &w.username_hint(), Msg::WizardUser));
        fields = fields.push(
            column![
                kit::txt(ctx, "密码（加密存入 Vault）", ts::MICRO, t.text_secondary),
                iced::widget::text_input("••••••••", &w.password)
                    .on_input(Msg::WizardPassword)
                    .secure(true)
                    .size(ctx.f(ts::SMALL))
                    .font(ctx.ui)
                    .padding(Padding { top: 5.0, right: 8.0, bottom: 5.0, left: 8.0 })
                    .style(move |_: &Theme, status| {
                        let focused = matches!(status, iced::widget::text_input::Status::Focused { .. });
                        iced::widget::text_input::Style {
                            background: ctx.t.bg.into(),
                            border: Border {
                                color: if focused { ctx.t.accent } else { ctx.t.hairline_strong },
                                width: 1.0,
                                radius: radius::SM.into(),
                            },
                            icon: ctx.t.text_secondary,
                            placeholder: ctx.t.text_tertiary,
                            value: ctx.t.text,
                            selection: crate::theme::rgba(0x2383E2, 0.25),
                        }
                    }),
            ]
            .spacing(4),
        );
        fields = fields.push(
            row![
                column![
                    kit::txt(ctx, "SSL 模式", ts::MICRO, t.text_secondary),
                    kit::segmented(
                        ctx,
                        &[
                            ("禁用", w.ssl == 0),
                            ("优先", w.ssl == 1),
                            ("必须", w.ssl == 2),
                        ],
                        Msg::WizardSsl
                    ),
                ]
                .spacing(4),
                kit::hspace(),
                column![
                    kit::txt(ctx, "SSH 隧道", ts::MICRO, t.text_secondary),
                    kit::switch(ctx, w.ssh, Msg::WizardSsh),
                ]
                .spacing(4),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        );
        if w.ssh {
            fields = fields.push(field(ctx, "SSH 主机 (user@host:port)", &w.ssh_host, Msg::WizardSshHost));
        }
    }
    fields = fields.push(
        row![
            kit::checkbox(ctx, "只读连接（禁止写操作）", w.readonly, Msg::WizardReadonly),
            kit::hspace(),
            kit::txt(ctx, format!("分组：{}", w.group), ts::MICRO, t.text_tertiary),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    );

    let _ = SslMode::ALL;
    scrollable(fields).height(Length::Fixed(320.0)).into()
}

fn step_test<'a>(w: &'a Wizard, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;
    let result: Element<Msg> = match &w.test_result {
        Some(Ok(msg)) => row![
            kit::icon(Icon::Check, 13.0, t.success),
            kit::txt(ctx, msg.clone(), ts::SMALL, t.success),
        ]
        .spacing(6)
        .into(),
        Some(Err(err)) => row![
            kit::icon(Icon::Close, 13.0, t.error),
            kit::txt(ctx, err.clone(), ts::SMALL, t.error),
        ]
        .spacing(6)
        .into(),
        None => kit::txt(
            ctx,
            "点击「测试连接」验证网络与凭据（仅执行握手与心跳，不写入任何数据）。",
            ts::SMALL,
            t.text_secondary,
        ),
    };

    let summary = [
        ("类型", w.kind.label().to_string()),
        ("名称", w.name.clone()),
        (
            "地址",
            if w.kind.is_file_based() {
                w.database.clone()
            } else {
                format!("{}:{}", w.host, w.port)
            },
        ),
        ("用户名", w.username_hint()),
        ("只读", if w.readonly { "是".into() } else { "否".to_string() }),
    ];

    let mut rows = column![].spacing(0);
    for (k, v) in summary {
        rows = rows.push(kit::setting_row(ctx, k, "", kit::txt(ctx, v, ts::SMALL, t.text)));
    }

    column![
        container(
            column![
                rows,
                Space::new().width(Length::Fixed(0.0)).height(Length::Fixed(10.0)),
                result,
                Space::new().width(Length::Fixed(0.0)).height(Length::Fixed(10.0)),
                kit::secondary(ctx, if w.testing { "测试中…" } else { "测试连接" }, Msg::WizardTest),
            ]
            .spacing(0),
        )
        .padding(Padding { top: 12.0, right: 16.0, bottom: 16.0, left: 16.0 }),
    ]
    .into()
}

// ---------------------------------------------------------------------------
// 导出向导
// ---------------------------------------------------------------------------

fn export_wizard<'a>(app: &'a App, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;
    let st = &app.settings_ui;
    let steps = ["选择内容", "选择格式", "完成"];

    let mut stepper = row![].spacing(8).align_y(Alignment::Center);
    for (i, label) in steps.iter().enumerate() {
        let active = st.export_step >= i as i32;
        stepper = stepper.push(
            container(
                kit::txt(
                    ctx,
                    format!("{}", i + 1),
                    ts::MICRO,
                    if active { Color::WHITE } else { t.text_secondary },
                ),
            )
            .width(Length::Fixed(18.0))
            .height(Length::Fixed(18.0))
            .center_x(Length::Fixed(18.0))
            .center_y(Length::Fixed(18.0))
            .style(move |_: &Theme| container::Style {
                background: Some(if active { t.accent } else { t.hover }.into()),
                border: Border::default().rounded(9.0),
                ..Default::default()
            }),
        );
        stepper = stepper.push(kit::txt(
            ctx,
            label.to_string(),
            ts::SMALL,
            if active { t.text } else { t.text_tertiary },
        ));
        if i + 1 < steps.len() {
            stepper = stepper.push(kit::txt(ctx, "—", ts::MICRO, t.hairline_strong));
        }
    }

    let content: Element<Msg> = match st.export_step {
        0 => column![
            kit::txt(ctx, "选择要导出的内容", ts::SMALL, t.text_secondary),
            Space::new().width(Length::Fixed(0.0)).height(Length::Fixed(8.0)),
            kit::checkbox(ctx, "连接配置（全部已保存的连接与凭据）", st.export_items[0], |v| Msg::ExportToggle(0, v)),
            kit::checkbox(ctx, "查询历史（最近执行的 SQL 记录）", st.export_items[1], |v| Msg::ExportToggle(1, v)),
            kit::checkbox(ctx, "偏好设置（外观、编辑器与语言）", st.export_items[2], |v| Msg::ExportToggle(2, v)),
            kit::checkbox(ctx, "已保存的代码片段（Snippets）", st.export_items[3], |v| Msg::ExportToggle(3, v)),
        ]
        .spacing(8)
        .into(),
        1 => column![
            kit::txt(ctx, "导出格式", ts::SMALL, t.text_secondary),
            Space::new().width(Length::Fixed(0.0)).height(Length::Fixed(8.0)),
            kit::segmented(
                ctx,
                &[("JSON", st.export_format == 0), ("CSV", st.export_format == 1), ("SQL", st.export_format == 2)],
                Msg::ExportFormatPick
            ),
        ]
        .spacing(6)
        .into(),
        _ => column![
            kit::txt(ctx, "准备就绪", ts::H3, t.text),
            kit::txt(
                ctx,
                "点击「导出」选择保存位置，文件将包含所选项。",
                ts::SMALL,
                t.text_secondary
            ),
            match &st.export_result {
                Some(r) => kit::txt(ctx, r.clone(), ts::SMALL, t.success),
                None => Space::new().width(0).height(0).into(),
            },
        ]
        .spacing(8)
        .into(),
    };

    column![
        container(
            column![
                row![
                    kit::txt_bold(ctx, "导出向导", ts::H1, t.text),
                    kit::hspace(),
                    kit::icon_button(ctx, Icon::Close, 13.0, t.text_secondary, Msg::DismissModal),
                ]
                .align_y(Alignment::Center),
                kit::txt(ctx, "将连接配置、查询历史与偏好导出为文件", ts::SMALL, t.text_secondary),
                Space::new().width(Length::Fixed(0.0)).height(Length::Fixed(10.0)),
                stepper,
            ]
            .spacing(4),
        )
        .padding(Padding { top: 16.0, right: 18.0, bottom: 12.0, left: 18.0 }),
        kit::hline(ctx),
        container(content).padding(Padding { top: 14.0, right: 18.0, bottom: 20.0, left: 18.0 }),
        container(
            row![
                kit::ghost(ctx, "取消", Msg::DismissModal),
                kit::hspace(),
                if st.export_step < 2 {
                    kit::primary(ctx, "下一步", Msg::ExportStep(1))
                } else {
                    kit::primary(ctx, "导出", Msg::ExportRun)
                },
            ]
            .spacing(8)
            .padding(Padding { top: 10.0, right: 18.0, bottom: 12.0, left: 18.0 }),
        )
        .style(move |_: &Theme| container::Style {
            border: Border { color: t.hairline, width: 1.0, radius: 0.0.into() },
            ..Default::default()
        }),
    ]
    .into()
}

// ---------------------------------------------------------------------------
// 高危确认
// ---------------------------------------------------------------------------

fn confirm<'a>(ctx: Ctx, c: &'a Confirm) -> Element<'a, Msg> {
    let t = ctx.t;
    column![
        container(
            row![
                kit::icon(Icon::Shield, 16.0, if c.danger { t.warning } else { t.accent }),
                kit::txt_bold(ctx, c.title.clone(), ts::H2, t.text),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .padding(Padding { top: 16.0, right: 18.0, bottom: 8.0, left: 18.0 }),
        ),
        container(
            column![
                kit::txt(ctx, c.message.clone(), ts::BODY, t.text),
                match &c.detail {
                    Some(d) => kit::mono(ctx, d.clone(), ts::MICRO, t.text_secondary),
                    None => Space::new().width(0).height(0).into(),
                },
            ]
            .spacing(8)
            .padding(Padding { top: 0.0, right: 18.0, bottom: 14.0, left: 18.0 }),
        ),
        container(
            row![
                kit::hspace(),
                kit::secondary(ctx, "取消", Msg::CancelConfirm),
                if c.danger {
                    kit::button_kind(ctx, "确认执行", kit::BtnKind::Primary, Msg::ConfirmRun)
                } else {
                    kit::primary(ctx, "确认", Msg::ConfirmRun)
                },
            ]
            .spacing(8)
            .padding(Padding { top: 10.0, right: 18.0, bottom: 14.0, left: 18.0 }),
        )
        .style(move |_: &Theme| container::Style {
            border: Border { color: t.hairline, width: 1.0, radius: 0.0.into() },
            ..Default::default()
        }),
    ]
    .into()
}

pub fn _tone() -> Tone {
    Tone::Neutral
}

pub fn _space() -> f32 {
    space::M
}
