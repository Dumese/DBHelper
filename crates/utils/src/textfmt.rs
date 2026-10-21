//! 展示层格式化：体积、时长、数字、时间、文本截断。

use chrono::{DateTime, Local, TimeZone};

pub fn bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = n as f64;
    let mut idx = 0;
    while value >= 1024.0 && idx < UNITS.len() - 1 {
        value /= 1024.0;
        idx += 1;
    }
    if idx == 0 {
        format!("{n} {}", UNITS[idx])
    } else if value >= 100.0 {
        format!("{:.0} {}", value, UNITS[idx])
    } else if value >= 10.0 {
        format!("{:.1} {}", value, UNITS[idx])
    } else {
        format!("{:.2} {}", value, UNITS[idx])
    }
}

pub fn duration_ms(ms: u64) -> String {
    if ms < 1 {
        "<1ms".into()
    } else if ms < 1_000 {
        format!("{ms}ms")
    } else if ms < 60_000 {
        format!("{:.2}s", ms as f64 / 1000.0)
    } else {
        let m = ms / 60_000;
        let s = (ms % 60_000) / 1000;
        format!("{m}m{s}s")
    }
}

/// 千分位。小数位按原样保留。
pub fn thousands(input: &str) -> String {
    let (int_part, rest) = match input.find('.') {
        Some(pos) => (&input[..pos], &input[pos..]),
        None => (input, ""),
    };
    let negative = int_part.starts_with('-');
    let digits = int_part.trim_start_matches('-');
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    if negative {
        out.insert(0, '-');
    }
    format!("{out}{rest}")
}

pub fn ts(secs: i64) -> String {
    match Local.timestamp_opt(secs, 0).single() {
        Some(dt) => dt.format("%Y-%m-%d %H:%M").to_string(),
        None => "-".into(),
    }
}

pub fn ts_secs(secs: i64) -> String {
    match Local.timestamp_opt(secs, 0).single() {
        Some(dt) => dt.format("%Y-%m-%d %H:%M:%S").to_string(),
        None => "-".into(),
    }
}

pub fn clock(secs: i64) -> String {
    match Local.timestamp_opt(secs, 0).single() {
        Some(dt) => dt.format("%H:%M:%S").to_string(),
        None => "-".into(),
    }
}

pub fn now_secs() -> i64 {
    Local::now().timestamp()
}

pub fn relative(secs: i64) -> String {
    let diff = now_secs() - secs;
    if diff < 60 {
        "刚刚".into()
    } else if diff < 3_600 {
        format!("{} 分钟前", diff / 60)
    } else if diff < 86_400 {
        format!("{} 小时前", diff / 3_600)
    } else if diff < 2_592_000 {
        format!("{} 天前", diff / 86_400)
    } else {
        ts(secs)
    }
}

pub fn iso(secs: i64) -> String {
    match Local.timestamp_opt(secs, 0).single() {
        Some(dt) => dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        None => String::new(),
    }
}

/// 中间省略截断（保留首尾）。
pub fn ellipsis_middle(s: &str, max_chars: usize) -> String {
    let count = s.chars().count();
    if count <= max_chars {
        return s.to_string();
    }
    let head = max_chars.saturating_sub(2) / 2;
    let tail = max_chars.saturating_sub(2) - head;
    let start: String = s.chars().take(head).collect();
    let end: String = s.chars().skip(count - tail).collect();
    format!("{start}..{end}")
}

/// 末尾省略。
pub fn ellipsis_end(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// 超过阈值时压缩为 `1.2k` / `12.4k` / `2.1M`。
pub fn compact_count(n: u64) -> String {
    if n < 1_000 {
        n.to_string()
    } else if n < 1_000_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    }
}

pub fn pct(value: f64) -> String {
    format!("{:.0}%", value * 100.0)
}

pub fn date_time(dt: DateTime<Local>) -> String {
    dt.format("%Y-%m-%d %H:%M:%S").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_bytes() {
        assert_eq!(bytes(512), "512 B");
        assert_eq!(bytes(2048), "2.00 KB");
        assert_eq!(bytes(2 * 1024 * 1024), "2.00 MB");
    }

    #[test]
    fn formats_thousands() {
        assert_eq!(thousands("12480.00"), "12,480.00");
        assert_eq!(thousands("1234"), "1,234");
        assert_eq!(thousands("-1234567"), "-1,234,567");
    }
}
