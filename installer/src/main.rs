//! DBHelper 安装程序（单文件 setup.exe）。
//!
//! 负载（dbhelper.exe）在编译期通过 `include_bytes!` 嵌入，因此安装包
//! 无需解压其它文件即可完成安装；安装过程仅使用系统自带能力：
//! - 复制主程序到安装目录
//! - 通过 WScript.Shell 创建桌面与开始菜单快捷方式
//! - 写入 HKCU 卸载信息（UninstallString 指向 `dbhelper.exe --uninstall`）

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use iced::widget::{column, container, row, text, text_input};
use iced::{Alignment, Border, Color, Element, Length, Padding, Task, Theme};
use std::path::PathBuf;

const PAYLOAD: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/dbhelper.exe"));
const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> iced::Result {
    iced::application(Setup::boot, Setup::update, Setup::view)
        .title("DBHelper 安装程序")
        .window(iced::window::Settings {
            size: iced::Size::new(560.0, 340.0),
            resizable: false,
            decorations: true,
            position: iced::window::Position::Centered,
            ..Default::default()
        })
        .theme(|_: &Setup| Theme::Light)
        .default_font(iced::Font::with_name("Segoe UI"))
        .run()
}

#[derive(Debug, Clone)]
enum Msg {
    PathChanged(String),
    Browse,
    Install,
    Installed(Result<(), String>),
    Launch,
    Close,
}

#[derive(Debug, Clone, PartialEq)]
enum Stage {
    Ready,
    Installing,
    Done,
    Failed(String),
}

struct Setup {
    path: String,
    stage: Stage,
}

impl Setup {
    fn boot() -> Self {
        Self { path: default_dir().to_string_lossy().to_string(), stage: Stage::Ready }
    }

    fn update(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::PathChanged(p) => {
                self.path = p;
                Task::none()
            }
            Msg::Browse => Task::perform(
                async { rfd_pick_folder().await },
                |p| Msg::PathChanged(p.map(|p| p.to_string_lossy().to_string()).unwrap_or_default()),
            ),
            Msg::Install => {
                self.stage = Stage::Installing;
                let dir = PathBuf::from(self.path.clone());
                Task::perform(async move { install(dir).await }, Msg::Installed)
            }
            Msg::Installed(result) => {
                match result {
                    Ok(()) => self.stage = Stage::Done,
                    Err(e) => self.stage = Stage::Failed(e),
                }
                Task::none()
            }
            Msg::Launch => {
                let exe = PathBuf::from(&self.path).join("dbhelper.exe");
                let _ = std::process::Command::new(exe).spawn();
                iced::exit()
            }
            Msg::Close => iced::exit(),
        }
    }

    fn view(&self) -> Element<'_, Msg> {
        let ink = Color::from_rgb(0.216, 0.208, 0.184);
        let muted = Color::from_rgb(0.471, 0.467, 0.455);

        let header = column![
            text("DBHelper 安装向导").size(24).font(bold()),
            text(format!("版本 {VERSION} · Windows 原生数据库管理工具")).size(13).color(muted),
        ]
        .spacing(4);

        let body: Element<Msg> = match &self.stage {
            Stage::Ready | Stage::Installing => column![
                text("安装位置").size(13).color(ink),
                row![
                    text_input("安装目录", &self.path)
                        .on_input(Msg::PathChanged)
                        .padding(8)
                        .size(13)
                        .width(Length::Fill),
                    button_secondary("浏览…", Msg::Browse),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                text("将创建桌面与开始菜单快捷方式，并写入当前用户的卸载信息。").size(12).color(muted),
                iced::widget::Space::new(Length::Fill, Length::Fixed(10.0)),
                row![
                    iced::widget::horizontal_space(),
                    button_primary(
                        if self.stage == Stage::Installing { "正在安装…" } else { "安装" },
                        Msg::Install
                    ),
                ],
            ]
            .spacing(10)
            .into(),
            Stage::Done => column![
                text("安装完成").size(18).color(ink),
                text(format!("DBHelper 已安装到 {}", self.path)).size(13).color(muted),
                iced::widget::Space::new(Length::Fill, Length::Fixed(12.0)),
                row![
                    iced::widget::horizontal_space(),
                    button_secondary("关闭", Msg::Close),
                    button_primary("立即启动", Msg::Launch),
                ]
                .spacing(8),
            ]
            .spacing(10)
            .into(),
            Stage::Failed(e) => column![
                text("安装失败").size(18).color(Color::from_rgb(0.878, 0.243, 0.243)),
                text(e.clone()).size(13).color(ink),
                iced::widget::Space::new(Length::Fill, Length::Fixed(12.0)),
                row![
                    iced::widget::horizontal_space(),
                    button_primary("重试", Msg::Install),
                    button_secondary("关闭", Msg::Close),
                ]
                .spacing(8),
            ]
            .spacing(10)
            .into(),
        };

        container(
            column![header, iced::widget::Space::new(Length::Fill, Length::Fixed(18.0)), body]
                .spacing(0)
                .width(Length::Fill),
        )
        .padding(Padding::from([24, 26]))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_: &Theme| container::Style {
            background: Some(Color::WHITE.into()),
            text_color: Some(ink),
            ..Default::default()
        })
        .into()
    }
}

fn bold() -> iced::Font {
    iced::Font { weight: iced::font::Weight::Semibold, ..iced::Font::with_name("Segoe UI") }
}

fn button_primary<'a>(label: &'a str, msg: Msg) -> Element<'a, Msg> {
    iced::widget::button(text(label).size(13).color(Color::WHITE))
        .padding(Padding::from([8, 18]))
        .style(|_: &Theme, status| {
            let bg = match status {
                iced::widget::button::Status::Hovered => Color::from_rgb(0.043, 0.42, 0.796),
                _ => Color::from_rgb(0.137, 0.514, 0.886),
            };
            iced::widget::button::Style {
                background: Some(bg.into()),
                text_color: Color::WHITE,
                border: Border::default().rounded(6),
                ..Default::default()
            }
        })
        .on_press(msg)
        .into()
}

fn button_secondary<'a>(label: &'a str, msg: Msg) -> Element<'a, Msg> {
    iced::widget::button(text(label).size(13))
        .padding(Padding::from([8, 14]))
        .style(|_: &Theme, status| {
            let bg = match status {
                iced::widget::button::Status::Hovered => Color::from_rgb(0.945, 0.945, 0.937),
                _ => Color::WHITE,
            };
            iced::widget::button::Style {
                background: Some(bg.into()),
                text_color: Color::from_rgb(0.216, 0.208, 0.184),
                border: Border { color: Color::from_rgb(0.827, 0.819, 0.796), width: 1.0, radius: 6.0.into() },
                ..Default::default()
            }
        })
        .on_press(msg)
        .into()
}

fn default_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("C:/Program Files"))
        .join("DBHelper")
}

async fn rfd_pick_folder() -> Option<PathBuf> {
    rfd::AsyncFileDialog::new().pick_folder().await.map(|f| f.path().to_path_buf())
}

// ---------------------------------------------------------------------------
// 安装逻辑
// ---------------------------------------------------------------------------

async fn install(dir: PathBuf) -> Result<(), String> {
    tokio::task::spawn_blocking(move || install_blocking(dir))
        .await
        .map_err(|e| e.to_string())?
}

fn install_blocking(dir: PathBuf) -> Result<(), String> {
    if PAYLOAD.starts_with(b"DBHELPER_PAYLOAD_MISSING") {
        return Err("安装包未嵌入主程序：请先 cargo build --release -p dbhelper，再重新构建安装器。".into());
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建安装目录失败：{e}"))?;
    let exe = dir.join("dbhelper.exe");
    std::fs::write(&exe, PAYLOAD).map_err(|e| format!("写入主程序失败：{e}"))?;

    let desktop = std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .map(|p| p.join("Desktop"))
        .unwrap_or_default();
    let start_menu = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .map(|p| p.join("Microsoft/Windows/Start Menu/Programs"))
        .unwrap_or_default();
    let _ = shortcut(&desktop.join("DBHelper.lnk"), &exe);
    let _ = shortcut(&start_menu.join("DBHelper.lnk"), &exe);

    let uninstall = format!("\"{}\" --uninstall", exe.display());
    let ps = format!(
        "$k='HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\DBHelper'; \
         New-Item -Path $k -Force | Out-Null; \
         New-ItemProperty -Path $k -Name DisplayName -Value 'DBHelper' -PropertyType String -Force | Out-Null; \
         New-ItemProperty -Path $k -Name DisplayVersion -Value '{}' -PropertyType String -Force | Out-Null; \
         New-ItemProperty -Path $k -Name Publisher -Value 'DBHelper' -PropertyType String -Force | Out-Null; \
         New-ItemProperty -Path $k -Name InstallLocation -Value '{}' -PropertyType String -Force | Out-Null; \
         New-ItemProperty -Path $k -Name UninstallString -Value '{}' -PropertyType String -Force | Out-Null; \
         New-ItemProperty -Path $k -Name DisplayIcon -Value '{}' -PropertyType String -Force | Out-Null",
        VERSION,
        dir.display(),
        uninstall,
        exe.display()
    );
    run_powershell(&ps)?;
    Ok(())
}

fn shortcut(lnk: &std::path::Path, exe: &std::path::Path) -> Result<(), String> {
    let ps = format!(
        "$s=(New-Object -COM WScript.Shell).CreateShortcut('{}'); $s.TargetPath='{}'; $s.WorkingDirectory='{}'; $s.Save()",
        lnk.display(),
        exe.display(),
        exe.parent().map(|p| p.display().to_string()).unwrap_or_default()
    );
    run_powershell(&ps)
}

#[cfg(windows)]
fn run_powershell(script: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let output = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", script])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("调用 PowerShell 失败：{e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

#[cfg(not(windows))]
fn run_powershell(_script: &str) -> Result<(), String> {
    Ok(())
}
