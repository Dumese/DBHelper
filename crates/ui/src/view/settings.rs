//! 设置页：显示 / 常规 / 文件三类偏好，全部即时生效并持久化。

use crate::icons::Icon;
use crate::kit::{self, Ctx};
use crate::theme::{radius, size as ts, space};
use crate::{App, DirTarget, Msg};
use dbhelper_models::{Language, ThemeMode};
use iced::widget::{column, container, pick_list, row, scrollable, Space};
use iced::{Alignment, Border, Element, Length, Padding, Theme};

pub fn view<'a>(app: &'a App, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;

    let header = container(
        column![
            kit::txt_bold(ctx, "设置", ts::PAGE, t.text),
            kit::txt(
                ctx,
                "管理 DBHelper 的外观、编辑器行为与文件位置偏好。",
                ts::SMALL,
                t.text_secondary
            ),
        ]
        .spacing(2),
    )
    .padding(Padding { top: 18.0, right: 24.0, bottom: 10.0, left: 24.0 });

    let display = card_block(
        ctx,
        "显示配置",
        column![
            kit::setting_row(
                ctx,
                "颜色模式",
                "切换深色或浅色界面主题（全局生效）",
                kit::segmented(
                    ctx,
                    &[
                        ("浅色", app.settings.theme == ThemeMode::Light),
                        ("深色", app.settings.theme == ThemeMode::Dark)
                    ],
                    |i| Msg::SetTheme(if i == 0 { ThemeMode::Light } else { ThemeMode::Dark })
                )
            ),
            kit::hline(ctx),
            kit::setting_row(
                ctx,
                "语言",
                "界面显示语言",
                pick_list(
                    &Language::ALL[..],
                    Some(app.settings.language),
                    Msg::SetLanguage
                )
                .text_size(ctx.f(ts::SMALL))
                .font(ctx.ui)
                .padding(Padding { top: 6.0, right: 10.0, bottom: 6.0, left: 10.0 })
                .width(Length::Fixed(200.0))
                .style(kit::select_style(ctx))
                .menu_style(kit::select_menu_style(ctx))
                .handle(pick_list::Handle::Arrow { size: Some(9.0.into()) })
            ),
            kit::hline(ctx),
            kit::setting_row(
                ctx,
                "编辑器字体",
                "代码编辑区使用，仅可选等宽字体",
                pick_list(
                    vec!["JetBrains Mono".to_string(), "Noto Sans SC".to_string(), "Cascadia Mono".to_string()],
                    Some(app.settings.editor_font.clone()),
                    Msg::SetEditorFont
                )
                .text_size(ctx.f(ts::SMALL))
                .font(ctx.mono)
                .padding(Padding { top: 6.0, right: 10.0, bottom: 6.0, left: 10.0 })
                .width(Length::Fixed(200.0))
                .style(kit::select_style(ctx))
                .menu_style(kit::select_menu_style(ctx))
                .handle(pick_list::Handle::Arrow { size: Some(9.0.into()) })
            ),
            kit::hline(ctx),
            kit::setting_row(
                ctx,
                "显示字体",
                "界面组件与按钮文字的显示",
                row![
                    pick_list(
                        vec!["Noto Sans SC".to_string(), "Noto Serif SC".to_string()],
                        Some(app.settings.ui_font.clone()),
                        Msg::SetUiFont
                    )
                    .text_size(ctx.f(ts::SMALL))
                    .font(ctx.ui)
                    .padding(Padding { top: 6.0, right: 10.0, bottom: 6.0, left: 10.0 })
                    .width(Length::Fixed(150.0))
                    .style(kit::select_style(ctx))
                    .menu_style(kit::select_menu_style(ctx))
                    .handle(pick_list::Handle::Arrow { size: Some(9.0.into()) }),
                    kit::segmented(
                        ctx,
                        &[("无衬线", !app.settings.ui_serif), ("衬线", app.settings.ui_serif)],
                        |i| Msg::SetSerif(i == 1)
                    ),
                ]
                .spacing(8)
            ),
            kit::hline(ctx),
            kit::setting_row(
                ctx,
                "字体大小",
                "调整界面文字大小（12–18 px）",
                row![
                    container(
                        kit::slider_row(
                            ctx,
                            app.settings.font_size as f32,
                            12.0..=18.0,
                            |v| Msg::SetFontSize(v.round() as u16)
                        )
                    )
                    .width(Length::Fixed(200.0)),
                    kit::txt(ctx, format!("{} px", app.settings.font_size), ts::SMALL, t.text_secondary),
                ]
                .spacing(10)
                .align_y(Alignment::Center)
            ),
        ]
        .spacing(0),
    );

    let normal = card_block(
        ctx,
        "常规配置",
        column![
            toggle_row(ctx, "显示行号", "在编辑器中显示行号", app.settings.show_line_numbers, Msg::SetLineNumbers),
            toggle_row(ctx, "自动换行", "长行自动折叠换行", app.settings.auto_wrap, Msg::SetAutoWrap),
            toggle_row(ctx, "语法高亮", "为 SQL 等语句着色关键字与函数", app.settings.syntax_highlight, Msg::SetSyntax),
            toggle_row(ctx, "括号高亮", "匹配括号时高亮提示", app.settings.bracket_highlight, Msg::SetBracket),
        ]
        .spacing(0),
    );

    let files = card_block(
        ctx,
        "文件配置",
        column![
            path_row(
                ctx,
                "日志位置",
                "应用程序日志文件保存路径",
                &app.settings.log_dir,
                DirTarget::Log
            ),
            kit::hline(ctx),
            path_row(
                ctx,
                "配置文件位置",
                "DBHelper 配置文件（config.json）路径",
                &app.settings.config_path,
                DirTarget::Config
            ),
            kit::hline(ctx),
            kit::setting_row(
                ctx,
                "导出向导",
                "将连接配置、查询历史与偏好导出为文件",
                kit::primary(ctx, "打开导出向导", Msg::OpenExportWizard)
            ),
        ]
        .spacing(0),
    );

    // 左侧导航切换的是当前分区，而不是把三张卡片一次性铺开
    let section = app.settings_ui.section.min(2);
    let active: Element<Msg> = match section {
        1 => normal,
        2 => files,
        _ => display,
    };
    let content = column![
        header,
        container(active)
            .width(Length::Fill)
            .padding(Padding { top: 0.0, right: 24.0, bottom: 24.0, left: 24.0 }),
    ]
    .width(Length::Fill);

    scrollable(content)
        .height(Length::Fill)
        .style(crate::view::shell::scroll_style(ctx))
        .into()
}

fn card_block<'a>(ctx: Ctx, title: &str, body: impl Into<Element<'a, Msg>>) -> Element<'a, Msg> {
    let body: Element<'a, Msg> = body.into();
    let t = ctx.t;
    container(
        column![
            kit::txt_bold(ctx, title.to_string(), ts::H2, t.text),
            Space::new().width(Length::Fixed(0.0)).height(Length::Fixed(8.0)),
            body,
        ]
        .spacing(0)
        .width(Length::Fill),
    )
    .padding(Padding { top: 14.0, right: 16.0, bottom: 6.0, left: 16.0 })
    .width(Length::Fill)
    .style(move |_: &Theme| container::Style {
        background: Some(t.bg.into()),
        border: Border { color: t.hairline, width: 1.0, radius: radius::MD.into() },
        ..Default::default()
    })
    .into()
}

fn toggle_row<'a>(
    ctx: Ctx,
    title: &str,
    desc: &str,
    value: bool,
    on: impl Fn(bool) -> Msg + 'a,
) -> Element<'a, Msg> {
    kit::setting_row(ctx, title, desc, kit::switch(ctx, value, on))
}

fn path_row<'a>(ctx: Ctx, title: &str, desc: &str, value: &str, target: DirTarget) -> Element<'a, Msg> {
    let t = ctx.t;
    kit::setting_row(
        ctx,
        title,
        desc,
        row![
            container(kit::input(ctx, "", value, |_| Msg::Noop))
                .width(Length::Fixed(330.0)),
            kit::secondary(ctx, "浏览…", Msg::BrowseDir(target)),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .pipe_icon(ctx, Icon::Folder, t.text_tertiary)
}

trait PipeIcon<'a>: Sized {
    fn pipe_icon(self, ctx: Ctx, icon: Icon, color: iced::Color) -> Element<'a, Msg>;
}

impl<'a> PipeIcon<'a> for Element<'a, Msg> {
    fn pipe_icon(self, _ctx: Ctx, _icon: Icon, _color: iced::Color) -> Element<'a, Msg> {
        self
    }
}

pub fn _space() -> f32 {
    space::M
}
