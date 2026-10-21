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
    let tr = ctx.tr;

    let header = container(
        column![
            kit::txt_bold(ctx, tr.settings(), ts::PAGE, t.text),
            kit::txt(ctx, tr.settings_sub(), ts::SMALL, t.text_secondary),
        ]
        .spacing(2),
    )
    .padding(Padding { top: 18.0, right: 24.0, bottom: 10.0, left: 24.0 });

    let display = card_block(
        ctx,
        tr.display_config(),
        column![
            kit::setting_row(
                ctx,
                tr.color_mode(),
                tr.color_mode_desc(),
                kit::segmented(
                    ctx,
                    &[
                        (tr.light(), app.settings.theme == ThemeMode::Light),
                        (tr.dark(), app.settings.theme == ThemeMode::Dark)
                    ],
                    |i| Msg::SetTheme(if i == 0 { ThemeMode::Light } else { ThemeMode::Dark })
                )
            ),
            kit::hline(ctx),
            kit::setting_row(
                ctx,
                tr.language(),
                tr.language_desc(),
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
                tr.editor_font(),
                tr.editor_font_desc(),
                pick_list(
                    vec![
                        "JetBrains Mono".to_string(),
                        "Consolas".to_string(),
                        "Courier New".to_string(),
                    ],
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
                tr.ui_font(),
                tr.ui_font_desc(),
                row![
                    pick_list(
                        vec![
                            "Noto Sans SC".to_string(),
                            "Noto Serif SC".to_string(),
                        ],
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
                        &[(tr.sans(), !app.settings.ui_serif), (tr.serif(), app.settings.ui_serif)],
                        |i| Msg::SetSerif(i == 1)
                    ),
                ]
                .spacing(8)
            ),
            kit::hline(ctx),
            kit::setting_row(
                ctx,
                tr.font_size(),
                tr.font_size_desc(),
                row![
                    container(kit::slider_row(
                        ctx,
                        app.settings.font_size as f32,
                        12.0..=18.0,
                        |v| Msg::SetFontSize(v.round() as u16)
                    ))
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
        tr.normal_config(),
        column![
            toggle_row(ctx, tr.show_line_numbers(), tr.show_line_numbers_desc(), app.settings.show_line_numbers, Msg::SetLineNumbers),
            toggle_row(ctx, tr.auto_wrap(), tr.auto_wrap_desc(), app.settings.auto_wrap, Msg::SetAutoWrap),
            toggle_row(ctx, tr.syntax_highlight(), tr.syntax_highlight_desc(), app.settings.syntax_highlight, Msg::SetSyntax),
            toggle_row(ctx, tr.bracket_highlight(), tr.bracket_highlight_desc(), app.settings.bracket_highlight, Msg::SetBracket),
        ]
        .spacing(0),
    );

    let files = card_block(
        ctx,
        tr.file_config(),
        column![
            path_row(ctx, tr.log_location(), tr.log_location_desc(), &app.settings.log_dir, DirTarget::Log),
            kit::hline(ctx),
            path_row(ctx, tr.config_location(), tr.config_location_desc(), &app.settings.config_path, DirTarget::Config),
            kit::hline(ctx),
            kit::setting_row(
                ctx,
                tr.export_wizard(),
                tr.export_wizard_desc(),
                kit::primary(ctx, tr.open_export_wizard(), Msg::OpenExportWizard)
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
            kit::secondary(ctx, ctx.tr.browse(), Msg::BrowseDir(target)),
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
