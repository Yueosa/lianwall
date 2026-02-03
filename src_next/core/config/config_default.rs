/// 默认配置模板（TOML）
///
/// 说明：此处仅定义默认配置文本，真正的解析与加载逻辑在 config.rs 实现。
///       后续需要调整配置结构时，优先改动这里的模板以保持一致。

pub const DEFAULT_CONFIG_TOML: &str = r#"# ===================================
# LianWall 配置文件, 默认路径: ~/.config/lianwall/config.toml
# ===================================

# === 路径与模式 ===
[paths]
# 模式: Video 或 Image
mode = "Video"

# 动态壁纸目录
video_dir = "~/Videos/lianwall"

# 静态壁纸目录
image_dir = "~/Pictures/lianwall"

# === 动态壁纸 (mpvpaper) ===
[video_engine]
# 切换间隔（秒）
interval = 600

# 透传给 mpv 的参数（mpvpaper 的 -o 选项内容）
# 默认值优化：硬解码、无黑边填充、静音循环
mpv_args = [
    "--no-audio",
    "--loop=inf",
    "--hwdec=auto",
    "--video-zoom=0",
    "--panscan=1.0"
]

# === 静态壁纸 (swww) ===
[image_engine]
# 切换间隔（秒）
interval = 600

# 透传给 swww img 的参数
# 可选的过渡效果: none, simple, fade, left, right, top, bottom, wipe, wave, grow, center, any, outer, random
swww_args = [
    "--transition-type=fade",
    "--transition-duration=2.0",
    "--transition-fps=60",
    "--transition-step=20",
    "--resize=crop"
]

# === 权重算法 ===
[weight]
# 基础权重
base = 100.0
# 选中惩罚值
select_penalty = 10.0
# 扰动比例
perturbation_ratio = 0.03
# 容差阈值 (用于权重接近时的随机化)
tolerance = 1.0
# 归一化阈值
normalization_threshold = 500.0
# 归一化目标
normalization_target = 100.0
# 洗牌周期 (0 表示禁用)
shuffle_period = 100
# 洗牌强度 (0.0 - 1.0)
shuffle_intensity = 0.1

# === 显存检测 ===
[vram]
# 是否启用显存检测
enabled = true
# 显存剩余低于此百分比触发降级
threshold_percent = 25.0
# 显存剩余高于此百分比触发恢复
recovery_percent = 40.0
# 检测间隔 (秒, 建议 2-5 秒以快速响应游戏场景)
check_interval = 2
"#;
