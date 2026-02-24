# Changelog - 5.2.0

> 发布日期：2026-02-25

---

## 📊 版本摘要

| 分类 | 数量 |
|------|------|
| 🔵 新功能 | 3 |
| 🟣 改进 | 2 |
| 📝 文档 | 2 |

---

## 🎯 概述

5.2.0 是一个功能扩展版本，核心新增了 **Hook 系统**，允许用户在壁纸切换等事件触发时自动执行自定义 Shell 脚本（通知桌面、记录日志、联动其他程序等）。同时修复了 Video→Video 切换时的黑屏闪烁问题，并全面补齐了 CLI `--json` 模式下所有操作命令的结构化输出。

---

## 🔵 新功能

### 1. Hook 系统

**背景**：用户希望在壁纸切换时自动触发外部脚本，例如同步更新状态栏、发送桌面通知、记录播放日志等。

**实现**：新增事件驱动的 Hook 机制，由 daemon 在内部事件发生时（如壁纸切换、模式切换）查找匹配的 Hook 并在独立线程并发执行。

**配置文件**：`~/.config/lianwall/hooks.toml`（首次运行时自动生成，包含完整注释示例）

```toml
# 顶层配置
max_concurrent = 8   # 最大并发执行的 hook 数量（修改需重启 daemon）

[[hook]]
name    = "notify"
on      = "wallpaper_changed"
command = "notify-send '壁纸已切换' \"$LIANWALL_FILENAME\""
enabled = true
```

**支持的事件**（`on` 字段）：

| 事件名 | 触发时机 |
|--------|----------|
| `wallpaper_changed` | 壁纸切换完成（定时 / 手动 next / prev / set 等） |
| `mode_changed` | 模式切换（Video ↔ Image） |
| `space_updated` | 壁纸空间更新（扫描完成 / 锁定变化 / 时间点刷新） |
| `config_changed` | 配置项更新 |
| `vram_changed` | 显存状态变化（降级 / 恢复） |
| `time_point_reached` | 时间点到达 |
| `error` | daemon 内部错误 |
| `daemon_shutdown` | daemon 即将关闭 |

> **注意**：暂不支持 `daemon_startup` 事件。如需感知 daemon 启动，可用 `on = "wallpaper_changed"` + `trigger = ["daemon_start"]` 代替（daemon 启动后设置首张壁纸时触发）。

所有事件的完整环境变量列表见 [HOOKS.md](../HOOKS.md)。`LIANWALL_EVENT` 是所有事件共有的环境变量，其余变量按事件类型各不相同。

`wallpaper_changed` 注入的主要变量：`$LIANWALL_PATH`（完整路径）、`$LIANWALL_FILENAME`（文件名）、`$LIANWALL_MODE`（`video`/`image`）、`$LIANWALL_TRIGGER`（触发原因）。

**可选过滤字段**（不填则不限制）：

```toml
# 只在 Video 模式下触发
mode = "video"

# 只在手动 next/prev 时触发，排除自动轮换
trigger = ["manual_next", "manual_prev"]

# 超时时间（秒，超时后强制终止）
timeout = 10
```

**新增文件**：
- `crates/lianwall-core/src/hook/config.rs` — Hook 配置结构 + 默认 hooks.toml 生成
- `crates/lianwall-core/src/hook/mod.rs` — hooks_path() 等公共接口
- `crates/lianwall-core/src/hook/runner.rs` — Hook 执行逻辑（进程 spawn、环境变量注入、超时控制）
- `crates/lianwall-daemon/src/hook.rs` — HookManager（启动、事件匹配、并发调度、热重载）

---

### 2. CLI Hook 管理命令

新增 `lianwall hook` 子命令组，用于查看当前 Hook 配置状态及触发热重载：

```bash
# 列出所有 hook（含启用/禁用状态、命令预览）
lianwall hook list

# 重新加载 hooks.toml（无需重启 daemon）
lianwall hook reload
```

`hook list` 示例输出：

```
  2 hooks (1 enabled, 1 disabled)

  ● [1] notify on wallpaper_changed
    cmd: notify-send '壁纸已切换' "$LIANWALL_FILENAME"
  ○ [2] log on wallpaper_changed
    cmd: echo "$(date) $LIANWALL_FILENAME" >> ~/wallpaper.log
    mode: Video
```

`--json` 模式下输出 `HookInfo[]` 数组，详见 [CLI-JSON.md](../CLI-JSON.md)。

**新增协议请求**：`ListHooks`（#17）、`ReloadHooks`（#16），详见 [DAEMON-API.md](../DAEMON-API.md)。

---

### 3. hooks.toml `max_concurrent` 配置项

Hook 执行器使用信号量限制最大并发数，防止同时触发大量 Hook 时耗尽系统资源。

```toml
# ~/.config/lianwall/hooks.toml
max_concurrent = 8   # 默认值 8
```

- 最小值强制为 1（设为 0 时自动修正为 1）
- 修改此配置需**重启 daemon** 生效（其他 Hook 字段热重载即可）

---

## 🟣 改进

### 4. 视频切换延迟杀死旧进程，消除黑屏闪烁

**问题**：5.1.x 中壁纸切换时，旧引擎进程在新引擎启动后立即被杀死。由于新引擎尚未渲染出首帧，存在黑屏闪烁窗口。

- **Video→Video**（mpvpaper → mpvpaper）：旧 mpvpaper 被立即 kill，新的尚未解码首帧 → 持续黑屏约 300–800ms
- **Video→Image** / **Image→Video**（跨引擎）：旧引擎被提前终止 → 合成器短暂暴露黑色背景

**修复**：引入延迟杀死（graceful handoff）策略：

| 切换类型 | 策略 |
|----------|------|
| Video→Video（同引擎） | 先用 `set_without_kill()` 启动新 mpvpaper，等待 **600ms** 后再 kill 旧 mpvpaper |
| Image→Video（跨引擎） | 新 mpvpaper 启动，等待 **800ms** 后再停止 swww-daemon |
| Video→Image（跨引擎） | swww img 命令返回后等待 **200ms** 再 kill mpvpaper |

新增 `ManagedProcess` API：`set_without_kill()`（设置新进程但不杀旧的）、`take()`（取出进程所有权由调用方管理生命周期）。

**修改文件**：`crates/lianwall-daemon/src/handler/command.rs`、`crates/lianwall-daemon/src/state.rs`

---

### 5. CLI `--json` 全面结构化输出

5.1.x 中大量操作命令（next、prev、switch、set、mode、lock、unlock、toggle-lock、start、stop、restart、config set、config reset、hook reload）在 `--json` 模式下不输出任何 JSON，仅调用 `print_success()` 打印彩色文字，导致脚本无法判断操作结果。

**修复**：所有操作命令在 `--json` 模式下均输出统一的结构化 JSON。

**统一约定**：
- 成功操作均含 `"success": true` 字段
- 出错时（exit code 非零）输出 `{"success": false, "error": "..."}`

**各命令响应结构**：

```jsonc
// next / prev
{"success": true, "current": "/path/file.mp4", "current_filename": "file.mp4", "mode": "Video"}

// switch / mode
{"success": true, "mode": "Image", "current": "/path/file.jpg", "current_filename": "file.jpg"}

// set
{"success": true, "path": "/path/file.mp4", "current_filename": "file.mp4", "mode": "Video"}

// lock
{"success": true, "path": "/path/file.mp4", "filename": "file.mp4", "locked": true}

// unlock
{"success": true, "path": "/path/file.mp4", "filename": "file.mp4", "locked": false}

// toggle-lock
{"success": true, "path": "/path/file.mp4", "filename": "file.mp4"}

// start（首次启动）
{"success": true, "pid": 12345}

// start（已运行）
{"success": true, "already_running": true}

// stop（成功停止）
{"success": true}

// stop（本来未运行）
{"success": true, "already_stopped": true}

// config set
{"success": true, "key": "video_engine.interval", "old_value": 300, "new_value": 600}

// config reset
{"success": true, "config": { ...完整配置... }}

// hook reload
{"success": true}
```

完整字段说明见 [CLI-JSON.md](../CLI-JSON.md)。

---

## 📝 文档

### 6. 新增 CLI-JSON.md

新增 [CLI-JSON.md](../CLI-JSON.md)，完整记录 CLI `--json` 模式下所有命令的输出结构、字段含义及脚本使用示例。

### 7. 新增 HOOKS.md

新增 [HOOKS.md](../HOOKS.md)，独立于 hooks.toml 内联注释之外，完整记录所有事件的环境变量、trigger 过滤可用値、完整配置示例和调试技巧。

### 8. 更新 DAEMON-API.md

- 新增 `ReloadHooks`（请求 #16）、`ListHooks`（请求 #17）协议文档
- 新增 `HookList` 响应类型及 `HookInfo` 字段说明
- 更新 `Subscribe` / `Unsubscribe` 序号（调整为 #19 / #20）
- 补充各请求的超时行为说明

---

## 📁 变更文件清单

| 文件 | 变更类型 |
|------|----------|
| `Cargo.toml` | 版本号 5.2.0 |
| `crates/lianwall-core/Cargo.toml` | 添加 tracing 依赖 |
| `crates/lianwall-core/src/hook/config.rs` | 新增：Hook 配置结构、默认 hooks.toml |
| `crates/lianwall-core/src/hook/mod.rs` | 新增：hook 模块公共接口 |
| `crates/lianwall-core/src/hook/runner.rs` | 新增：Hook 执行器 |
| `crates/lianwall-core/src/socket/protocol.rs` | 新增 HookInfo、ListHooks、ReloadHooks 协议类型 |
| `crates/lianwall-daemon/src/hook.rs` | 新增：HookManager（并发调度、热重载） |
| `crates/lianwall-daemon/src/handler/command.rs` | 视频切换延迟杀进程、ReloadHooks 处理 |
| `crates/lianwall-daemon/src/state.rs` | 新增 set_without_kill() / take() API |
| `crates/lianwall-daemon/src/connection.rs` | 添加 hook 事件触发 |
| `crates/lianwall-cli/src/handlers/hook.rs` | 新增：hook list / hook reload 命令 |
| `crates/lianwall-cli/src/handlers/wallpaper.rs` | --json 结构化输出 |
| `crates/lianwall-cli/src/handlers/lock.rs` | --json 结构化输出 |
| `crates/lianwall-cli/src/handlers/lifecycle.rs` | --json 结构化输出 |
| `crates/lianwall-cli/src/handlers/config.rs` | --json 结构化输出（补 success 字段、reset 包装） |
| `crates/lianwall-cli/src/commands.rs` | 新增 HookAction 子命令 |
| `crates/lianwall-cli/src/client.rs` | 新增 list_hooks() / reload_hooks() 方法 |
| `CLI-JSON.md` | 新增：CLI JSON 输出完整参考文档 |
| `HOOKS.md` | 新增：Hook 系统完整说明文档 |
| `DAEMON-API.md` | 更新：Hook 协议文档、超时说明 |
| `README.md` | 新增：文档索引表格、Hook 管理功能说明 |
