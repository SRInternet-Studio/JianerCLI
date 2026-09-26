#![allow(dead_code)]

//! 终端输出helper：颜色、状态行、表格。
//!
//! 所有面向用户的输出都走这里，保证风格一致，并且统一遵守
//! `NO_COLOR` / `--no-color` / 非 TTY 的降级规则。

use std::io::{self, IsTerminal};

use owo_colors::OwoColorize;

/// 全局输出设置。
#[derive(Debug, Clone, Copy)]
pub struct Style {
    /// 是否启用颜色。
    pub color: bool,
    /// 是否静默（只输出错误）。
    pub quiet: bool,
    /// 是否输出 JSON（供脚本消费，关闭所有装饰）。
    pub json: bool,
}

impl Style {
    /// 根据命令行参数与终端能力推导输出风格。
    pub fn detect(no_color: bool, quiet: bool, json: bool) -> Self {
        let color = !no_color
            && !json
            && std::env::var_os("NO_COLOR").is_none()
            && io::stdout().is_terminal();
        Style { color, quiet, json }
    }

    /// 信息行（默认可见）。
    pub fn info(&self, msg: impl AsRef<str>) {
        if self.quiet || self.json {
            return;
        }
        println!("{}", msg.as_ref());
    }

    /// 成功行：绿色 ✓ 前缀。
    pub fn success(&self, msg: impl AsRef<str>) {
        self.badge("✓", msg.as_ref(), BadgeKind::Success);
    }

    /// 警告行：黄色 ! 前缀。
    pub fn warn(&self, msg: impl AsRef<str>) {
        self.badge("!", msg.as_ref(), BadgeKind::Warn);
    }

    /// 错误行：红色 ✗ 前缀。即使 `--quiet` 也输出。
    pub fn error(&self, msg: impl AsRef<str>) {
        self.badge("✗", msg.as_ref(), BadgeKind::Error);
    }

    /// 提示行：蓝色 → 前缀，用于「下一步该做什么」。
    pub fn hint(&self, msg: impl AsRef<str>) {
        self.badge("→", msg.as_ref(), BadgeKind::Hint);
    }

    /// 次要说明行，缩进对齐，通常跟在某个状态行下方。
    pub fn detail(&self, msg: impl AsRef<str>) {
        if self.quiet || self.json {
            return;
        }
        println!("  {}", msg.as_ref().dimmed_hint(self.color));
    }

    fn badge(&self, symbol: &str, msg: &str, kind: BadgeKind) {
        if self.json {
            return;
        }
        if self.quiet && !matches!(kind, BadgeKind::Error) {
            return;
        }
        let head = if self.color {
            match kind {
                BadgeKind::Success => symbol.green().bold().to_string(),
                BadgeKind::Warn => symbol.yellow().bold().to_string(),
                BadgeKind::Error => symbol.red().bold().to_string(),
                BadgeKind::Hint => symbol.cyan().bold().to_string(),
            }
        } else {
            symbol.to_string()
        };
        eprintln!("{head} {msg}");
    }

    /// 分组标题，用于 `plugin show` 这类多段输出。
    pub fn section(&self, title: &str) {
        if self.quiet || self.json {
            return;
        }
        if self.color {
            println!("\n{}", title.bold().underline());
        } else {
            println!("\n{title}");
        }
    }

    /// 键值行，键固定宽度以便对齐。
    pub fn kv(&self, key: &str, value: impl AsRef<str>) {
        if self.quiet || self.json {
            return;
        }
        if self.color {
            println!("  {} {}", format!("{key}:").dimmed(), value.as_ref());
        } else {
            println!("  {key}: {}", value.as_ref());
        }
    }

    /// 给一段文本加上颜色（不换行输出时用）。
    pub fn paint_ok(&self, s: &str) -> String {
        if self.color {
            s.green().to_string()
        } else {
            s.to_string()
        }
    }

    /// 给一段文本加上警告色。
    pub fn paint_warn(&self, s: &str) -> String {
        if self.color {
            s.yellow().to_string()
        } else {
            s.to_string()
        }
    }

    /// 给一段文本加上强调色。
    pub fn paint_accent(&self, s: &str) -> String {
        if self.color {
            s.cyan().bold().to_string()
        } else {
            s.to_string()
        }
    }

    /// 弱化文本。
    pub fn paint_dim(&self, s: &str) -> String {
        if self.color {
            s.dimmed().to_string()
        } else {
            s.to_string()
        }
    }

    /// 输出 JSON 到 stdout（`--json` 模式下所有命令的统一出口）。
    pub fn emit_json(&self, value: &serde_json::Value) -> crate::error::Result<()> {
        let text = serde_json::to_string_pretty(value)
            .map_err(|e| crate::error::json_err("序列化输出", e))?;
        println!("{text}");
        Ok(())
    }
}

/// 渲染辅助：给 `dimmed` 提供条件包装，避免每个调用点都写 if。
trait ConditionalStyle {
    fn dimmed_hint(self, color: bool) -> String;
}

impl ConditionalStyle for &str {
    fn dimmed_hint(self, color: bool) -> String {
        if color {
            self.dimmed().to_string()
        } else {
            self.to_string()
        }
    }
}

#[derive(Clone, Copy)]
enum BadgeKind {
    Success,
    Warn,
    Error,
    Hint,
}

/// 打印一个简易表格（列宽按内容计算，CJK 字符按 2 列计）。
pub fn table(style: &Style, headers: &[&str], rows: &[Vec<String>]) {
    if style.quiet || style.json || rows.is_empty() {
        return;
    }
    let widths: Vec<usize> = headers
        .iter()
        .enumerate()
        .map(|(i, h)| {
            let body = rows
                .iter()
                .map(|r| display_width(r.get(i).map(String::as_str).unwrap_or("")))
                .max()
                .unwrap_or(0);
            display_width(h).max(body)
        })
        .collect();

    let mut head = String::new();
    for (i, h) in headers.iter().enumerate() {
        head.push_str(&pad(h, widths[i]));
        if i + 1 != headers.len() {
            head.push_str("  ");
        }
    }
    println!("{}", style.paint_dim(&head));

    for row in rows {
        let mut line = String::new();
        for (i, w) in widths.iter().enumerate() {
            let cell = row.get(i).map(String::as_str).unwrap_or("");
            line.push_str(&pad(cell, *w));
            if i + 1 != widths.len() {
                line.push_str("  ");
            }
        }
        println!("{}", line.trim_end());
    }
}

/// 计算字符串在终端里的显示宽度：CJK 与全角字符占 2 列。
pub fn display_width(s: &str) -> usize {
    s.chars().map(|c| if is_wide(c) { 2 } else { 1 }).sum()
}

fn is_wide(c: char) -> bool {
    matches!(c as u32,
        0x1100..=0x115F
        | 0x2E80..=0xA4CF
        | 0xAC00..=0xD7A3
        | 0xF900..=0xFAFF
        | 0xFE30..=0xFE6F
        | 0xFF00..=0xFF60
        | 0xFFE0..=0xFFE6
        | 0x1F300..=0x1F64F
        | 0x1F900..=0x1F9FF
        | 0x20000..=0x3FFFD)
}

fn pad(s: &str, width: usize) -> String {
    let w = display_width(s);
    if w >= width {
        s.to_string()
    } else {
        format!("{s}{}", " ".repeat(width - w))
    }
}

/// 把字节数格式化成人读形式。
pub fn human_bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = n as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{n} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}
