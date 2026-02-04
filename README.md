<div align="center">

# 🎬 LianWall

智能动态壁纸管理器 - 基于负反馈闭环调节的壁纸轮换系统

</div>

> **有记忆的智能壁纸轮换器** 🎯
> - 刚播放过的壁纸会"冷却"，短期内不会重复
> - 长期未播放的壁纸会"积攒期望"，逐渐获得出场机会
> - 支持 **动态壁纸**（视频）和 **静态壁纸**（图片）两种模式
> - 权重数据持久化，重启后继续之前的状态轮换
> - 显存监控：游戏时自动降级为静态壁纸

---

## 📦 安装

> 你可以直接下载 Releases 页面中编译好的二进制包

### 依赖

| 依赖 | 用途 | 安装 (Arch Linux) | 必需 |
|------|------|------|------|
| [mpvpaper](https://github.com/GhostNaN/mpvpaper) | 动态壁纸引擎 | `paru -S mpvpaper` | ✓ |
| [swww](https://github.com/LGFae/swww) | 静态壁纸引擎 | `paru -S swww` | ✓ |
| nvidia-smi | NVIDIA 显存检测 | 随驱动安装 | ✗ |
| rocm-smi | AMD 显存检测 | `pacman -S rocm-smi-lib` | ✗ |

#### 关于显存检测

显存检测功能用于在游戏等高显存占用场景自动降级为静态壁纸，**这是可选功能**：

| GPU 厂商 | 支持情况 | 说明 |
|----------|---------|------|
| **NVIDIA** | ✓ 完整支持 | 默认使用 `nvidia-smi`，可编译 `nvml` 版本获得更好性能 |
| **AMD** | ✓ 完整支持 | 使用 `rocm-smi` 命令 |
| **Intel** | ✗ 不支持 | 目前无可用工具 |

> **不支持的用户**：在配置文件中设置 `vram.enabled = false` 即可，不影响壁纸轮换的正常使用。

### 编译

```bash
# 标准编译
cargo build --release

# NVIDIA 用户可选：启用 NVML 原生库支持（需要 CUDA 运行时）
cargo build --release --features nvml
```

> **关于 nvml feature**：默认使用 `nvidia-smi` 命令获取显存信息（每次调用约 50-100ms）。启用 `nvml` feature 后使用 NVIDIA 原生库直接读取（<1ms），适合对性能敏感的场景。需要安装 CUDA 运行时。

### 安装

```bash
cp target/release/lianwall ~/.local/bin/
chmod +x ~/.local/bin/lianwall
```

---

## 🚀 快速开始

### 1. 初始化配置

```bash
lianwall diagnose   # 检查依赖，自动生成配置文件
```

配置文件位置：`~/.config/lianwall/config.toml`

### 2. 准备壁纸目录

默认路径（可在配置文件中修改）：
- 动态壁纸：`~/Videos/lianwall/`
- 静态壁纸：`~/Pictures/lianwall/`

#### 🕐 时间段目录（高级功能）

通过文件夹命名来控制壁纸在特定时间段出现，**支持无限嵌套**：

```
~/Videos/lianwall/
├── 常规壁纸.mp4              # 全天候可用
├── 08-18/                    # 8:00 ~ 18:00 可用
│   ├── 工作日壁纸1.mp4
│   └── 1200-1330/            # 嵌套：12:00 ~ 13:30 可用
│       └── 午休壁纸.mp4
├── 2200-2400/                # 22:00 ~ 24:00（午夜）可用
│   └── 夜间壁纸.mp4
└── 2300-0600/                # 23:00 ~ 次日 06:00（跨天）可用
    └── 深夜壁纸.mp4
```

**命名格式**：
| 格式 | 示例 | 说明 |
|------|------|------|
| `HH-HH` | `08-18` | 8:00 到 18:00 |
| `HHMM-HHMM` | `0830-1745` | 8:30 到 17:45（精确到分钟） |
| 混合 | `08-1730` | 8:00 到 17:30 |
| 跨天 | `2200-0600` | 22:00 到次日 6:00 |
| 午夜 | `2200-2400` | 22:00 到 24:00（午夜） |

**刷新时机**：
- 守护进程运行时，**每分钟自动刷新**活跃列表
- 运行 `lianwall reload` 手动刷新
- 运行 `lianwall list` 实时显示当前时间段的活跃壁纸

### 3. 启动

```bash
lianwall start      # 启动守护进程
lianwall status     # 查看状态
lianwall next       # 切换下一张
```

### 特别的: 如果你是 Hyprland 用户

可以参考这一套 启动/快捷键 配置

```conf
# ~/.config/hypr/hyprland.conf

# 随 Hyprland 启动
exec-once = lianwall start

# 快捷键
bind = SUPER ALT, S, exec, lianwall next      # 下一张壁纸
bind = SUPER ALT, D, exec, lianwall switch    # 切换 Video/Image 模式
```

---

## 📖 命令参考

### 全局参数

| 参数 | 说明 |
|------|------|
| `--json` | JSON 格式输出（用于 GUI 集成） |

### 命令列表

| 命令 | 说明 |
|------|------|
| `start` | 启动守护进程 |
| `stop` | 停止守护进程 |
| `next` | 切换下一张壁纸 |
| `switch` | 切换模式（Video ↔ Image） |
| `reload` | 热重载壁纸目录 |
| `status` | 查询当前状态 |
| `list` | 列出壁纸（`--filter all/active/locked`） |
| `lock <PATH>` | 锁定壁纸，不再参与轮换 |
| `unlock <PATH>` | 解锁壁纸 |
| `stats` | 统计信息 |
| `time-ranges` | 列出时间段目录 |
| `diagnose` | 系统诊断 |

### 配置命令

| 命令 | 说明 |
|------|------|
| `config get <KEY>` | 获取配置项（如 `weight.base`） |
| `config set <KEY> <VALUE>` | 设置配置项 |
| `config show` | 显示完整配置 |
| `config reset` | 重置为默认配置（`-y` 跳过确认） |

---

## ⚙️ 配置文件

### 配置文件生成规则

配置文件路径：`~/.config/lianwall/config.toml`

**自动生成时机**：
- 运行任意命令时，如果配置文件不存在，会自动创建默认配置
- 运行 `lianwall diagnose` 会检查并创建配置文件
- 运行 `lianwall config reset` 会删除当前配置并重新生成默认配置

### 完整配置说明

```toml
# ===================================
# LianWall 配置文件
# 路径: ~/.config/lianwall/config.toml
# ===================================

# === 路径与模式 ===
[paths]
# 启动模式: "Video"（动态壁纸）或 "Image"（静态壁纸）
mode = "Video"

# 动态壁纸目录（支持 ~ 展开）
video_dir = "~/Videos/lianwall"

# 静态壁纸目录
image_dir = "~/Pictures/lianwall"


# === 动态壁纸引擎 (mpvpaper) ===
[video_engine]
# 自动切换间隔（秒），0 表示不自动切换
interval = 600

# 透传给 mpv 的参数（通过 mpvpaper 的 -o 选项）
# 默认参数解释：
#   --no-audio       静音播放
#   --loop=inf       无限循环
#   --hwdec=auto     自动硬件解码（推荐）
#   --video-zoom=0   不缩放
#   --panscan=1.0    填充模式，1.0 = 完全填充（可能裁剪边缘）
mpv_args = [
    "--no-audio",
    "--loop=inf",
    "--hwdec=auto",
    "--video-zoom=0",
    "--panscan=1.0"
]


# === 静态壁纸引擎 (swww) ===
[image_engine]
# 自动切换间隔（秒）
interval = 600

# 透传给 swww img 的参数
# 过渡效果可选值：
#   none, simple, fade, left, right, top, bottom,
#   wipe, wave, grow, center, any, outer, random
# 默认参数解释：
#   --transition-type=fade     淡入淡出效果
#   --transition-duration=2.0  过渡时长 2 秒
#   --transition-fps=60        过渡动画帧率
#   --transition-step=20       过渡步进值
#   --resize=crop              裁剪填充（保持比例）
swww_args = [
    "--transition-type=fade",
    "--transition-duration=2.0",
    "--transition-fps=60",
    "--transition-step=20",
    "--resize=crop"
]


# === 权重算法配置 ===
[weight]
# 权重区间 [weight_min, weight_max]
# 所有壁纸的权重将被约束在此区间内，精确到 2 位小数
# 范围越大，壁纸间的区分度越高
weight_min = 50.0
weight_max = 150.0

# 选中惩罚：壁纸被选中后扣除的权重值
# 建议值：(weight_max - weight_min) / (壁纸数量 * 0.4)
# 增大：被选中后"冷却"更久，轮换周期更长
select_penalty = 5.0

# Top-N 百分比：选择时只考虑权重最高的前 N% 壁纸
# 范围: 0.1 ~ 1.0，值越小选择越集中于高权重壁纸
top_n_percent = 0.25

# 哈希混合字节数（0-8）
# 控制选择的随机性：0 = 完全确定性，8 = 完全随机
# 推荐 3-5，兼顾随机性和可预测性
hash_mix_bytes = 4

# Seed 重置周期（小时）
# 每 N 小时重置随机种子，影响选择倾向
# 0 = 每次选择都重置（最随机），建议 4-12
seed_reset_hours = 6

# 归一化阈值：当平均权重超过此值时触发归一化
# 建议设为 weight_max * 1.2
normalization_threshold = 180.0

# 归一化目标：归一化后的目标平均权重
# 建议设为 (weight_min + weight_max) / 2
normalization_target = 100.0

# 洗牌周期：每 N 次选择后随机重置部分壁纸权重
# 0 = 禁用，建议 50-200
shuffle_period = 100

# 洗牌强度：每次洗牌重置的壁纸比例（0.0 ~ 1.0）
# 0.15 = 15% 的壁纸被重置到中间权重
shuffle_intensity = 0.15


# === 显存监控 ===
[vram]
# 是否启用显存监控
# 不支持的 GPU（如 Intel）请设为 false
enabled = true

# 降级阈值：显存剩余低于此百分比时切换到 Image 模式
# 例如：游戏占用大量显存时自动降级
threshold_percent = 25.0

# 恢复阈值：显存剩余高于此百分比时恢复 Video 模式
# 注意：需要大于 threshold_percent 以避免频繁切换
recovery_percent = 40.0

# 检测间隔（秒）
# 建议 2-5 秒，过短会增加 CPU 开销
check_interval = 2
```

---

## 📁 缓存文件

LianWall 在 `~/.cache/lianwall/` 目录下存储运行时数据：

| 文件 | 用途 | 说明 |
|------|------|------|
| `state.json` | 运行时状态 | 当前模式、壁纸、切换计数等，跨进程持久化 |
| `video_weights.json` | Video 模式权重 | 每张视频壁纸的权重、跳过次数、上次播放时间 |
| `image_weights.json` | Image 模式权重 | 每张图片壁纸的权重数据 |

### 权重文件格式

```json
[
  {
    "path": "/home/user/Videos/lianwall/wallpaper1.mp4",
    "value": 95.50,          // 当前权重（精确到 2 位小数）
    "skip_streak": 3,        // 连续未被选中次数
    "last_played": 1706918400, // 上次播放的 Unix 时间戳
    "content_hash": 12345678901234567890, // 文件内容哈希（用于选择算法）
    "locked": false          // 是否被锁定
  }
]
```

> **提示**：删除缓存文件可重置所有权重状态。权重数据会在每次壁纸切换后自动保存。

---

## 🧠 算法详解

LianWall 使用 **内容哈希初始化 + 零和博弈更新 + Top-N 哈希亲和度选择** 算法。

### 初始化机制：内容哈希均匀分布

新壁纸的初始权重计算方式：

```
content_hash = hash(文件前 64KB 内容 + 文件路径)
hash_ratio = content_hash / u64::MAX  // 映射到 [0, 1]
初始权重 = weight_min + hash_ratio × (weight_max - weight_min)
```

**示例**（weight_min=50, weight_max=150）：
- 哈希值较小的文件 → 权重接近 50
- 哈希值较大的文件 → 权重接近 150
- 权重均匀分布在整个区间

**设计目的**：
- **确定性分散**：同一批文件始终获得相同的初始权重分布，不依赖添加顺序或时间
- **均匀覆盖**：哈希值的均匀性确保权重不会集中在某个区间
- **快速计算**：只读取文件前 64KB，对大文件友好

### 核心机制一：零和博弈权重更新

每次选择壁纸后，系统进行权重更新：

```
被选中的壁纸:  权重 -= select_penalty (如 -5)
未被选中的壁纸: 权重 += select_penalty / (总数-1)
所有权重约束在 [weight_min, weight_max] 区间内
```

**零和特性**：惩罚值完全分配给其他壁纸，总权重趋于稳定。

**效果**：
- 刚播放过的壁纸权重降低，短期内难以再次被选中
- 长期未播放的壁纸权重逐渐累积，最终获得出场机会
- 形成自然的轮换周期，避免重复

### 核心机制二：Top-N + 哈希亲和度选择

选择壁纸时的三步流程：

#### Step 1: 筛选 Top-N 候选
```
按权重降序排序
候选集 = 前 top_n_percent 的壁纸（如前 25%）
```

**目的**：只考虑高权重壁纸，确保权重仍是主要因素。

#### Step 2: 计算混合哈希
```
mixed_hash = (前 hash_mix_bytes 字节: content_hash XOR system_seed)
           | (后 8-hash_mix_bytes 字节: content_hash)
```

**示例**（hash_mix_bytes=4）：
- 前 4 字节与种子异或 → 随机性
- 后 4 字节保持原哈希 → 确定性

**目的**：平衡随机性和可预测性。

#### Step 3: 选择最亲和的壁纸
```
选中 = 候选集中 (mixed_hash XOR system_seed) 最小的壁纸
```

**目的**：
- 种子相同时，倾向选择同一张壁纸（有记忆）
- 种子变化后，选择倾向自然改变（有新鲜感）

### 系统种子管理

```
seed_reset_hours = 6 时:
- 每 6 小时重置一次系统种子
- 同一时段内选择倾向稳定
- 跨时段后选择倾向自然变化

seed_reset_hours = 0 时:
- 每次选择都重置种子
- 完全随机模式
```

### 辅助机制：周期洗牌

每 `shuffle_period` 次选择后，随机重置 `shuffle_intensity` 比例的壁纸权重：

```
被重置的壁纸: 权重 = (weight_min + weight_max) / 2 ± 25%
```

**目的**：
- 打破可能形成的"生态锁定"
- 给长期低权重壁纸"翻身"机会
- 保持系统长期活力

### 辅助机制：自动归一化

当平均权重超过 `normalization_threshold` 时：

```
所有权重 = 所有权重 × (normalization_target / 当前平均权重)
```

**目的**：防止权重数值漂移过大。

---

## 🏗️ 项目架构

```
Core → API → CLI
│
├── algorithm/   # 权重计算、选择算法
├── config/      # 配置读写
├── engine/      # mpvpaper / swww 引擎
├── gpu/         # 显存监控
├── manager/     # 核心管理器
├── runtime/     # 调度器、状态机
└── wallpaper/   # 扫描器、时间范围计算
```

---

## 📜 License

MIT
