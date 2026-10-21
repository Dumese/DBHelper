//! Windows 平台窗口适配。
//!
//! 目标：同一个二进制在 Windows 10 与 Windows 11 上呈现各自符合系统规范的窗口边框。
//!
//! - **Windows 11（build ≥ 22000）**：通过 DWM 的 `DWMWA_WINDOW_CORNER_PREFERENCE`
//!   设为 `DWMWCP_ROUND`，由系统绘制符合 Win11 规范的圆角边框与阴影。
//! - **Windows 10**：DWM 没有该属性，保持直角边框。
//!
//! 关键细节：DWM 只为**带系统边框样式**的窗口绘制圆角与阴影。iced 的
//! `decorations: false` 让 winit 创建 `WS_POPUP` 窗口，DWM 不会接管。
//! 因此在 Win11 上补上 `WS_THICKFRAME | WS_CAPTION`，让 DWM 绘制系统规范的
//! 圆角（Win11 会把原生标题栏折叠为标题栏按钮区，与自绘标题栏叠加后视觉正常）；
//! 在 Win10 上显式移除这两个样式，保证是直角无边框窗口。
//!
//! 这里手写 `extern "system"` 声明，避免为几个常量引入新的 crates.io 依赖，
//! 与项目 `update.rs` 中 `ShowWindow` 的处理方式保持一致。

use raw_window_handle::{HasWindowHandle, RawWindowHandle};

// ---- Win32 常量（仅取所需部分）----

/// `GWL_STYLE`：取窗口样式（仅用于日志观测）。
const GWL_STYLE: i32 = -16;

/// DWM 窗口属性：`DWMWA_WINDOW_CORNER_PREFERENCE`（Win11 22H2 新增）。
const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
/// DWM 窗口属性：`DWMWA_BORDER_COLOR`（Win11 新增）。
const DWMWA_BORDER_COLOR: u32 = 34;

/// `DWMWCP_ROUND`：使用系统标准圆角（Win11 22H2 默认）。
const DWMWCP_ROUND: i32 = 2;
/// `DWMWCP_DONOTROUND`：不使用圆角（Windows 10 的视觉效果）。
const DWMWCP_DONOTROUND: i32 = 0;

/// DWM 颜色值 `COLOR_DEFAULT`：由系统决定（用于边框色）。
const DWMWA_COLOR_DEFAULT: u32 = 0xFFFF_FFFF;

#[cfg(windows)]
mod sys {
    extern "system" {
        pub fn GetWindowLongPtrW(hwnd: *mut core::ffi::c_void, index: i32) -> isize;
        pub fn SetWindowLongPtrW(hwnd: *mut core::ffi::c_void, index: i32, value: isize) -> isize;
        pub fn DwmSetWindowAttribute(
            hwnd: *mut core::ffi::c_void,
            attribute: u32,
            value: *const core::ffi::c_void,
            size: u32,
        ) -> i32;
        pub fn GetVersionExW(version_info: *mut core::ffi::c_void) -> i32;
    }
}

/// 系统是否为 Windows 11（build number ≥ 22000）。
///
/// 优先用 `RtlGetVersion`（拿到真实 build number），失败则回退到
/// `GetVersionExW`，再失败按 Windows 10 处理（直角边框）。
#[cfg(windows)]
fn is_windows_11() -> bool {
    /// `OSVERSIONINFOEXW` 的内存布局（Win32 定义）。
    ///
    /// 8 个 `u32`（含 dwOSVersionInfoSize / 版本号 / build / platform / SP），
    /// 随后 3 个 `u16`（SP 主次版本 + suite mask），最后 2 个 `u8`（产品类型 + 保留）。
    /// 字段顺序与填充必须与 Win32 完全一致，否则读到的 build number 是错的。
    #[repr(C)]
    struct VersionInfoExW {
        dw_os_version_info_size: u32,
        dw_major_version: u32,
        dw_minor_version: u32,
        dw_build_number: u32,
        dw_platform_id: u32,
        dw_service_pack_major: u32,
        dw_service_pack_minor: u32,
        dw_service_pack_major2: u32,
        dw_service_pack_minor2: u32,
        w_service_pack_major: u16,
        w_service_pack_minor: u16,
        w_suite_mask: u16,
        w_product_type: u8,
        w_reserved: u8,
    }

    // 直接调用 ntdll 的 RtlGetVersion：GetVersionEx 在 Win10+ 已被 shim，
    // 只有 RtlGetVersion 才返回真实的 build number。
    #[link(name = "ntdll")]
    extern "system" {
        fn RtlGetVersion(version_info: *mut core::ffi::c_void) -> i32;
    }

    let mut info = VersionInfoExW {
        dw_os_version_info_size: std::mem::size_of::<VersionInfoExW>() as u32,
        dw_major_version: 0,
        dw_minor_version: 0,
        dw_build_number: 0,
        dw_platform_id: 0,
        dw_service_pack_major: 0,
        dw_service_pack_minor: 0,
        dw_service_pack_major2: 0,
        dw_service_pack_minor2: 0,
        w_service_pack_major: 0,
        w_service_pack_minor: 0,
        w_suite_mask: 0,
        w_product_type: 0,
        w_reserved: 0,
    };

    // SAFETY: `info` 是有效的 VersionInfoExW 结构，字段尺寸与 Win32 定义一致；
    // RtlGetVersion 只读取并填充该结构。
    let status = unsafe { RtlGetVersion(&mut info as *mut VersionInfoExW as *mut core::ffi::c_void) };
    if status == 0 && info.dw_build_number != 0 {
        return info.dw_build_number >= 22000;
    }

    // 回退：GetVersionExW
    // SAFETY: 同上，传入有效结构指针。
    unsafe {
        sys::GetVersionExW(&mut info as *mut VersionInfoExW as *mut core::ffi::c_void)
    };
    info.dw_build_number >= 22000
}
/// 把窗口边框调整为当前 Windows 版本应有的样式。
///
/// - Win11：系统圆角 + 跟随主题的 1px 边框 + 阴影。
/// - Win10：保持直角，不引入 Win11 才有的边框色。
///
/// 必须在窗口创建**之后**调用（DWM 属性是按窗口句柄设置的）。
pub fn apply_window_decoration(hwnd: *mut core::ffi::c_void, dark: bool) {
    #[cfg(windows)]
    {
        if hwnd.is_null() {
            return;
        }
        let win11 = is_windows_11();

        // SAFETY: hwnd 来自本进程自己的窗口（由 winit 创建），
        // Get/SetWindowLongPtrW 对本线程拥有的窗口是安全的。
        unsafe {
            // 注意：**不要**修改窗口样式。
            // 实测（tools/probe_corner.py）：仅设置
            // DWMWA_WINDOW_CORNER_PREFERENCE=DWMWCP_ROUND 即可让
            // Win11 对 WS_POPUP 无边框窗口裁出系统圆角；
            // 而额外加上 WS_CAPTION 会让 DWM 绘制原生标题栏区域，
            // 反而破坏自绘标题栏的布局与圆角。
            //
            // 圆角偏好：Win11 → 系统圆角；Win10 → 该属性不存在，
            // 调用失败，窗口保持直角（这正是期望行为）。
            let preference: i32 = if win11 { DWMWCP_ROUND } else { DWMWCP_DONOTROUND };
            let hr = sys::DwmSetWindowAttribute(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &preference as *const _ as *const core::ffi::c_void,
                std::mem::size_of::<i32>() as u32,
            );
            let _ = hr;

            if win11 {
                // 边框颜色跟随应用主题（浅色 #E9E9E7 / 深色 #373737），
                // 避免系统默认深灰边框在浅色界面里显得突兀。
                let border_color: u32 = if dark { 0x0037_3737 } else { 0x00E9_E9E7 };
                sys::DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_BORDER_COLOR,
                    &border_color as *const _ as *const core::ffi::c_void,
                    std::mem::size_of::<u32>() as u32,
                );
                let _ = DWMWA_COLOR_DEFAULT;
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (hwnd, dark);
    }
}

/// 从 iced 窗口对象取出原生 HWND 并应用平台适配。
///
/// `window` 是 iced 的窗口 trait object；`HasWindowHandle` 是 `Window` 的
/// 父 trait，但 trait object 需要 `?Sized` 才能通过编译。
pub fn decorate_window(window: &dyn iced::window::Window, dark: bool) {
    if let Ok(handle) = window.window_handle() {
        if let RawWindowHandle::Win32(win32) = handle.as_raw() {
            apply_window_decoration(win32.hwnd.get() as *mut core::ffi::c_void, dark);
        }
    }
}
