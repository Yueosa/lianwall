//! Hook 配置结构和默认模板

use serde::{Deserialize, Serialize};

/// hooks.toml 顶层结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookConfig {
    /// 最大并发 hook 数（默认 8）
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent: usize,
    /// hook 规则列表
    #[serde(default)]
    pub hook: Vec<HookEntry>,
}

impl Default for HookConfig {
    fn default() -> Self {
        Self {
            max_concurrent: default_max_concurrent(),
            hook: vec![],
        }
    }
}

fn default_max_concurrent() -> usize {
    8
}

/// 单条 hook 规则
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookEntry {
    /// 标识名（可选，用于日志标识和 CLI list）
    #[serde(default)]
    pub name: Option<String>,

    /// 触发事件（必填）
    pub on: HookEvent,

    /// 要执行的 shell 命令（必填，通过 sh -c 执行）
    pub command: String,

    /// 模式过滤（可选）：仅在 video/image 模式时触发
    /// 只对 wallpaper_changed / mode_changed / space_updated 有效
    #[serde(default)]
    pub mode: Option<String>,

    /// Trigger 过滤（可选）：仅在特定触发原因时执行
    /// 只对 wallpaper_changed 有效
    /// 可选值: scheduled, manual_next, manual_prev, manual_set, mode_switch,
    ///         vram_downgrade, vram_upgrade, time_point_refresh, daemon_start
    #[serde(default)]
    pub trigger: Option<Vec<String>>,

    /// 超时秒数（可选，默认 10s）
    #[serde(default = "default_timeout")]
    pub timeout: u64,

    /// 是否启用（可选，默认 true）
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_timeout() -> u64 {
    10
}

fn default_enabled() -> bool {
    true
}

/// 可触发的 hook 事件类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookEvent {
    /// 壁纸切换完成
    WallpaperChanged,
    /// 模式切换
    ModeChanged,
    /// 壁纸空间变更（扫描/锁定）
    SpaceUpdated,
    /// 配置变更
    ConfigChanged,
    /// 显存降级/恢复
    VramChanged,
    /// 时间点到达
    TimePointReached,
    /// 错误发生
    Error,
    /// daemon 即将关闭
    DaemonShutdown,
}

impl std::fmt::Display for HookEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HookEvent::WallpaperChanged => write!(f, "wallpaper_changed"),
            HookEvent::ModeChanged => write!(f, "mode_changed"),
            HookEvent::SpaceUpdated => write!(f, "space_updated"),
            HookEvent::ConfigChanged => write!(f, "config_changed"),
            HookEvent::VramChanged => write!(f, "vram_changed"),
            HookEvent::TimePointReached => write!(f, "time_point_reached"),
            HookEvent::Error => write!(f, "error"),
            HookEvent::DaemonShutdown => write!(f, "daemon_shutdown"),
        }
    }
}

impl HookEntry {
    /// 获取显示名称
    pub fn display_name(&self) -> String {
        self.name
            .clone()
            .unwrap_or_else(|| format!("hook({})", self.on))
    }
}

/// 默认 hooks.toml 文件内容（带完整注释说明）
pub const DEFAULT_HOOKS_TOML: &str = r#"# ============================================================================
# LianWall Hooks Configuration
# ============================================================================
#
# 事件驱动的用户脚本执行。当 daemon 发生特定事件时自动执行 shell 命令。
# 每条 hook 在独立子进程中执行（sh -c），不影响 daemon 运行。
#
# 配置热更新: `lianwall hook reload` 可在不重启 daemon 的情况下重载此文件。
#
# ============================================================================
# [[hook]] 配置字段说明
# ============================================================================
#
#   name     (string, 可选)  标识名，用于日志和 `lianwall hook list` 显示
#   on       (string, 必填)  触发事件，可选值见下方【事件列表】
#   command  (string, 必填)  Shell 命令，通过 sh -c 执行，支持环境变量
#   mode     (string, 可选)  模式过滤: "video" / "image"，仅对部分事件有效
#   trigger  (array,  可选)  触发原因过滤，仅对 wallpaper_changed 有效
#   timeout  (int,    可选)  超时秒数，默认 10，超时后进程会被杀死
#   enabled  (bool,   可选)  是否启用，默认 true
#
# ============================================================================
# 顶层配置字段说明
# ============================================================================
#
#   max_concurrent  (int, 可选)  最大同时运行的 hook 数，默认 8
#                               超出时后续 hook 排队等待，不会丢失
#                               若你的 hook 都是快速命令（notify-send 等），8 完全够用
#                               若有多个长耗时 hook（如 pywal），可适当调大
#
# 示例: max_concurrent = 4
#
# ============================================================================
# 事件列表 (on)
# ============================================================================
#
#   wallpaper_changed  壁纸切换完成
#     环境变量:
#       $LIANWALL_PATH       壁纸完整路径
#       $LIANWALL_FILENAME   壁纸文件名
#       $LIANWALL_MODE       当前模式 (video/image)
#       $LIANWALL_TRIGGER    触发原因 (scheduled/manual_next/manual_prev/
#                            manual_set/mode_switch/vram_downgrade/
#                            vram_upgrade/time_point_refresh/daemon_start)
#
#   mode_changed       模式切换
#     环境变量:
#       $LIANWALL_MODE_FROM  切换前模式 (video/image)
#       $LIANWALL_MODE_TO    切换后模式 (video/image)
#
#   space_updated      壁纸空间更新（扫描/锁定/解锁）
#     环境变量:
#       $LIANWALL_SPACE_MODE    更新的模式 (video/image)
#       $LIANWALL_SPACE_REASON  原因 (rescanned/lock_changed/time_point_refresh/config_changed)
#       $LIANWALL_TOTAL         壁纸总数
#       $LIANWALL_AVAILABLE     可用数量
#
#   config_changed     配置变更
#     环境变量:
#       $LIANWALL_CONFIG_KEY  变更的配置键
#
#   vram_changed       显存状态变化
#     环境变量:
#       $LIANWALL_VRAM_ACTION    动作 (downgrade/upgrade)
#       $LIANWALL_VRAM_USED_MB   已用显存 (MB)
#       $LIANWALL_VRAM_FREE_PCT  剩余百分比
#
#   time_point_reached 时间点到达
#     环境变量:
#       $LIANWALL_TIME       当前时间点 (HH:MM)
#       $LIANWALL_NEXT_TIME  下一个时间点 (HH:MM)，无则为空
#
#   error              错误发生
#     环境变量:
#       $LIANWALL_ERROR_MSG  错误信息
#
#   daemon_shutdown    daemon 即将关闭（在清理操作前执行，超时 5s）
#     环境变量: 无
#
# ============================================================================
# trigger 过滤可选值（仅 wallpaper_changed 有效）
# ============================================================================
#
#   scheduled, manual_next, manual_prev, manual_set, mode_switch,
#   vram_downgrade, vram_upgrade, time_point_refresh, daemon_start
#
# ============================================================================
# 示例
# ============================================================================

# --- 示例: 壁纸切换后发送通知 ---
# [[hook]]
# name = "notify-wallpaper"
# on = "wallpaper_changed"
# command = "notify-send 'LianWall' \"$LIANWALL_FILENAME\""

# --- 示例: 仅图片壁纸切换时执行 pywal ---
# [[hook]]
# name = "pywal"
# on = "wallpaper_changed"
# mode = "image"
# trigger = ["scheduled", "manual_next"]
# command = "wal -i \"$LIANWALL_PATH\" -n"

# --- 示例: 模式切换时记录日志 ---
# [[hook]]
# name = "log-mode"
# on = "mode_changed"
# command = "echo \"$(date '+%H:%M:%S') $LIANWALL_MODE_FROM → $LIANWALL_MODE_TO\" >> /tmp/lianwall-mode.log"

# --- 示例: 扫描完成后通知 ---
# [[hook]]
# name = "scan-notify"
# on = "space_updated"
# command = "notify-send 'LianWall' \"Space updated: $LIANWALL_TOTAL wallpapers ($LIANWALL_AVAILABLE available)\""

# --- 示例: daemon 关闭前清理 ---
# [[hook]]
# name = "cleanup"
# on = "daemon_shutdown"
# command = "rm -f /tmp/lianwall-*.log"
# timeout = 5
"#;
