<div align="center">

# 🎬 LianWall

智能动态壁纸管理器 - 基于黄金角算法的壁纸轮换系统

[![Version](https://img.shields.io/badge/version-5.1.1-blue.svg)](https://github.com/Yueosa/lianwall/releases)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Wayland-blueviolet.svg)](https://wayland.freedesktop.org/)

**适用于所有支持 Wayland 的类 Unix 系统**（Linux / BSD 等）

**关于项目的大版本更新日志, 你可以查看 [CHANGELOG](./CHANGELOG.md)**

</div>

> **有记忆的智能壁纸轮换器** 🎯
> - 使用 **黄金角算法** 均匀遍历所有壁纸，避免重复
> - 刚播放过的壁纸自动进入"冷却队列"，短期内不会重复
> - 支持 **prev** 命令回退到上一张壁纸（历史栈）
> - **时间段目录**：通过文件夹命名控制壁纸在特定时间出现
> - **守护进程架构**：CLI 与 Daemon 分离，Unix Socket 通信
> - 支持 **动态壁纸**（视频/mpvpaper）和 **静态壁纸**（图片/swww）
> - 模式切换时自动播放新壁纸，无需手动 next
> - 显存监控：游戏高占用时自动降级为静态壁纸

---

## 📦 安装

### 说明

你会看到两个二进制文件 `lianwalld (lianwall-daemonm)` 和 `lianwall`

* 其中 `lianwalld` 是引擎核心, 负责真正的壁纸播放, 轮换
* 而 `lianwall` 是一个简单的命令行程序, 你可以通过他来控制 `lianwalld` 的各种行为, 例如:
    * 快速启动/退出
    * 切换壁纸/模式
    * ...

如果你不喜欢在命令行敲指令, 我们也提供了图形化界面程序

你可以在 [这里](https://github.com/Yueosa/lianwall-gui) 下载他

### 依赖

| 依赖 | 用途 | 安装 (Arch Linux) | 必需 |
|------|------|------|------|
| [mpvpaper](https://github.com/GhostNaN/mpvpaper) | 动态壁纸引擎 | `paru -S mpvpaper` | ✓ |
| [swww](https://github.com/LGFae/swww) | 静态壁纸引擎 | `paru -S swww` | ✓ |
| nvidia-smi | NVIDIA 显存检测 | 随驱动安装 | ✗ |
| rocm-smi | AMD 显存检测 | `pacman -S rocm-smi-lib` | ✗ |

### 编译

```bash
git clone https://github.com/Yueosa/lianwall.git
cd lianwall
cargo build --release

# 或者使用构建脚本
./build.sh
```

### 安装

```bash
# 安装两个二进制文件
cp target/release/lianwall ~/.local/bin/
cp target/release/lianwalld ~/.local/bin/
chmod +x ~/.local/bin/lianwall ~/.local/bin/lianwalld
```

或者使用一键安装脚本：

```bash
curl -fsSL https://raw.githubusercontent.com/Yueosa/lianwall/main/install.sh | bash
```

#### Arch Linux (AUR)

```bash
paru -S lianwall-bin   # 自动安装 lianwalld-bin 依赖
```

在 **AUR 安装** 将会自动安装所有依赖 `lianwalld` -> `swww` + `mpvpaper`

---

## 🚀 快速开始

### 1. 准备壁纸目录

默认路径（可在配置文件中修改）：
- 动态壁纸：`~/Videos/lianwall/`
- 静态壁纸：`~/Pictures/lianwall/`

### 2. 启动

```bash
lianwall start      # 启动守护进程（自动播放第一张壁纸）
lianwall status     # 查看状态
lianwall next       # 切换下一张
lianwall prev       # 回到上一张
```

### 3. 模式切换

```bash
lianwall switch          # 切换模式（Video ↔ Image）
lianwall mode video      # 切换到动态壁纸模式
lianwall mode image      # 切换到静态壁纸模式
```

切换模式后会**自动播放**对应模式的壁纸，无需手动 next。

---

## 🕐 时间段目录（高级功能）

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

### 命名格式

| 格式 | 示例 | 说明 |
|------|------|------|
| `HH-HH` | `08-18` | 8:00 到 18:00 |
| `HHMM-HHMM` | `0830-1745` | 8:30 到 17:45（精确到分钟） |
| 混合 | `08-1730` | 8:00 到 17:30 |
| 跨天 | `2300-0600` | 23:00 到次日 6:00 |
| 午夜 | `2200-2400` | 22:00 到 24:00（24:00 = 00:00） |

### 嵌套规则

- 子目录继承父目录的时间约束
- **所有约束必须同时满足**
- 例如 `08-18/1200-1330/` 下的壁纸只在 12:00~13:30 活跃

### 刷新时机

- 守护进程检测到**关键时间点**（每个时间范围的开始/结束）时自动刷新
- 刷新会重建向量空间，重置指针和冷却队列
- 使用 `lianwall status` 查看下一个刷新时间点

### 空壁纸处理

当某个时间段没有可用壁纸时，会清空当前壁纸显示（`swww clear` / 停止 `mpvpaper`）。

### Hyprland 用户配置

```conf
# ~/.config/hypr/hyprland.conf

# 随 Hyprland 启动
exec-once = lianwall start

# 快捷键
bind = ALT, S, exec, lianwall switch    # 切换模式
```

---

## 📖 命令参考

### 全局参数

| 参数 | 说明 |
|------|------|
| `--json` | JSON 格式输出（用于脚本/GUI 集成） |
| `--no-color` | 禁用彩色输出 |

### 生命周期

| 命令 | 说明 |
|------|------|
| `start` | 启动守护进程（`-F` 前台运行） |
| `stop` | 停止守护进程 |
| `restart` | 重启守护进程 |

### 壁纸控制

| 命令 | 说明 |
|------|------|
| `next` | 切换下一张壁纸 |
| `prev` | 回退上一张壁纸（从历史栈弹出） |
| `switch` | 切换模式（Video ↔ Image） |
| `mode <video\|image>` | 设置指定模式 |
| `set <PATH>` | 设置指定壁纸 |
| `lock <PATH>` | 锁定壁纸（不参与轮换） |
| `unlock <PATH>` | 解锁壁纸 |
| `toggle-lock <PATH>` | 切换壁纸锁定状态 |
| `reload` | 重新加载配置文件并重新扫描壁纸目录 |
| `rescan` | 仅重新扫描壁纸目录（不重载配置） |

### 状态查询

| 命令 | 说明 |
|------|------|
| `status` | 查看当前状态 |
| `space [--mode <video\|image>]` | 查看向量空间详情 |
| `time` | 查看时间段调度信息 |

### 配置管理

| 命令 | 说明 |
|------|------|
| `config show` | 显示完整配置 |
| `config get <KEY>` | 获取配置项（如 `paths.mode`） |
| `config set <KEY> <VALUE>` | 设置配置项 |
| `config reset` | 重置为默认配置 |

---

## 🏗️ 架构设计

LianWall 5.0 采用 **双文件 + 守护进程** 架构，CLI 和 Daemon 彻底分离：

| 二进制 | 用途 | 说明 |
|--------|------|------|
| `lianwall` | CLI 客户端 | 轻量级命令行工具，通过 Socket 与 Daemon 通信 |
| `lianwalld` | 守护进程 | 常驻后台，管理壁纸状态和定时任务 |

```
┌─────────────────────────────────────────────────────────────────────┐
│                           lianwall (CLI)                            │
│    lianwall start  →  自动查找并启动 lianwalld                       │
│    lianwall next   →  发送命令到 lianwalld                          │
│    lianwall status →  查询状态                                      │
└───────────────────────────────┬─────────────────────────────────────┘
                                │ Unix Socket (/tmp/lianwall.sock)
┌───────────────────────────────▼─────────────────────────────────────┐
│                          lianwalld (Daemon)                         │
│    • 内存维护壁纸状态（冷却队列、历史栈）                              │
│    • 定时切换壁纸                                                   │
│    • 监听时间点刷新                                                 │
│    • GPU 显存监控                                                   │
└───────────────────────────────┬─────────────────────────────────────┘
                                │
          ┌─────────────────────┼─────────────────────┐
          │                     │                     │
          ▼                     ▼                     ▼
     ┌─────────┐          ┌─────────┐        ┌────────────────┐
     │mpvpaper │          │  swww   │        │ GPU Monitoring │
     │ (Video) │          │ (Image) │        │     (VRAM)     │
     └─────────┘          └─────────┘        └────────────────┘
```

### 模块结构

```
lianwall/
├── crates/
│   ├── lianwall-core/      # 核心库（共享）
│   │   ├── algorithm/      # 黄金角选择算法
│   │   ├── config/         # 配置读写
│   │   ├── engine/         # mpvpaper / swww 引擎
│   │   ├── gpu/            # 显存监控
│   │   ├── socket/         # Unix Socket 通信协议 (V2)
│   │   └── wallpaper/      # 壁纸扫描、向量空间、时间段
│   │
│   ├── lianwall-cli/       # CLI 客户端 → lianwall
│   │   ├── client.rs       # Socket 客户端封装
│   │   ├── commands.rs     # 命令定义
│   │   └── handlers.rs     # 命令处理
│   │
│   └── lianwall-daemon/    # 守护进程 → lianwalld
│       ├── handler.rs      # 请求处理
│       ├── scheduler.rs    # 定时调度 (壁纸/GPU/时间点)
│       └── server.rs       # Socket 服务器
```

---

## 🧠 算法详解：黄金角遍历

LianWall 使用 **黄金角算法** 选择壁纸，这是自然界中向日葵种子排列所使用的角度，数学上证明是最均匀的遍历方式。

### 核心概念

**黄金角** = 2π × (1 - 1/φ) ≈ **137.508°** ≈ 2.4 弧度

其中 φ = (1 + √5) / 2 ≈ 1.618（黄金比例）

### 向量空间模型

每张壁纸在初始化时被分配一个固定角度：

```
wallpaper[0].angle = 0°
wallpaper[1].angle = 137.508°
wallpaper[2].angle = 275.016°
wallpaper[3].angle = 52.524°   (mod 360°)
...
```

系统维护一个**指针角度**，每次 `next` 时：
1. 找到距离指针最近的**未冷却、未锁定**壁纸
2. 将该壁纸加入冷却队列
3. 指针旋转一个黄金角

### 冷却队列

- 冷却值 = min(壁纸数/2, 7)
- 人的短期记忆约 5-7 个项目
- 冷却中的壁纸不会被选中

### 历史栈（prev 支持）

- 每次切换壁纸时将当前壁纸路径压入历史栈
- `prev` 从历史栈弹出，**支持跨模式播放**
- 历史栈存储在 daemon 层，与向量空间解耦
- 历史栈最大 100 条

---

## ⚙️ 配置文件

配置文件路径：`~/.config/lianwall/config.toml`

首次运行时自动创建默认配置。

```toml
# === 路径与模式 ===
[paths]
mode = "Video"                           # 启动模式: "Video" 或 "Image"
video_dir = "~/Videos/lianwall"          # 动态壁纸目录
image_dir = "~/Pictures/lianwall"        # 静态壁纸目录

# === 动态壁纸引擎 (mpvpaper) ===
[video_engine]
interval = 600                           # 自动切换间隔（秒），0 = 禁用
display = "*"                            # 目标显示器（"*" = 所有）
mpvpaper_args = []                       # 透传给 mpvpaper 的参数
mpv_args = [                             # 透传给 mpv 的参数
    "--no-audio",
    "--loop=inf",
    "--hwdec=auto",
    "--video-zoom=0",
    "--panscan=1.0"
]

# === 静态壁纸引擎 (swww) ===
[image_engine]
interval = 600                           # 自动切换间隔（秒）
outputs = ""                             # 目标显示器（空 = 所有）
swww_args = [                            # 透传给 swww img 的参数
    "--transition-type=fade",
    "--transition-duration=2.0",
    "--transition-fps=60",
    "--transition-step=20",
    "--resize=crop"
]

# === 显存监控 ===
[vram]
enabled = true                           # 是否启用
threshold_percent = 25.0                 # 降级阈值（显存剩余 < 25%）
recovery_percent = 40.0                  # 恢复阈值（显存剩余 > 40%）
check_interval = 2                       # 检测间隔（秒）
cooldown_seconds = 30                    # 降级冷却时间

# === 守护进程 ===
[daemon]
socket_path = "/tmp/lianwall.sock"       # Socket 路径
pid_path = "/tmp/lianwall.pid"           # PID 文件路径
log_level = "info"                       # 日志级别
```

---

## 📁 数据文件

LianWall 在 `~/.cache/lianwall/` 存储运行时数据：

| 文件 | 用途 |
|------|------|
| `weights.json` | 壁纸状态（角度、锁定、冷却队列、播放历史） |

---

## 🔧 故障排除

### Daemon 无法启动

```bash
# 检查是否已在运行
pgrep -af "lianwalld"

# 查看详细日志（前台运行）
lianwall start -F
```

### swww 壁纸不显示

```bash
# 检查 swww-daemon 是否正常
swww query

# 手动启动 swww-daemon
swww-daemon &
```

### 显存监控不工作

```bash
# NVIDIA 用户
nvidia-smi

# AMD 用户
rocm-smi

# 如果命令不存在或不支持，在配置中禁用
# [vram]
# enabled = false
```

---

## 📜 License

MIT

---

<div align="center">

**Made with 💜 by [Lian](https://github.com/Yueosa)**

</div>
