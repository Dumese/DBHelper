//! 通用工具：应用目录、加密 Vault、SQL 词法/格式化、展示格式化、日志。

pub mod paths;
pub mod sql;
pub mod textfmt;
pub mod vault;

pub use paths::{app_dirs, AppDirs};
pub use vault::{Vault, VaultError};

/// 初始化 tracing（写文件 + 控制台），重复调用安全。
pub fn init_tracing(log_dir: &std::path::Path) {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};

    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        let _ = std::fs::create_dir_all(log_dir);
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_dir.join("dbhelper.log"))
            .ok();

        let filter = EnvFilter::try_from_env("DBHELPER_LOG")
            .unwrap_or_else(|_| EnvFilter::new("info,dbhelper=debug,sqlx=warn,hyper=warn"));

        let registry = tracing_subscriber::registry().with(filter);
        match file {
            Some(f) => {
                let layer = fmt::layer()
                    .with_writer(std::sync::Mutex::new(f))
                    .with_ansi(false)
                    .with_target(true);
                let _ = registry.with(layer).try_init();
            }
            None => {
                let _ = registry.with(fmt::layer()).try_init();
            }
        }
    });
}
