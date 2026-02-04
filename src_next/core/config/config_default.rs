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
# 权重区间 [weight_min, weight_max]
# 所有壁纸的权重将被约束在此区间内
weight_min = 50.0
weight_max = 150.0

# 选中惩罚值：每次壁纸被选中后减少的权重
# 建议值：(weight_max - weight_min) / (壁纸数量 * 0.4)
select_penalty = 5.0

# Top-N 百分比：选择时只考虑权重最高的前 N% 壁纸
# 范围 0.1 - 1.0，值越小选择越集中于高权重壁纸
top_n_percent = 0.25

# 哈希混合字节数（0-8）
# 控制选择的随机性：0 = 完全确定性，8 = 完全随机
# 推荐 3-5，兼顾随机性和可预测性
hash_mix_bytes = 4

# Seed 重置周期（小时）
# 每 N 小时重置随机种子，影响选择倾向
# 0 = 每次选择都重置（最随机），建议 4-12
seed_reset_hours = 6

# 归一化阈值：当平均权重超过此值时，按比例缩放所有权重
# 防止权重无限增长，建议设为 weight_max * 1.2
normalization_threshold = 180.0

# 归一化目标值：归一化后的目标平均权重
# 建议设为 (weight_min + weight_max) / 2
normalization_target = 100.0

# 洗牌周期（选择次数）
# 每 N 次选择后，随机重置部分壁纸权重，打破"生态锁定"
# 0 = 禁用，建议 50-200
shuffle_period = 100

# 洗牌强度（0.0 - 1.0）
# 每次洗牌时重置的壁纸比例
# 0.1 = 10% 的壁纸被重置
shuffle_intensity = 0.15

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
