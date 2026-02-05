# Changelog

## [4.0.0] - 2026-02-05

### 🎉 完全重构

LianWall 4.0 是一次**彻底的架构重写**，代码从零开始重新设计。

### ⚡ 核心变化

#### 算法革新：黄金角取代权重系统

| | v3.0.0 | v4.0.0 |
|---|--------|--------|
| **算法** | 零和博弈权重系统 | 黄金角算法 (137.508°) |
| **配置项** | 7 个权重参数需要调优 | **零配置**，开箱即用 |
| **复杂度** | 扰动、容差、归一化、洗牌... | 简单的圆周遍历 |
| **可预测性** | 概率性，难以预测 | 数学确定性，均匀遍历 |

**为什么改？** 权重系统虽然"智能"，但参数调优困难，用户反馈"不知道怎么配置"。黄金角是自然界（向日葵种子排列）证明的最优解，无需配置。

#### 架构革新：Daemon-Client 分离

| | v3.0.0 | v4.0.0 |
|---|--------|--------|
| **架构** | 单进程，每次命令独立运行 | Daemon 常驻 + CLI 轻量通信 |
| **状态管理** | 每次从文件重新加载 | Daemon 内存维护，实时响应 |
| **进程通信** | ❌ 无 | Unix Socket + JSON 协议 |
| **二进制** | 单文件 `lianwall` | 单文件 `lianwall`（内含 Daemon） |

**为什么改？** 
- v3.0.0 每次命令都要读取配置、扫描目录、加载权重，启动慢
- 没有真正的"守护进程"，无法维护运行时状态（如冷却队列、播放历史）
- 定时切换需要依赖外部 cron/systemd timer
- v4.0.0 的 Daemon 架构可以：
  - 内存维护状态，命令响应 <50ms
  - 内置定时器，自动切换壁纸
  - 支持 `prev` 回退（需要历史栈）
  - 支持时间段智能刷新（需要常驻监听）

#### 运行模式

```bash
# v3.0.0 - 单次执行
lianwall next                  # 每次独立运行，无后台进程
# 定时切换需要 cron/systemd timer

# v4.0.0 - Daemon 常驻
lianwall start                 # 启动 daemon（后台）
lianwall start -F              # 前台运行（调试用）
lianwall next                  # CLI 通过 socket 与 daemon 通信
```

### ✨ 新增功能

#### `prev` 命令 - 回退上一张

```bash
lianwall prev    # 回到上一张壁纸
```

- 维护播放历史栈（最大 100 条）
- 忽略锁定状态，强制播放历史壁纸

#### 模式切换自动播放

```bash
lianwall switch  # 切换模式后自动播放新壁纸
lianwall mode video
```

v3.0.0 切换模式后需要手动 `next`，v4.0.0 自动播放。

#### 时间段目录优化

```
~/Videos/lianwall/
├── 08-18/           # 8:00 ~ 18:00
│   └── 1200-1330/   # 嵌套：12:00 ~ 13:30
└── 2300-0600/       # 跨天支持
```

- **智能刷新**：不再每分钟扫描，改为检测关键时间点触发
- **冷却队列保护**：活跃壁纸数 ≤ 冷却大小时自动清空队列

#### GUI 友好的协议

新增 `GetTimeInfo` 请求，返回完整的时间调度信息：

```json
{
  "current_time": "14:30",
  "video_schedule": {
    "scanned_count": 50,
    "active_count": 20,
    "time_points": ["08:00", "12:00", "18:00"],
    "wallpaper_segments": [...]
  }
}
```

### 🗑️ 移除的功能

| 移除项 | 原因 |
|--------|------|
| `diagnose` 命令 | 架构简化后不再需要复杂诊断 |
| `list` 命令 | 用 `status --json` 替代 |
| `stats` 命令 | 合并到 `status` |
| 权重配置 (`[weight]`) | 黄金角算法无需配置 |
| NVML feature | nvidia-smi 已足够，无需额外依赖 |
| `video_weights.json` / `image_weights.json` | 合并为 `weights.json` |
| `state.json` | 状态由 Daemon 内存维护 |

### 📦 文件变化

```
# v3.0.0 缓存文件
~/.cache/lianwall/
├── state.json
├── video_weights.json
└── image_weights.json

# v4.0.0 缓存文件
~/.cache/lianwall/
└── weights.json        # 统一的持久化文件
```

### ⚙️ 配置文件变化

```toml
# v3.0.0 - 复杂的权重配置
[weight]
base = 100.0
select_penalty = 10.0
perturbation_ratio = 0.03
tolerance = 1.0
normalization_threshold = 500.0
normalization_target = 100.0
shuffle_period = 100
shuffle_intensity = 0.1

# v4.0.0 - 移除整个 [weight] 段
# 黄金角算法无需配置！
```

新增：
```toml
[daemon]
socket_path = "/tmp/lianwall.sock"
pid_path = "/tmp/lianwall.pid"
log_level = "info"
```

### 🔧 命令变化

| v3.0.0 | v4.0.0 | 说明 |
|--------|--------|------|
| `lianwall start` | `lianwall start` | ✓ 保留 |
| `lianwall stop` | `lianwall stop` | ✓ 保留 |
| `lianwall next` | `lianwall next` | ✓ 保留 |
| - | `lianwall prev` | ✨ 新增 |
| `lianwall switch` | `lianwall switch` | ✓ 保留（现在自动播放） |
| `lianwall reload` | `lianwall reload` | ✓ 保留 |
| `lianwall status` | `lianwall status` | ✓ 保留（输出格式变化） |
| `lianwall list` | - | ❌ 移除 |
| `lianwall stats` | - | ❌ 移除 |
| `lianwall diagnose` | - | ❌ 移除 |
| `lianwall config *` | `lianwall config *` | ✓ 保留 |

### 📊 对比

| 指标 | v3.0.0 | v4.0.0 |
|------|--------|--------|
| 架构 | 单次执行 | Daemon 常驻 |
| 状态 | 文件读写 (state.json) | 内存维护 |
| IPC | ❌ 无 | Unix Socket |
| 算法 | 权重计算 O(n) | 黄金角 O(1) |
| 历史回退 | ❌ 无 | ✅ `prev` 命令 |

### 🚀 升级指南

1. **停止旧版本**
   ```bash
   lianwall stop  # 如果 v3.0.0 正在运行
   ```

2. **替换二进制**
   ```bash
   cp lianwall_4.0.0_linux_x86_64 ~/.local/bin/lianwall
   chmod +x ~/.local/bin/lianwall
   ```

3. **清理旧缓存**（可选，v4.0.0 会自动创建新格式）
   ```bash
   rm -rf ~/.cache/lianwall/
   ```

4. **更新配置**（可选）
   ```bash
   # 移除 [weight] 段（不再需要）
   # 添加 [daemon] 段（使用默认值即可）
   lianwall config reset  # 或者直接重置
   ```

5. **启动新版本**
   ```bash
   lianwall start
   lianwall status
   ```

---

## [3.0.0] - 2026-02-03

- 零和博弈权重系统
- 时间段目录支持
- 显存监控与自动降级
