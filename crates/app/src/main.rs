//! DBHelper 主程序入口。
//!
//! 支持 `--uninstall`：由安装器写入的 UninstallString 调用，清理快捷方式、
//! 注册表项与安装目录（自身 exe 通过延迟 cmd 进程删除）。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use dbhelper_ui::App;
use iced::window;
use iced::Size;

fn main() -> iced::Result {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--uninstall") {
        uninstall();
        return Ok(());
    }

    iced::application(App::boot, App::update, view)
        .title("DBHelper")
        .window(window::Settings {
            size: Size::new(1440.0, 900.0),
            min_size: Some(Size::new(1200.0, 760.0)),
            decorations: false,
            transparent: false,
            resizable: true,
            position: window::Position::Centered,
            ..Default::default()
        })
        .theme(App::theme)
        .subscription(App::subscription)
        .default_font(iced::Font::with_name("Noto Sans SC"))
        .antialiasing(true)
        .run()
        .map(|ok| {
            let _ = std::fs::write(
                std::env::temp_dir().join("dbhelper_exit.flag"),
                format!("exit ok={ok:?}"),
            );
            ok
        })
}

fn view(app: &App) -> iced::Element<'_, dbhelper_ui::Msg> {
    dbhelper_ui::view::view(app)
}

#[cfg(windows)]
fn uninstall() {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let exe = std::env::current_exe().unwrap_or_default();
    let dir = exe.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let desktop = std::env::var_os("USERPROFILE")
        .map(std::path::PathBuf::from)
        .map(|p| p.join("Desktop/DBHelper.lnk"))
        .unwrap_or_default();
    let start_menu = std::env::var_os("APPDATA")
        .map(std::path::PathBuf::from)
        .map(|p| p.join("Microsoft/Windows/Start Menu/Programs/DBHelper.lnk"))
        .unwrap_or_default();

    let script = format!(
        "Remove-Item -LiteralPath '{}' -Force -ErrorAction SilentlyContinue; \
         Remove-Item -LiteralPath '{}' -Force -ErrorAction SilentlyContinue; \
         Remove-Item -LiteralPath 'HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\DBHelper' -Recurse -Force -ErrorAction SilentlyContinue; \
         Start-Process -WindowStyle Hidden cmd -ArgumentList '/c timeout /t 2 >nul & rmdir /s /q \"{}\"'",
        desktop.display(),
        start_menu.display(),
        dir.display()
    );
    let _ = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn();
}

#[cfg(not(windows))]
fn uninstall() {}
