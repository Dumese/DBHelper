//! 设计令牌：颜色 / 字号 / 间距 / 圆角 / 阴影。
//!
//! 数值取自原型《DBHelper-全国设计规范》：亮色 `#FFFFFF/#FBFBFA/#37352F/#2383E2`，
//! 暗色 `#191919/#202020/#E9E9E7`，1px `#E9E9E7` 细边框，4px 基准间距网格。

use iced::{Color, Font};

pub const fn rgb(hex: u32) -> Color {
    Color {
        r: ((hex >> 16) & 0xFF) as f32 / 255.0,
        g: ((hex >> 8) & 0xFF) as f32 / 255.0,
        b: (hex & 0xFF) as f32 / 255.0,
        a: 1.0,
    }
}

pub const fn rgba(hex: u32, a: f32) -> Color {
    let c = rgb(hex);
    Color { a, ..c }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tokens {
    pub dark: bool,
    pub bg: Color,
    pub surface: Color,
    pub soft: Color,
    pub hairline: Color,
    pub hairline_strong: Color,
    pub text: Color,
    pub text_secondary: Color,
    pub text_tertiary: Color,
    pub accent: Color,
    pub accent_hover: Color,
    pub accent_pressed: Color,
    pub accent_soft: Color,
    pub hover: Color,
    pub selected: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub success_soft: Color,
    pub warning_soft: Color,
    pub error_soft: Color,
    pub overlay: Color,
}

impl Tokens {
    pub const LIGHT: Tokens = Tokens {
        dark: false,
        bg: rgb(0xFFFFFF),
        surface: rgb(0xFBFBFA),
        soft: rgb(0xFAFAF9),
        hairline: rgb(0xE9E9E7),
        hairline_strong: rgb(0xD3D1CB),
        text: rgb(0x37352F),
        text_secondary: rgb(0x787774),
        text_tertiary: rgb(0xA4A097),
        accent: rgb(0x3A7FC9),
        accent_hover: rgb(0x2E6BAF),
        accent_pressed: rgb(0x25588C),
        accent_soft: rgb(0xEAF2FB),
        hover: rgb(0xF1F0EE),
        selected: rgb(0xEDECEA),
        success: rgb(0x3B9A67),
        warning: rgb(0xE2A93B),
        error: rgb(0xC2453D),
        success_soft: rgb(0xEDF3EC),
        warning_soft: rgb(0xFBF3DB),
        error_soft: rgb(0xFBECEC),
        overlay: rgba(0x0F0F0F, 0.45),
    };

    pub const DARK: Tokens = Tokens {
        dark: true,
        bg: rgb(0x191919),
        surface: rgb(0x202020),
        soft: rgb(0x1C1C1C),
        hairline: rgb(0x373737),
        hairline_strong: rgb(0x4A4A4A),
        text: rgb(0xE9E9E7),
        text_secondary: rgb(0x9B9B9B),
        text_tertiary: rgb(0x6F6F6F),
        accent: rgb(0x2383E2),
        accent_hover: rgb(0x4C9BEE),
        accent_pressed: rgb(0x1B6FBE),
        accent_soft: rgb(0x1B2C3D),
        hover: rgb(0x2C2C2C),
        selected: rgb(0x2F2F2F),
        success: rgb(0x5DA97A),
        warning: rgb(0xD9A343),
        error: rgb(0xE5645E),
        success_soft: rgb(0x1E2B23),
        warning_soft: rgb(0x2E2718),
        error_soft: rgb(0x33201F),
        overlay: rgba(0x000000, 0.55),
    };

    pub fn of(dark: bool) -> Self {
        if dark {
            Self::DARK
        } else {
            Self::LIGHT
        }
    }

    /// 语义色软背景：把状态色转成浅底。
    pub fn tone_soft(&self, tone: Tone) -> Color {
        match tone {
            Tone::Neutral => self.hover,
            Tone::Success => self.success_soft,
            Tone::Warning => self.warning_soft,
            Tone::Error => self.error_soft,
            Tone::Accent => self.accent_soft,
        }
    }

    pub fn tone(&self, tone: Tone) -> Color {
        match tone {
            Tone::Neutral => self.text_secondary,
            Tone::Success => self.success,
            Tone::Warning => self.warning,
            Tone::Error => self.error,
            Tone::Accent => self.accent,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Neutral,
    Success,
    Warning,
    Error,
    Accent,
}

// ---- 间距（4px 基准网格）----
pub mod space {
    pub const XS: f32 = 4.0;
    pub const S: f32 = 8.0;
    pub const M: f32 = 12.0;
    pub const L: f32 = 16.0;
    pub const XL: f32 = 24.0;
    pub const XXL: f32 = 32.0;
}

// ---- 圆角 ----
pub mod radius {
    pub const XS: f32 = 4.0;
    pub const SM: f32 = 6.0;
    pub const MD: f32 = 8.0;
    pub const LG: f32 = 12.0;
}

// ---- 字号 ----
pub mod size {
    // 与原型图（1080 逻辑宽）对齐的紧凑字阶
    pub const PAGE: f32 = 22.0;
    pub const H1: f32 = 18.0;
    pub const H2: f32 = 15.0;
    pub const H3: f32 = 14.0;
    pub const BODY: f32 = 12.5;
    pub const SMALL: f32 = 11.5;
    pub const MICRO: f32 = 10.5;
    pub const TINY: f32 = 10.0;
}

/// 界面字体族：Windows 上 "Inter" 未安装时由 cosmic-text 回退到系统无衬线体。
/// 字体族必须是 `'static`：把设置里的可选项映射为静态字面量。
pub fn static_family(name: &str) -> &'static str {
    match name {
        // 设计规范指定的三套字体（随系统安装）
        "Noto Sans SC" => "Noto Sans SC",
        "Noto Serif SC" => "Noto Serif SC",
        "JetBrains Mono" => "JetBrains Mono",
        // 兼容旧配置与外部导入的配置
        "Inter" | "Segoe UI" | "Microsoft YaHei UI" => "Noto Sans SC",
        "Consolas" | "Cascadia Mono" | "IBM Plex Mono" => "JetBrains Mono",
        "Georgia" | "宋体" => "Noto Serif SC",
        _ => "Noto Sans SC",
    }
}

pub fn ui_font(ui_font_name: &str, serif: bool) -> Font {
    let family = if serif { "Noto Serif SC" } else { static_family(ui_font_name) };
    Font::with_name(family)
}

pub fn mono_font(name: &str) -> Font {
    Font::with_name(static_family(name))
}

pub fn bold(font: Font) -> Font {
    Font { weight: iced::font::Weight::Semibold, ..font }
}

pub fn medium(font: Font) -> Font {
    Font { weight: iced::font::Weight::Medium, ..font }
}
