//! 弹层：新建连接向导（3 步）、导出向导（3 步）、高危确认。

use crate::icons::Icon;
use crate::kit::{self, Ctx};
use crate::state::{Confirm, Modal, Wizard};
use crate::theme::{control, radius, size as ts, space, Tone};
use crate::view::kind_icon;
use crate::{App, Msg};
use dbhelper_models::{DbKind, SslMode};
use iced::widget::{column, container, row, scrollable, stack, Space};
use iced::{Alignment, Border, Color, Element, Length, Padding, Theme};

pub fn modal<'a>(app: &'a App, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;
    let content: Element<Msg> = match &app.modal {
        Modal::Wizard => wizard(app, ctx),
        Modal::Export => export_wizard(app, ctx),
        Modal::Import => crate::view::features::import_panel(app, ctx),
        Modal::Confirm(c) => confirm(ctx, c),
        Modal::About => about(app, ctx),
        Modal::Shortcuts => shortcuts(ctx),
        Modal::History => history(app, ctx),
        _ => Space::new().width(0).height(0).into(),
    };

    // 遮罩 + 居中弹窗。
    //
    // 用 `stack` 而不是 `container(center_x/center_y)`：后者在 Fill 容器里
    // 会把 Shrink 内容的对齐交给 flex 解析，导致内容被拉伸到边角
    // （表现为标题跑到弹窗右下角）。stack 叠放更可控。
    let panel = container(content)
        .width(Length::Fixed(if matches!(app.modal, Modal::Export) { 560.0 } else { 620.0 }))
        .style(move |_: &Theme| container::Style {
            background: Some(t.bg.into()),
            border: Border { color: t.hairline, width: 1.0, radius: radius::LG.into() },
            // 注：tiny-skia 后端对大半径阴影绘制异常（会盖住内容），
            // 这里改用描边 + 遮罩表达层级。
            shadow: iced::Shadow::default(),
            ..Default::default()
        });

    // 遮罩层：Fill 尺寸 + 半透明底 + 居中放置弹窗面板。
    //
    // 关键：面板本身是 Shrink 尺寸（宽度固定、高度自适应），
    // 外层用 `center_x/center_y` 把它推到正中。
    container(panel)
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

/// 弹窗统一头部：标题 + 副标题 + 右上角关闭按钮。
///
/// 此前只有「新建连接」「导出向导」两个弹窗有关闭按钮，
/// 确认框、导入面板完全没有 X，用户无法用鼠标关闭；这里统一为所有弹窗
/// 提供同一套头部，保证右上角永远有可点的关闭按钮。
fn modal_header<'a>(
    ctx: Ctx,
    title: impl Into<String>,
    subtitle: Option<String>,
    close_msg: Msg,
) -> Element<'a, Msg> {
    let t = ctx.t;
    // 头部结构：左侧标题（纵向：主标题 + 副标题），右侧关闭按钮。
    // 用 container 明确指定高度，避免 Shrink/Fill 混排导致布局被拉开。
    let mut titles = column![].spacing(2);
    titles = titles.push(kit::txt_bold(ctx, title, ts::H2, t.text));
    if let Some(sub) = subtitle {
        titles = titles.push(kit::txt(ctx, sub, ts::SMALL, t.text_secondary));
    }

    let head = row![
        container(titles)
            .height(Length::Shrink)
            .width(Length::Fill)
            .align_y(Alignment::Center),
        // 关闭按钮：28×28 点击区（Notion 规范），16px 图标
        kit::icon_button(ctx, Icon::Close, control::ICON, t.text_secondary, close_msg),
    ]
    .align_y(Alignment::Center)
    .spacing(8);

    container(head)
        .padding(Padding { top: 16.0, right: 16.0, bottom: 12.0, left: 20.0 })
        .width(Length::Fill)
        .height(Length::Shrink)
        .into()
}

/// 弹窗底部按钮条（取消 / 确认）。
fn modal_footer<'a, M: Clone + 'a>(
    ctx: Ctx,
    left: Option<Element<'a, M>>,
    right: Vec<Element<'a, M>>,
) -> Element<'a, M> {
    let t = ctx.t;
    let mut r = row![].spacing(8).align_y(Alignment::Center);
    for b in right {
        r = r.push(b);
    }
    let mut row = match left {
        Some(l) => row![l, kit::hspace(), r],
        None => row![kit::hspace(), r],
    };
    row = row.align_y(Alignment::Center);

    container(row)
        .padding(Padding { top: 12.0, right: 20.0, bottom: 16.0, left: 20.0 })
        .width(Length::Fill)
        .style(move |_: &Theme| container::Style {
            border: Border { color: t.hairline, width: 1.0, radius: 0.0.into() },
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

    let header = modal_header(
        ctx,
        "新建连接",
        Some(format!("第 {} 步 / 共 3 步 · {}", w.step + 1, step_name(w.step))),
        Msg::WizardClose,
    );

    let body: Element<Msg> = match w.step {
        0 => step_kind(w, ctx),
        1 => step_form(w, ctx),
        _ => step_test(w, ctx),
    };

    let footer = modal_footer(
        ctx,
        Some(if w.step > 0 {
            kit::ghost(ctx, "上一步", Msg::WizardStep(-1))
        } else {
            kit::gap(1.0, 1.0)
        }),
        vec![
            kit::secondary(ctx, "取消", Msg::WizardClose),
            if w.step < 2 {
                kit::primary(ctx, "下一步", Msg::WizardStep(1))
            } else {
                kit::primary(ctx, if w.saving { "保存中…" } else { "保存连接" }, Msg::WizardSave)
            },
        ],
    );

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

    // header 固定顶部、body 自然高度、footer 固定底部。
    //
    // 关键：body **不能**放进 `height(Fill)` 的容器——body 本身是 Shrink 高度，
    // 在 Fill 容器里会被垂直居中（表现为标题/内容跑到弹窗中间、卡片沉底）。
    // column 默认就是顶部对齐排列。
    // 注意：不要给 column 设 height(Fill) —— Shrink 子元素在 Fill 列里会被拉开分布，
    // 表现为标题跑到弹窗中间、内容沉到底部。
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
    scrollable(fields).height(Length::Fill).into()
}

fn step_test<'a>(w: &'a Wizard, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;
    let result: Element<Msg> = match &w.test_result {
        Some(Ok(msg)) => row![
            kit::icon(Icon::Check, control::ICON_SM, t.success),
            kit::txt(ctx, msg.clone(), ts::SMALL, t.success),
        ]
        .spacing(6)
        .into(),
        Some(Err(err)) => row![
            kit::icon(Icon::Close, control::ICON_SM, t.error),
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
            stepper,
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
            stepper,
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
            stepper,
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
        modal_header(
            ctx,
            "导出向导",
            Some("将连接配置、查询历史与偏好导出为文件".into()),
            Msg::DismissModal,
        ),
        kit::hline(ctx),
        container(
            column![content]
                .spacing(0)
                .padding(Padding { top: 16.0, right: 20.0, bottom: 20.0, left: 20.0 }),
        )
        .width(Length::Fill),
        modal_footer(
            ctx,
            Some(kit::ghost(ctx, "取消", Msg::DismissModal)),
            vec![if st.export_step < 2 {
                kit::primary(ctx, "下一步", Msg::ExportStep(1))
            } else {
                kit::primary(ctx, "导出", Msg::ExportRun)
            }],
        ),
    ]
    .into()
}

// ---------------------------------------------------------------------------
// 高危确认
// ---------------------------------------------------------------------------

fn confirm<'a>(ctx: Ctx, c: &'a Confirm) -> Element<'a, Msg> {
    let t = ctx.t;
    column![
        // 确认框此前没有关闭按钮，只能靠「取消」键关闭；现在右上角补 X。
        container(
            row![
                kit::icon(Icon::Shield, control::ICON, if c.danger { t.warning } else { t.accent }),
                kit::txt_bold(ctx, c.title.clone(), ts::H2, t.text),
                kit::hspace(),
                kit::icon_button(ctx, Icon::Close, control::ICON, t.text_secondary, Msg::CancelConfirm),
            ]
            .spacing(10)
            .align_y(Alignment::Center)
            .padding(Padding { top: 16.0, right: 16.0, bottom: 8.0, left: 20.0 }),
        )
        .width(Length::Fill),
        container(
            column![
                kit::txt(ctx, c.message.clone(), ts::BODY, t.text),
                match &c.detail {
                    Some(d) => kit::mono(ctx, d.clone(), ts::MICRO, t.text_secondary),
                    None => Space::new().width(0).height(0).into(),
                },
            ]
            .spacing(8)
            .padding(Padding { top: 0.0, right: 20.0, bottom: 16.0, left: 20.0 }),
        )
        .width(Length::Fill),
        modal_footer(
            ctx,
            None,
            vec![
                kit::secondary(ctx, "取消", Msg::CancelConfirm),
                if c.danger {
                    kit::button_kind(ctx, "确认执行", kit::BtnKind::Primary, Msg::ConfirmRun)
                } else {
                    kit::primary(ctx, "确认", Msg::ConfirmRun)
                },
            ],
        ),
    ]
    .into()
}

// ---------------------------------------------------------------------------
// 关于 / 快捷键 / 查询历史
// ---------------------------------------------------------------------------

/// 「关于」弹窗。
fn about<'a>(app: &'a App, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;
    let _ = app;
    let mut rows = column![].spacing(10);
    for (k, v) in [
        ("产品", "DBHelper"),
        ("版本", "v2.4.1"),
        ("定位", "多数据库管理工具（对标 Navicat / DBeaver）"),
        ("支持", "MySQL / PostgreSQL / SQLite / SQL Server / Oracle / Redis / MongoDB / Elasticsearch"),
        ("凭据", "Vault 加密存储（AES-256-GCM + Windows Credential Manager）"),
    ] {
        rows = rows.push(
            row![
                kit::txt(ctx, k.to_string(), ts::SMALL, t.text_secondary),
                kit::txt(ctx, v.to_string(), ts::SMALL, t.text),
            ]
            .spacing(16)
            .align_y(Alignment::Start),
        );
    }

    column![
        modal_header(ctx, "关于 DBHelper", Some("多数据库管理工具".into()), Msg::DismissModal),
        container(
            column![
                row![
                    crate::icons::fox_logo(56.0),
                    column![
                        kit::txt_bold(ctx, "DBHelper", ts::H1, t.text),
                        kit::txt(ctx, "v2.4.1", ts::SMALL, t.text_secondary),
                    ]
                    .spacing(2),
                ]
                .spacing(16)
                .align_y(Alignment::Center),
                Space::new().width(Length::Fixed(0.0)).height(Length::Fixed(8.0)),
                rows,
            ]
            .spacing(0)
            .padding(Padding { top: 16.0, right: 20.0, bottom: 20.0, left: 20.0 }),
        )
        .width(Length::Fill),
        modal_footer(ctx, None, vec![kit::primary(ctx, "关闭", Msg::DismissModal)]),
    ]
    .into()
}

/// 「快捷键」弹窗。
fn shortcuts<'a>(ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;
    let items: &[(&str, &str)] = &[
        ("F5", "执行当前查询"),
        ("Ctrl+Space", "SQL 补全"),
        ("Ctrl+F", "搜索"),
        ("Ctrl+S", "保存"),
        ("Ctrl+Z / Ctrl+Y", "撤销 / 重做"),
        ("Ctrl+C / Ctrl+V / Ctrl+X", "复制 / 粘贴 / 剪切"),
        ("F11", "最大化 / 还原"),
        ("Esc", "关闭弹窗 / 取消补全"),
    ];
    let mut rows = column![].spacing(0);
    for (i, (k, v)) in items.iter().enumerate() {
        rows = rows.push(
            container(
                row![
                    kit::mono(ctx, (*k).to_string(), ts::MICRO, t.text),
                    kit::txt(ctx, (*v).to_string(), ts::SMALL, t.text_secondary),
                    kit::hspace(),
                ]
                .spacing(16)
                .align_y(Alignment::Center),
            )
            .padding(Padding { top: 8.0, right: 0.0, bottom: 8.0, left: 0.0 })
            .width(Length::Fill)
            .style(move |_: &Theme| container::Style {
                background: if i % 2 == 0 { Some(t.soft.into()) } else { None },
                ..Default::default()
            }),
        );
    }

    column![
        modal_header(ctx, "快捷键", Some("编辑器与窗口".into()), Msg::DismissModal),
        container(rows).padding(Padding { top: 12.0, right: 20.0, bottom: 16.0, left: 20.0 }),
        modal_footer(ctx, None, vec![kit::primary(ctx, "关闭", Msg::DismissModal)]),
    ]
    .into()
}

/// 「查询历史」弹窗。
fn history<'a>(app: &'a App, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;
    let mut rows = column![].spacing(0);
    if app.history_ui.loading {
        rows = rows.push(kit::txt(ctx, "正在加载历史记录…", ts::SMALL, t.text_secondary));
    } else if let Some(err) = &app.history_ui.error {
        rows = rows.push(kit::txt(ctx, err.clone(), ts::SMALL, t.error));
    } else if app.history_ui.entries.is_empty() {
        rows = rows.push(kit::txt(ctx, "暂无查询历史记录。", ts::SMALL, t.text_secondary));
    } else {
        for h in app.history_ui.entries.iter().take(200) {
            let when = dbhelper_utils::textfmt::ts(h.executed_at);
            rows = rows.push(
                container(
                    row![
                        kit::mono(ctx, h.sql.chars().take(120).collect::<String>(), ts::MICRO, t.text),
                        kit::hspace(),
                        kit::txt(ctx, format!("{when} · {} 行", h.rows), ts::TINY, t.text_tertiary),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center),
                )
                .padding(Padding { top: 6.0, right: 0.0, bottom: 6.0, left: 0.0 })
                .width(Length::Fill),
            );
        }
    }

    column![
        modal_header(ctx, "查询历史", Some("最近执行的 SQL".into()), Msg::DismissModal),
        container(
            scrollable(rows).height(Length::Fixed(320.0)),
        )
        .padding(Padding { top: 12.0, right: 20.0, bottom: 16.0, left: 20.0 }),
        modal_footer(ctx, None, vec![kit::primary(ctx, "关闭", Msg::DismissModal)]),
    ]
    .into()
}

pub fn _tone() -> Tone {
    Tone::Neutral
}

pub fn _space() -> f32 {
    space::M
}
