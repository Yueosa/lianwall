/// 默认配置模板（TOML）
///
/// 说明：此处仅定义默认配置文本，真正的解析与加载逻辑在 ops.rs 实现。
///       后续需要调整配置结构时，优先改动这里的模板以保持一致。

pub const DEFAULT_CONFIG_TOML: &str = r#"# ===================================
# LianWall 配置文件
# 默认路径: ~/.config/lianwall/config.toml
# ===================================

# === 路径与模式 ===
[paths]
# 运行模式: Video (动态壁纸) 或 Image (静态壁纸)
mode = "Video"

# 动态壁纸目录（支持 ~ 展开）
video_dir = "~/Videos/lianwall"

# 静态壁纸目录（支持 ~ 展开）
image_dir = "~/Pictures/lianwall"

# === 动态壁纸引擎 (mpvpaper) ===
# 官方文档: https://github.com/GhostNaN/mpvpaper
# mpvpaper 是基于 mpv 的 Wayland 动态壁纸播放器
[video_engine]
# 壁纸切换间隔（秒）
# 范围: 10 - 86400（10秒 ~ 24小时）
interval = 600

# 目标显示器
# - "*" = 所有已连接的显示器
# - 指定显示器名称如 "eDP-1"、"HDMI-A-1"
# 可通过 `wlr-randr` 或 `hyprctl monitors` 查看显示器名称
display = "*"

# 透传给 mpvpaper 的参数
# 常用选项:
#   -p          启动时暂停视频
#   -s          不显示 mpvpaper 的状态信息
#   -f          fork 到后台运行（由 lianwall 管理，通常不需要）
# 详见: mpvpaper --help
mpvpaper_args = []

# 透传给 mpv 的参数（通过 mpvpaper -o 传递）
# mpv 官方文档: https://mpv.io/manual/stable/
# 常用选项:
#   --no-audio          禁用音频
#   --loop=inf          无限循环
#   --hwdec=auto        自动硬件解码（推荐）
#   --video-zoom=0      视频缩放
#   --panscan=1.0       填充模式，避免黑边
mpv_args = [
    "--no-audio",
    "--loop=inf",
    "--hwdec=auto",
    "--video-zoom=0",
    "--panscan=1.0"
]

# === 静态壁纸引擎 (swww/awww) ===
# 请注意, swww包已改名为 awww
# 官方文档: https://github.com/LGFae/swww (awww)
# swww (awww) 是一个高效的 Wayland 壁纸设置工具，支持平滑过渡动画
[image_engine]
# 壁纸切换间隔（秒）
# 范围: 10 - 86400（10秒 ~ 24小时）
interval = 600

# 目标显示器
# - 空字符串 = 所有已连接的显示器
# - 逗号分隔多个显示器如 "eDP-1,HDMI-A-1"
# 可通过 `swww/awww query` 查看显示器名称
outputs = ""

# 透传给 swww/awww img 的参数
# 过渡效果类型 (--transition-type):
#   none, simple, fade, left, right, top, bottom,
#   wipe, wave, grow, center, any, outer, random
# 其他常用选项:
#   --transition-duration   过渡时长（秒）
#   --transition-fps        过渡帧率
#   --transition-step       过渡步长
#   --resize                缩放模式: crop, fit, no
# 详见: swww/awww img --help
swww_args = [
    "--transition-type=fade",
    "--transition-duration=2.0",
    "--transition-fps=60",
    "--transition-step=20",
    "--resize=crop"
]

# === 壁纸选择算法 ===
[algorithm]
# 当前仅支持：clockwise_biased_softmax
strategy = "clockwise_biased_softmax"

# 左侧候选惩罚强度；越大越偏向顺时针方向
bias_lambda = 0.35

# softmax 温度；越小越稳定，越大越随机
temperature = 0.28

# === 显存监控 ===
# 监控 GPU 显存使用，在显存不足时自动降级为静态壁纸
# 支持的 GPU: NVIDIA (nvidia-smi), AMD (rocm-smi), 以及用户自定义命令
[vram]
# 是否启用显存监控
# 禁用后将不会自动降级/恢复
enabled = true

# 后端选择: "auto" 或 "custom"
# - auto:   自动检测 nvidia-smi / rocm-smi（NVIDIA / AMD 用户使用此选项）
# - custom: 使用下方 custom_command 指定的命令（Intel 等其他 GPU 用户使用此选项）
backend = "auto"

# 自定义查询命令（backend = "custom" 时生效）
# 命令的 stdout 必须包含以下两行（顺序不限）：
#   used_mb=<整数MiB>
#   total_mb=<整数MiB>
#
# Intel 示例（需安装 intel_gpu_top）：
#   custom_command = "~/.config/lianwall/intel_vram.sh"
#
# NVIDIA 等效示例（可用于测试自定义路径）：
#   custom_command = "nvidia-smi --query-gpu=memory.used,memory.total --format=csv,noheader,nounits | awk -F', ' '{print \"used_mb=\"$1\"\\ntotal_mb=\"$2}'"
# custom_command = ""

# 降级阈值：显存剩余低于此百分比时，切换到静态壁纸
# 范围: 5.0 - 50.0（%）
# 建议: 20-30%，留足够余量给系统
threshold_percent = 25.0

# 恢复阈值：显存剩余高于此百分比时，恢复动态壁纸
# 范围: 20.0 - 80.0（%），必须大于 threshold_percent
# 建议: 比 threshold 高 10-20%，避免频繁切换
recovery_percent = 40.0

# 检测间隔（秒）
# 范围: 1 - 60
# 建议: 2-5 秒，快速响应游戏启动等场景
check_interval = 2

# 降级冷却时间（秒）
# 范围: 10 - 600
# 降级后在此时间内不会尝试恢复，防止显存波动导致频繁切换
# 建议: 30-60 秒
cooldown_seconds = 30

# === 守护进程 ===
[daemon]
# Unix Socket 路径，CLI 通过此路径与 daemon 通信
socket_path = "/tmp/lianwall.sock"

# PID 文件路径，用于防止重复启动
pid_path = "/tmp/lianwall.pid"

# 日志级别: error, warn, info, debug, trace
log_level = "info"
"#;
