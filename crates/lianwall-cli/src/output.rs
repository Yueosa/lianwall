//! 终端输出格式化
//!
//! 支持:
//! - 终端能力检测 (颜色, emoji)
//! - 彩色/ASCII 降级输出
//! - JSON 输出模式
//! - 消息常量 (i18n ready)

use colored::{ColoredString, Colorize};
use std::env;

// ============================================================================
// Messages (i18n ready)
// ============================================================================

/// User-facing messages
///
/// TODO: i18n implementation plan
/// 1. Detect locale from LANG/LC_ALL environment variables
/// 2. Create locale modules: locales/en.rs, locales/zh.rs
/// 3. Load messages based on detected locale
/// 4. Fallback to English if locale not supported
///
/// Example future structure:
/// ```ignore
/// output/
/// ├── mod.rs          // Formatter + locale detection
/// ├── messages.rs     // Message trait + loader
/// └── locales/
///     ├── en.rs       // English (default)
///     └── zh.rs       // Simplified Chinese
/// ```
pub mod messages {
    // ==================== reload/rescan ====================
    pub const RELOADING: &str = "Reloading...";
    pub const RESCANNING: &str = "Rescanning...";
    pub const WAITING: &str = "Waiting...";
    pub const SCAN_HINT_LINE1: &str = "(daemon not responding, large wallpaper directory may take longer to scan)";
    pub const SCAN_HINT_LINE2: &str = "(press Ctrl+C to exit waiting, scan continues in background)";
    pub const TIMEOUT_WARNING: &str = "Timeout waiting for scan completion";
    pub const SCAN_BACKGROUND_HINT: &str = "Scan continues in background, use 'lianwall status' to check";
    
    // ==================== status ====================
    pub const DAEMON_RUNNING: &str = "lianwall daemon running";
    pub const DAEMON_NOT_RUNNING: &str = "Daemon is not running. Start it with: lianwall start";
    
    // ==================== common ====================
    pub const CANCELLED: &str = "Cancelled";
}

// ============================================================================
// Terminal Capabilities
// ============================================================================

/// 终端能力
#[derive(Debug, Clone, Copy)]
pub struct TerminalCaps {
    /// 支持颜色
    pub color: bool,
    /// 支持 emoji (UTF-8)
    pub emoji: bool,
}

impl TerminalCaps {
    /// 检测终端能力
    pub fn detect() -> Self {
        Self {
            color: detect_color_support(),
            emoji: detect_emoji_support(),
        }
    }

    /// 强制禁用颜色
    pub fn without_color(mut self) -> Self {
        self.color = false;
        self
    }
}

impl Default for TerminalCaps {
    fn default() -> Self {
        Self::detect()
    }
}

/// 检测颜色支持
fn detect_color_support() -> bool {
    // 检查 NO_COLOR 环境变量 (https://no-color.org/)
    if env::var("NO_COLOR").is_ok() {
        return false;
    }

    // 检查 TERM
    if let Ok(term) = env::var("TERM") {
        if term == "dumb" {
            return false;
        }
    }

    // 检查 COLORTERM
    if env::var("COLORTERM").is_ok() {
        return true;
    }

    // 检查是否是 TTY
    atty::is(atty::Stream::Stdout)
}

/// 检测 emoji 支持 (UTF-8)
fn detect_emoji_support() -> bool {
    // 检查 LANG 环境变量
    if let Ok(lang) = env::var("LANG") {
        if lang.to_uppercase().contains("UTF-8") || lang.to_uppercase().contains("UTF8") {
            return true;
        }
    }

    // 检查 LC_ALL
    if let Ok(lc) = env::var("LC_ALL") {
        if lc.to_uppercase().contains("UTF-8") || lc.to_uppercase().contains("UTF8") {
            return true;
        }
    }

    false
}

/// 输出格式化器
#[allow(dead_code)]
pub struct Formatter {
    caps: TerminalCaps,
    json_mode: bool,
}

#[allow(dead_code)]
impl Formatter {
    pub fn new(caps: TerminalCaps, json_mode: bool) -> Self {
        Self { caps, json_mode }
    }

    /// 是否 JSON 模式
    pub fn is_json(&self) -> bool {
        self.json_mode
    }

    // ========================================================================
    // 图标
    // ========================================================================

    pub fn icon_ok(&self) -> &'static str {
        if self.caps.emoji { "✓" } else { "[OK]" }
    }

    pub fn icon_err(&self) -> &'static str {
        if self.caps.emoji { "✗" } else { "[ERR]" }
    }

    pub fn icon_warn(&self) -> &'static str {
        if self.caps.emoji { "⚠" } else { "[WARN]" }
    }

    pub fn icon_info(&self) -> &'static str {
        if self.caps.emoji { "ℹ" } else { "[INFO]" }
    }

    pub fn icon_video(&self) -> &'static str {
        if self.caps.emoji { "🎬" } else { "[VIDEO]" }
    }

    pub fn icon_image(&self) -> &'static str {
        if self.caps.emoji { "🖼" } else { "[IMAGE]" }
    }

    pub fn icon_lock(&self) -> &'static str {
        if self.caps.emoji { "🔒" } else { "[LOCK]" }
    }

    pub fn icon_unlock(&self) -> &'static str {
        if self.caps.emoji { "🔓" } else { "[UNLOCK]" }
    }

    pub fn icon_running(&self) -> &'static str {
        if self.caps.emoji { "●" } else { "[*]" }
    }

    pub fn icon_stopped(&self) -> &'static str {
        if self.caps.emoji { "○" } else { "[ ]" }
    }

    // ========================================================================
    // 颜色
    // ========================================================================

    pub fn success(&self, text: &str) -> ColoredString {
        if self.caps.color {
            text.green()
        } else {
            text.normal()
        }
    }

    pub fn error(&self, text: &str) -> ColoredString {
        if self.caps.color {
            text.red()
        } else {
            text.normal()
        }
    }

    pub fn warning(&self, text: &str) -> ColoredString {
        if self.caps.color {
            text.yellow()
        } else {
            text.normal()
        }
    }

    pub fn info(&self, text: &str) -> ColoredString {
        if self.caps.color {
            text.cyan()
        } else {
            text.normal()
        }
    }

    pub fn dim(&self, text: &str) -> ColoredString {
        if self.caps.color {
            text.dimmed()
        } else {
            text.normal()
        }
    }

    pub fn bold(&self, text: &str) -> ColoredString {
        if self.caps.color {
            text.bold()
        } else {
            text.normal()
        }
    }

    pub fn highlight(&self, text: &str) -> ColoredString {
        if self.caps.color {
            text.bright_white().bold()
        } else {
            text.normal()
        }
    }

    // ========================================================================
    // 组合输出
    // ========================================================================

    /// 打印成功消息
    pub fn print_success(&self, message: &str) {
        println!("{} {}", self.success(self.icon_ok()), message);
    }

    /// 打印错误消息
    pub fn print_error(&self, message: &str) {
        eprintln!("{} {}", self.error(self.icon_err()), message);
    }

    /// 打印警告消息
    pub fn print_warning(&self, message: &str) {
        println!("{} {}", self.warning(self.icon_warn()), message);
    }

    /// 打印信息消息
    pub fn print_info(&self, message: &str) {
        println!("{} {}", self.info(self.icon_info()), message);
    }

    /// 打印键值对
    pub fn print_kv(&self, key: &str, value: &str) {
        println!("  {:<12}: {}", self.dim(key), value);
    }

    /// 打印分隔线
    pub fn print_separator(&self, title: &str) {
        println!();
        println!("  {}", self.bold(title));
        println!("  {}", "=".repeat(title.len()));
    }
}

// ============================================================================
// Formatting Functions
// ============================================================================

/// Format uptime duration
pub fn format_uptime(secs: u64) -> String {
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else if secs < 86400 {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    } else {
        format!("{}d {}h", secs / 86400, (secs % 86400) / 3600)
    }
}

/// Format countdown duration (for next switch)
pub fn format_countdown(secs: u64) -> String {
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        let mins = secs / 60;
        let s = secs % 60;
        if s == 0 {
            format!("{}m", mins)
        } else {
            format!("{}m {}s", mins, s)
        }
    } else {
        let hours = secs / 3600;
        let mins = (secs % 3600) / 60;
        if mins == 0 {
            format!("{}h", hours)
        } else {
            format!("{}h {}m", hours, mins)
        }
    }
}

/// Format file size (bytes -> human readable)
#[allow(dead_code)]
pub fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

/// Format VRAM usage
pub fn format_vram(used_mb: u64, total_mb: u64) -> String {
    if total_mb == 0 {
        return "N/A".to_string();
    }
    let free_percent = ((total_mb - used_mb) as f64 / total_mb as f64) * 100.0;
    format!("{}/{} MB ({:.0}% free)", used_mb, total_mb, free_percent)
}

/// Format VRAM status
pub fn format_vram_status(degraded: bool) -> &'static str {
    if degraded { "Degraded" } else { "Normal" }
}

/// Format wallpaper status indicators
pub fn format_wallpaper_flags(locked: bool, in_cooldown: bool, is_current: bool) -> String {
    let mut flags = Vec::new();
    if is_current {
        flags.push("*");
    }
    if locked {
        flags.push("L");
    }
    if in_cooldown {
        flags.push("C");
    }
    if flags.is_empty() {
        String::new()
    } else {
        format!("[{}]", flags.join(""))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_uptime() {
        assert_eq!(format_uptime(30), "30s");
        assert_eq!(format_uptime(90), "1m 30s");
        assert_eq!(format_uptime(3661), "1h 1m");
        assert_eq!(format_uptime(90000), "1d 1h");
    }

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(500), "500 B");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(1048576), "1.0 MB");
    }

    #[test]
    fn test_formatter_icons() {
        let caps = TerminalCaps { color: false, emoji: true };
        let fmt = Formatter::new(caps, false);
        assert_eq!(fmt.icon_ok(), "✓");

        let caps = TerminalCaps { color: false, emoji: false };
        let fmt = Formatter::new(caps, false);
        assert_eq!(fmt.icon_ok(), "[OK]");
    }
}
