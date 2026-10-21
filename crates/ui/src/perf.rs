//! 轻量性能埋点：每秒汇总一次 view / update 的调用次数与耗时写入日志。
//!
//! 用法：运行程序并操作界面，然后看 `%APPDATA%\DBHelper\logs\dbhelper.log`
//! 中的 `perf:` 行 —— 空闲时 view 应为 1 次/秒（Tick），交互时快速上升；
//! 平均/峰值耗时可直接定位「卡」发生在重建（view/update）还是绘制阶段。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

static START: OnceLock<Instant> = OnceLock::new();
static LAST_LOG_MS: AtomicU64 = AtomicU64::new(0);

static VIEW_N: AtomicU64 = AtomicU64::new(0);
static VIEW_SUM_US: AtomicU64 = AtomicU64::new(0);
static VIEW_MAX_US: AtomicU64 = AtomicU64::new(0);
static UPDATE_N: AtomicU64 = AtomicU64::new(0);
static UPDATE_SUM_US: AtomicU64 = AtomicU64::new(0);
static UPDATE_MAX_US: AtomicU64 = AtomicU64::new(0);

fn now_ms() -> u64 {
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

pub fn record_view(d: Duration) {
    let us = d.as_micros() as u64;
    VIEW_N.fetch_add(1, Ordering::Relaxed);
    VIEW_SUM_US.fetch_add(us, Ordering::Relaxed);
    VIEW_MAX_US.fetch_max(us, Ordering::Relaxed);
    maybe_flush();
}

pub fn record_update(d: Duration) {
    let us = d.as_micros() as u64;
    UPDATE_N.fetch_add(1, Ordering::Relaxed);
    UPDATE_SUM_US.fetch_add(us, Ordering::Relaxed);
    UPDATE_MAX_US.fetch_max(us, Ordering::Relaxed);
    maybe_flush();
}

fn maybe_flush() {
    let now = now_ms();
    let last = LAST_LOG_MS.load(Ordering::Relaxed);
    if now.saturating_sub(last) < 1000 {
        return;
    }
    if LAST_LOG_MS
        .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
        .is_err()
    {
        return;
    }
    let vn = VIEW_N.swap(0, Ordering::Relaxed);
    let vs = VIEW_SUM_US.swap(0, Ordering::Relaxed);
    let vm = VIEW_MAX_US.swap(0, Ordering::Relaxed);
    let un = UPDATE_N.swap(0, Ordering::Relaxed);
    let us = UPDATE_SUM_US.swap(0, Ordering::Relaxed);
    let um = UPDATE_MAX_US.swap(0, Ordering::Relaxed);
    if vn == 0 && un == 0 {
        return;
    }
    tracing::debug!(
        "perf: view×{} 平均{:.1}ms 峰值{:.1}ms | update×{} 平均{:.1}ms 峰值{:.1}ms",
        vn,
        vs as f64 / vn.max(1) as f64 / 1000.0,
        vm as f64 / 1000.0,
        un,
        us as f64 / un.max(1) as f64 / 1000.0,
        um as f64 / 1000.0
    );
}
