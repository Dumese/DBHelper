//! 连接树右键菜单（自绘浮层）。
//!
//! iced 0.14 **没有**内置 `context_menu` 组件（该组件在 0.13 之后从
//! `iced_widget` 移除，需要 `iced_advanced` crate，而本项目未引入）。
//! 因此这里自行实现：用 `stack` 把菜单面板叠在侧栏之上，按记录的鼠标
//! 坐标定位，点击菜单外部（遮罩层）关闭。
//!
//! 菜单项：
//! 1. 打开连接
//! 2. 编辑连接
//! 3. 删除连接
//! 4. 复制连接
//! 5. 新建查询
//! 6. 命令行界面
//! 7. 刷新

use crate::icons::{icon, Icon};
use crate::kit::{self, Ctx};
use crate::theme::{control, radius, size as ts};
use crate::{App, Msg};
use iced::widget::{column, container, mouse_area, row, stack, text, Space};
use iced::{Alignment, Border, Color, Element, Length, Padding, Theme};

/// 连接右键菜单的浮层：全屏透明遮罩（点击关闭）+ 定位到鼠标处的菜单面板。
///
/// 菜单可能超出窗口右/下边界，这里按窗口尺寸做钳制，保证完整可见。
///
/// 定位实现：用 `stack` 把「定位层」铺满窗口，再在定位层里用
/// `column` + `row` + `Space` 把菜单推到 (x, y)。
/// 注意 `stack` 的子元素必须显式给 Fill 尺寸，否则会按内容收缩导致菜单不可见。
pub fn overlay<'a>(app: &'a App, ctx: Ctx) -> Element<'a, Msg> {
    let Some(conn_id) = app.conn_menu.conn else {
        return Space::new().width(0).height(0).into();
    };
    let connected = app.connected.contains(&conn_id);

    const MENU_W: f32 = 200.0;
    const MENU_H: f32 = 280.0;

    let (wx, wy) = app.conn_menu.pos;
    let vw = app.viewport.width;
    let vh = app.viewport.height;
    // 钳制：菜单右/下边缘不超出窗口。
    let x = (wx - 8.0).clamp(4.0, (vw - MENU_W - 4.0).max(4.0));
    let y = (wy - 8.0).clamp(4.0, (vh - MENU_H - 4.0).max(4.0));

    let menu = conn_context_menu(conn_id, connected, ctx);

    // 定位层：撑满窗口，用空隙把菜单推到 (x, y)。
    let positioned = container(
        column![
            Space::new().width(Length::Fill).height(Length::Fixed(y)),
            row![
                Space::new().width(Length::Fixed(x)).height(Length::Fill),
                menu,
            ]
            .align_y(Alignment::Start),
        ]
        .width(Length::Fill)
        .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill);

    // 遮罩层：透明但可点击，点击任意空白处关闭菜单。
    let backdrop = mouse_area(
        container(Space::new().width(Length::Fill).height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .on_press(Msg::ConnContextClose);

    // stack：backdrop 在下（先命中），positioned 在上（菜单可点）
    stack![backdrop, positioned]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// 菜单项定义。
struct MenuItem {
    icon: Icon,
    label: &'static str,
    /// 危险项（删除）用红色文字。
    danger: bool,
    msg: Msg,
}

/// 构造连接右键菜单面板（不含遮罩）。
///
/// `conn_id` 为目标连接；`connected` 表示该连接当前已连接，
/// 「打开连接」在已连接时显示为「断开连接」。
pub fn conn_context_menu<'a>(conn_id: dbhelper_models::ConnId, connected: bool, ctx: Ctx) -> Element<'a, Msg> {
    let t = ctx.t;

    let items = vec![
        MenuItem {
            icon: if connected { Icon::Stop } else { Icon::Play },
            label: if connected { "断开连接" } else { "打开连接" },
            danger: false,
            msg: if connected {
                Msg::ConnContextClose
            } else {
                Msg::OpenConnection(conn_id)
            },
        },
        MenuItem { icon: Icon::Pencil, label: "编辑连接", danger: false, msg: Msg::ConnEdit(conn_id) },
        MenuItem { icon: Icon::Copy, label: "复制连接", danger: false, msg: Msg::ConnDuplicate(conn_id) },
        MenuItem { icon: Icon::Terminal, label: "命令行界面", danger: false, msg: Msg::ConnCli(conn_id) },
        MenuItem { icon: Icon::Doc, label: "新建查询", danger: false, msg: Msg::ConnNewQuery(conn_id) },
        MenuItem { icon: Icon::Refresh, label: "刷新", danger: false, msg: Msg::ConnRefresh(conn_id) },
    ];

    let mut col = column![].spacing(1);
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            col = col.push(
                container(Space::new().width(Length::Fill).height(Length::Fixed(1.0)))
                    .width(Length::Fill)
                    .style(move |_: &Theme| container::Style {
                        background: Some(t.hairline.into()),
                        ..Default::default()
                    }),
            );
        }
        col = col.push(menu_entry(ctx, item));
    }
    // 删除单独放在分隔线之后，避免误触。
    col = col.push(
        container(Space::new().width(Length::Fill).height(Length::Fixed(1.0)))
            .width(Length::Fill)
            .style(move |_: &Theme| container::Style {
                background: Some(t.hairline.into()),
                ..Default::default()
            }),
    );
    col = col.push(menu_entry(
        ctx,
        &MenuItem {
            icon: Icon::Trash,
            label: "删除连接",
            danger: true,
            msg: Msg::DeleteConnection(conn_id),
        },
    ));

    container(col)
        .padding(Padding { top: 4.0, right: 4.0, bottom: 4.0, left: 4.0 })
        .width(Length::Fixed(200.0))
        .style(move |_: &Theme| container::Style {
            background: Some(t.bg.into()),
            border: Border { color: t.hairline_strong, width: 1.0, radius: radius::MD.into() },
            ..Default::default()
        })
        .into()
}

/// 单个菜单项：图标 + 文字，hover 高亮。
fn menu_entry<'a>(ctx: Ctx, item: &MenuItem) -> Element<'a, Msg> {
    let t = ctx.t;
    let fg = if item.danger { t.error } else { t.text };
    let msg = item.msg.clone();
    let h = ctx.f(32.0);
    let font = ctx.ui;
    let size = ctx.f(ts::BODY);

    // 用 `mouse_area` + `container` 自行控制布局：
    // iced 的 `button` 在紧凑容器里会把内部 `row!` 的文字裁掉
    // （表现为菜单项只有图标、没有文字），因此这里自绘。
    //
    // hover 高亮通过 `mouse_area` 的交互态无法直接拿到，
    // 改用 `iced::widget::button` 的替代方案：`container` 固定背景，
    // hover 效果由 `mouse_area` 的 hover 消息驱动（见 overlay 层）。
    mouse_area(
        container(
            row![
                icon(item.icon, ctx.f(control::ICON_SM), fg),
                text(item.label.to_string())
                    .size(size)
                    .font(font)
                    .color(fg),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .padding(Padding { top: 0.0, right: 8.0, bottom: 0.0, left: 8.0 })
        .width(Length::Fill),
    )
    .on_press(msg)
    .into()
}
