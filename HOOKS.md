# LianWall Hook 系统

Hook 系统允许在 daemon 触发特定事件时自动执行用户定义的 Shell 命令，类似于 git hooks。

---

## 快速开始

编辑 `~/.config/lianwall/hooks.toml`（daemon 首次启动时自动生成）：

```toml
# 壁纸切换时发送桌面通知
[[hook]]
name    = "notify"
on      = "wallpaper_changed"
command = "notify-send 'LianWall' \"已切换到 $LIANWALL_FILENAME\""
```

然后执行热重载，无需重启 daemon：

```bash
lianwall hook reload
```

查看当前已配置的 hooks：

```bash
lianwall hook list
```

---

## 配置参考

### 顶层字段

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `max_concurrent` | `int` | `8` | 最大同时运行数，修改需重启 daemon。 |
| `max_pending` | `int` | `256` | 等待任务上限；0 表示不排队，修改需重启 daemon。 |

### `[[hook]]` 字段

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `name` | `string` | `"hook(<事件名>)"` | 标识名，用于日志和 `hook list` 显示 |
| `on` | `string` | **必填** | 触发事件名，见[事件列表](#事件列表) |
| `command` | `string` | **必填** | 通过 `sh -c` 执行的 Shell 命令，支持环境变量 |
| `mode` | `"video" \| "image"` | 不限制 | 模式过滤，仅对 `wallpaper_changed` / `mode_changed` / `space_updated` 有效 |
| `trigger` | `string[]` | 不限制 | [触发原因过滤](#trigger-过滤值)，仅对 `wallpaper_changed` 有效 |
| `timeout` | `int` | `10` | 超时秒数，超时后进程被强制终止 |
| `enabled` | `bool` | `true` | 是否启用 |

---

## 事件列表

### `wallpaper_changed` — 壁纸切换完成

每次壁纸切换后触发，无论是定时、手动还是其他原因。

**环境变量：**

| 变量 | 说明 | 示例值 |
|------|------|--------|
| `LIANWALL_EVENT` | 事件名 | `wallpaper_changed` |
| `LIANWALL_PATH` | 壁纸完整路径 | `/home/user/wallpapers/video/forest.mp4` |
| `LIANWALL_FILENAME` | 壁纸文件名 | `forest.mp4` |
| `LIANWALL_MODE` | 当前模式 | `video` 或 `image` |
| `LIANWALL_TRIGGER` | 触发原因 | `scheduled`、`manual_next` 等（见下方） |

**可用的 `mode` 过滤值：** `"video"` / `"image"`

**可用的 `trigger` 过滤值（见[完整说明](#trigger-过滤值)）：**  
`scheduled` / `manual_next` / `manual_prev` / `manual_set` / `mode_switch` /  
`vram_downgrade` / `vram_upgrade` / `time_point_refresh` / `daemon_start`

---

### `mode_changed` — 模式切换

执行 `lianwall switch` 或 `lianwall mode <模式>` 后触发。

**环境变量：**

| 变量 | 说明 | 示例值 |
|------|------|--------|
| `LIANWALL_EVENT` | 事件名 | `mode_changed` |
| `LIANWALL_MODE_FROM` | 切换前模式 | `video` 或 `image` |
| `LIANWALL_MODE_TO` | 切换后模式 | `video` 或 `image` |

**`mode` 过滤**：按切换后的模式过滤（即 `LIANWALL_MODE_TO`）。

---

### `space_updated` — 壁纸空间更新

以下情况会触发：
- 执行 `lianwall rescan` 或 `lianwall reload` 后扫描完成
- 执行 `lianwall lock` / `unlock` / `toggle-lock`
- 时间点触发重建向量空间

**环境变量：**

| 变量 | 说明 | 示例值 |
|------|------|--------|
| `LIANWALL_EVENT` | 事件名 | `space_updated` |
| `LIANWALL_SPACE_MODE` | 更新的模式 | `video` 或 `image` |
| `LIANWALL_SPACE_REASON` | 更新原因 | `rescanned`、`lock_changed`、`config_changed` |
| `LIANWALL_TOTAL` | 壁纸总数 | `42` |
| `LIANWALL_AVAILABLE` | 可用数量（未锁定且不在冷却中） | `38` |

> **注意**：该事件不包含具体的锁定/解锁文件路径信息。如需在锁定操作后感知具体文件，可结合 CLI `--json` 模式（`lianwall --json lock <path>` 输出包含文件路径）。

---

### `config_changed` — 配置变更

执行 `lianwall config set` 或 `lianwall reload` 时触发。

**环境变量：**

| 变量 | 说明 | 示例值 |
|------|------|--------|
| `LIANWALL_EVENT` | 事件名 | `config_changed` |
| `LIANWALL_CONFIG_KEY` | 变更的配置键（整体重载时为 `"all"`） | `video_engine.interval` |

---

### `vram_changed` — 显存状态变化

保留现有行为：VRAM 监控更新快照时触发，包括 `downgrade/upgrade/keep`；未跨阈值时也可能是 keep。只关心降级/恢复的脚本应检查 `LIANWALL_VRAM_ACTION`。监控关闭且没有手动覆盖事件时不会触发。

**环境变量：**

| 变量 | 说明 | 示例值 |
|------|------|--------|
| `LIANWALL_EVENT` | 事件名 | `vram_changed` |
| `LIANWALL_VRAM_ACTION` | 动作 | `downgrade`、`upgrade`、`keep` |
| `LIANWALL_VRAM_USED_MB` | 已用显存（MB） | `3200` |
| `LIANWALL_VRAM_FREE_PCT` | 剩余显存百分比 | `21.5` |

---

### `time_point_reached` — 时间点到达

时间到达壁纸目录中定义的时间段边界时触发（如 `08-22/` 目录在 08:00 和 22:00 触发）。

**环境变量：**

| 变量 | 说明 | 示例值 |
|------|------|--------|
| `LIANWALL_EVENT` | 事件名 | `time_point_reached` |
| `LIANWALL_TIME` | 当前时间点 | `08:00` |
| `LIANWALL_NEXT_TIME` | 下一个时间点（无则为空字符串） | `22:00` |

---

### `error` — 错误事件

daemon 内部发生错误时触发（如引擎启动失败）。

**环境变量：**

| 变量 | 说明 |
|------|------|
| `LIANWALL_EVENT` | 事件名（值为 `error`） |
| `LIANWALL_ERROR_MSG` | 错误信息文本 |

---

### `daemon_shutdown` — daemon 关闭

daemon 收到关闭信号后，在实际清理操作前触发。

**关闭顺序**：CLI Shutdown、SIGINT、SIGTERM 使用同一路径。停止普通 hook、丢弃等待队列，再执行关闭 hook，完成后清理持久化状态和引擎。所有关闭 hook **共用 5 秒总预算**，同时遵守每条 `timeout` 和全局并发上限；预算到期会取消剩余任务，不是每条都等待 5 秒。

**环境变量：**

| 变量 | 说明 |
|------|------|
| `LIANWALL_EVENT` | 事件名（值为 `daemon_shutdown`） |

---

## `trigger` 过滤值

`trigger` 字段仅对 `wallpaper_changed` 事件有效，可以限定只在特定操作触发的切换时才执行 hook。

| 值 | 触发场景 |
|----|----------|
| `scheduled` | 定时器到期自动切换 |
| `manual_next` | 用户执行 `lianwall next` |
| `manual_prev` | 用户执行 `lianwall prev` |
| `manual_set` | 用户执行 `lianwall set <路径>` |
| `mode_switch` | 模式切换后的首张壁纸 |
| `vram_downgrade` | 显存降级触发的切换 |
| `vram_upgrade` | 显存恢复触发的切换 |
| `time_point_refresh` | 时间点触发的空间重建后壁纸切换 |
| `daemon_start` | daemon 启动时恢复或选择首张壁纸 |

> **`daemon_start` 用法**：由于目前不支持独立的 `daemon_startup` 事件，使用 `on = "wallpaper_changed"` + `trigger = ["daemon_start"]` 可以间接感知 daemon 启动完成。前提是壁纸目录非空。

---

## 完整配置示例

```toml
# ~/.config/lianwall/hooks.toml

# 最大并发 hook 数（修改需重启 daemon）
max_concurrent = 8

# ==========================================================================
# 通知类
# ==========================================================================

# 壁纸切换时发送桌面通知
[[hook]]
name    = "notify-wallpaper"
on      = "wallpaper_changed"
command = "notify-send -i \"$LIANWALL_PATH\" 'LianWall' \"$LIANWALL_FILENAME\""
timeout = 5

# daemon 启动时通知（通过 wallpaper_changed + daemon_start 实现）
[[hook]]
name    = "notify-startup"
on      = "wallpaper_changed"
trigger = ["daemon_start"]
command = "notify-send 'LianWall' '壁纸管理器已启动'"
timeout = 5

# ==========================================================================
# 颜色主题联动
# ==========================================================================

# 仅图片壁纸切换时运行 pywal（排除定时自动，只响应手动切换）
[[hook]]
name    = "pywal"
on      = "wallpaper_changed"
mode    = "image"
trigger = ["manual_next", "manual_prev", "manual_set"]
command = "wal -i \"$LIANWALL_PATH\" -n -q"
timeout = 30

# ==========================================================================
# 日志记录
# ==========================================================================

# 记录壁纸播放历史（所有触发）
[[hook]]
name    = "log-history"
on      = "wallpaper_changed"
command = "echo \"$(date '+%Y-%m-%d %H:%M:%S') [$LIANWALL_MODE] $LIANWALL_FILENAME (trigger: $LIANWALL_TRIGGER)\" >> ~/.local/share/lianwall/history.log"
timeout = 3

# ==========================================================================
# 系统联动
# ==========================================================================

# 显存降级时记录
[[hook]]
name    = "vram-alert"
on      = "vram_changed"
command = "[ \"$LIANWALL_VRAM_ACTION\" = \"downgrade\" ] && notify-send -u critical 'LianWall' \"显存不足，已降级为静态壁纸 (剩余 ${LIANWALL_VRAM_FREE_PCT}%)\""
timeout = 5

# 时间点到达时记录当前活跃壁纸数
[[hook]]
name    = "time-point-log"
on      = "time_point_reached"
command = "echo \"[${LIANWALL_TIME}] 时间点刷新\" >> ~/.local/share/lianwall/schedule.log"
timeout = 3
```

---

## 调试技巧

### 查看 hook 执行日志

```bash
# 启用 debug 级别日志运行 daemon（前台）
RUST_LOG=lianwall_daemon=debug lianwalld
```

日志中会出现 `Running hook 'xxx'` 和结果信息。

### 手动测试命令

在写 hook 之前先在终端手动测试命令是否正常：

```bash
# 模拟环境变量测试
LIANWALL_PATH=/home/user/wallpapers/video/test.mp4 \
LIANWALL_FILENAME=test.mp4 \
LIANWALL_MODE=video \
LIANWALL_TRIGGER=manual_next \
  sh -c "notify-send 'LianWall' \"$LIANWALL_FILENAME\""
```

### 常见问题

**hook 没有执行**：
1. 检查 `enabled = true`
2. 检查 `on` 值是否拼写正确（全小写 snake_case）
3. 检查 `mode` / `trigger` 过滤是否过于严格
4. 执行 `lianwall hook reload` 确保配置已加载
5. 确认 `lianwall hook list` 中该 hook 显示为 `●`（已启用）

**hook 执行超时**：
- 增大 `timeout` 值，或优化命令使其更快返回
- 长耗时任务（如 pywal）建议 `timeout = 60`

**命令中包含 `$` 变量**：
- 使用双引号 `"..."` 包裹命令，单引号会阻止变量展开
- 或使用 `\"` 转义内部引号

---

## 已知限制

- **`daemon_startup` 事件暂不支持**：可用 `wallpaper_changed` + `trigger = ["daemon_start"]` 代替，但壁纸目录为空时不触发。
- **`lock`/`unlock` 事件不包含文件路径**：`space_updated` + `lock_changed` 仅通知空间有变化，不告知具体文件。
- **队列限制修改需重启**：`max_concurrent/max_pending` 需要重启；规则列表可热加载。

## 执行、积压与超时语义（5.6.0）

- 普通任务按匹配顺序入队，并发执行的完成顺序不保证一致。队列满时丢弃最新匹配任务，
  日志记录 hook 名称及累计丢弃数量，不再无限创建等待任务。广播缓冲丢事件另有警告，不能安全重放脚本。
- stdin/stdout 为 null；stderr 持续读取但最多保留 4 KiB，失败日志最多显示 200 个字符，Unicode 安全。
- 超时及取消会停止该次 hook 自有进程组，包括未脱离该组的普通后代；主动 setsid/更换进程组的程序不在这一清理边界内。
- 普通 shell 正常完成时不清理刻意后台运行的程序。后台命令应自行重定向输出，避免继承日志管道。
- `mode/trigger` 过滤只有在事件提供相应上下文时才匹配；不支持该上下文的事件不会忽略过滤条件后照常执行。

### 事件范围与现有限制

| 需求 | 当前可用事件/过滤 | 边界 |
|---|---|---|
| 壁纸应用完成 | wallpaper_changed + mode/trigger | 不是点击/请求受理通知；应用失败不发送成功事件 |
| 当前模式变化 | mode_changed，mode 按目标模式匹配 | 启动偏好 paths.mode 的配置写入不是实时模式切换 |
| 扫描/锁定变化 | space_updated | 时间点触发的重扫目前也使用 rescanned；精确时间边界用 time_point_reached |
| 配置变化 | config_changed | 提供键名；无模式/壁纸触发原因过滤上下文 |
| 显存变化 | vram_changed | 保留 keep 快照行为，脚本可按 ACTION 过滤 |
| 启动完成 | 暂无独立事件 | daemon_start 仅是首次抽取壁纸的原因；空图库或恢复旧壁纸不保证触发该原因 |
| 退出清理 | daemon_shutdown | 所有关闭规则共享预算；并发过多/脚本过慢时不保证每条都能完成 |

新增启动事件、锁定文件路径、更多触发条件的设计继续在统一修复总表跟踪；本轮没有虚构新的成功通知。
