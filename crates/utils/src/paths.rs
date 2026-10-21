//! 应用数据目录解析。
//!
//! 安装版写入 `%APPDATA%\DBHelper`，便携版（exe 同目录存在 `portable.flag`）
//! 写入 exe 同目录的 `data/`，两者都不会污染用户其它目录。

use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct AppDirs {
    pub root: PathBuf,
    pub config: PathBuf,
    pub logs: PathBuf,
    pub app_db: PathBuf,
    pub vault: PathBuf,
    pub portable: bool,
}

impl AppDirs {
    pub fn resolve() -> Self {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from("."));

        let portable = exe_dir.join("portable.flag").exists();
        let root = if portable {
            exe_dir.join("data")
        } else {
            std::env::var_os("APPDATA")
                .map(|v| PathBuf::from(v).join("DBHelper"))
                .unwrap_or_else(|| exe_dir.join("data"))
        };

        let dirs = Self {
            config: root.join("config.json"),
            logs: root.join("logs"),
            app_db: root.join("dbhelper.db"),
            vault: root.join("vault.bin"),
            portable,
            root,
        };

        let _ = std::fs::create_dir_all(&dirs.root);
        let _ = std::fs::create_dir_all(&dirs.logs);
        dirs
    }
}

pub fn app_dirs() -> AppDirs {
    AppDirs::resolve()
}

/// 把绝对路径压成便于展示的短形式（保留盘符）。
pub fn shorten_path(path: &str, max: usize) -> String {
    if path.chars().count() <= max {
        return path.to_string();
    }
    let chars: Vec<char> = path.chars().collect();
    let head: String = chars[..8.min(chars.len())].iter().collect();
    let tail: String = chars[chars.len().saturating_sub(max.saturating_sub(11))..]
        .iter()
        .collect();
    format!("{head}…{tail}")
}
