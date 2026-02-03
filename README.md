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

### 依赖

| 依赖 | 用途 | 安装 (archlinux) |
|------|------|------|
| [mpvpaper](https://github.com/GhostNaN/mpvpaper) | 动态壁纸引擎 | `paru -S mpvpaper` |
| [swww](https://github.com/LGFae/swww) | 静态壁纸引擎 | `paru -S swww` |

### 编译

```bash
# 标准编译
cargo build --release

# 启用 NVIDIA NVML 原生库支持（可选，更精确的显存监控）
cargo build --release --features nvml
```

> **关于 nvml feature**：默认使用 `nvidia-smi` 命令获取显存信息。启用 `nvml` feature 后使用 NVIDIA 原生库，性能更好但需要安装 CUDA 运行时。AMD 用户无需关心此选项。

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

### 3. 启动

```bash
lianwall start      # 启动守护进程
lianwall status     # 查看状态
lianwall next       # 切换下一张
```

### 4. Hyprland 配置

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
| `--debug` | 启用 debug 追踪，显示完整调用链 |
| `--json` | JSON 格式输出 |

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

```toml
# ~/.config/lianwall/config.toml

# === 路径与模式 ===
[paths]
mode = "Video"                      # 启动模式: Video 或 Image
video_dir = "~/Videos/lianwall"     # 动态壁纸目录
image_dir = "~/Pictures/lianwall"   # 静态壁纸目录

# === 动态壁纸 (mpvpaper) ===
[video_engine]
interval = 600                      # 切换间隔（秒）
mpv_args = [                        # 透传给 mpv 的参数
    "--no-audio",
    "--loop=inf",
    "--hwdec=auto",
    "--panscan=1.0"                 # 1.0 = 填充屏幕（可能裁剪）
]

# === 静态壁纸 (swww) ===
[image_engine]
interval = 600
swww_args = [                       # 透传给 swww img 的参数
    "--transition-type=fade",
    "--transition-duration=2.0",
    "--resize=crop"
]

# === 权重算法 ===
[weight]
base = 100.0                        # 基础权重
select_penalty = 10.0               # 选中惩罚
perturbation_ratio = 0.03           # 扰动比例（±3%）
tolerance = 1.0                     # 容差阈值
normalization_threshold = 500.0     # 归一化触发阈值
normalization_target = 100.0        # 归一化目标
shuffle_period = 100                # 洗牌周期（0 禁用）
shuffle_intensity = 0.1             # 洗牌强度

# === 显存监控 ===
[vram]
enabled = true                      # 是否启用
threshold_percent = 25.0            # 显存剩余 < 25% 触发降级
recovery_percent = 40.0             # 显存剩余 > 40% 触发恢复
check_interval = 2                  # 检测间隔（秒）
```

---

## 🧠 算法简介

### 核心思想：零和博弈

每次选择壁纸后：
- **被选中**：权重 -10（进入冷却）
- **未选中**：均分 +10（积攒期望）
- **总权重守恒**：不会膨胀或通缩

### 关键机制

| 机制 | 作用 |
|------|------|
| **动态扰动** | 权重 ×3% 随机偏移，打破确定性循环 |
| **容差中位选择** | 权重接近时随机选择，避免总是选最高 |
| **周期洗牌** | 每 100 轮重置 10% 壁纸权重，防止生态锁定 |
| **自动归一化** | 平均权重 >500 时缩放，防止数值溢出 |

### 参数调优

| 场景 | `perturbation_ratio` | `shuffle_period` | 效果 |
|------|---------------------|-----------------|------|
| 均衡（默认） | 0.03 | 100 | 平衡记忆与随机 |
| 高随机性 | 0.05 | 50 | 快速轮换，多样性强 |
| 偏好固定 | 0.01 | 200 | 趋向固定序列 |

---

## 🏗️ 项目架构

```
CLI → API → Core
     │
     ├── algorithm/   # 权重计算、选择算法
     ├── config/      # 配置读写
     ├── engine/      # mpvpaper / swww 引擎
     ├── gpu/         # 显存监控
     ├── manager/     # 核心管理器
     ├── runtime/     # 调度器、状态机
     └── wallpaper/   # 扫描器、时间范围
```

---

## 📜 License

MIT
