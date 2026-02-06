# Changelog - 5.1.1

> 发布日期：2026-02-07

---

## 📊 版本摘要

| 分类 | 数量 |
|------|------|
| 🔴 Critical 修复 | 2 |
| 🟡 Medium 修复 | 6 |
| 🟢 Minor 修复 | 3 |
| ❌ 误报排除 | 1 |
| 🔧 构建/元数据 | 2 |

---

## 🎯 概述

全面审计修复版本。修复了 `lianwall status` 数据不准确、`reload` 超时、`config` 命令不一致、调度器计时器行为异常等 11 个 bug，并统一了 workspace 元数据和构建脚本。

**所有改动均为 bug 修复，无结构性变更，完全向后兼容。**

---

## 🔴 Critical 修复

### 1. `Next Switch` 倒计时永远显示固定值

**影响**: `lianwall status` 的 `Next Switch` 始终显示配置的 interval（如 `10m 0s`），而非真实的剩余倒计时。

**根因**: `handler/query.rs` 直接读取配置的 `interval` 返回，scheduler 内部的 `_next_switch` 变量以 `_` 前缀标记为未使用，没有暴露给状态查询。

**修改文件**:

| 文件 | 变更 |
|------|------|
| `state.rs` | 新增 `next_switch: RwLock<Instant>` 字段、`next_switch_remaining_secs()` 和 `set_next_switch()` 方法 |
| `scheduler.rs` | 删除局部变量 `_next_switch`，改为调用 `state.set_next_switch()` 同步到共享状态 |
| `handler/query.rs` | 调用 `state.next_switch_remaining_secs()` 获取真实剩余时间 |

### 2. `available_count` 未扣除冷却队列

**影响**: `lianwall status` 显示的 `Available` 数量包含了冷却中的壁纸，高于实际可选壁纸数。

**修复**: `available_count` 计算改为 `total - locked - in_cooldown`（使用 `saturating_sub`）。

---

## 🟡 Medium 修复

### 3. `scanned_count` 语义错误

**影响**: `Scanned` 显示的是时间过滤后的活跃壁纸数，而非目录中实际扫描到的文件总数。

**修复**: 新增 `SharedState.scanned_counts: RwLock<(usize, usize)>` 字段，在初始扫描和 rescan 时保存过滤前的原始扫描数。

### 4. `interval` 首次 tick 立即触发

**影响**: daemon 启动后、配置变更后、模式切换后，`tokio::time::interval()` 的首次 `tick()` 立即返回，导致多切换一次壁纸。

**修复**: 3 处 `interval()` 创建后均立即消耗首次 tick（`timer.tick().await`）。

### 5. 手动切换不重置调度器计时器

**影响**: 用户手动 `next`/`prev`/`set` 后，调度器倒计时不重置。例如 interval=600s，在第 550s 手动 next，50s 后又自动切换。

**修复**: scheduler 监听 `WallpaperChanged` 事件，当 trigger 为 `ManualNext`/`ManualPrev`/`ManualSet` 时重建 interval timer 并更新 `next_switch`。

### 6. `reload` 不触发 rescan，CLI 等待超时

**影响**: `lianwall reload` 始终 30s 超时。

**根因**: daemon 的 `ReloadConfig` 只发 `ConfigChanged` 事件，不触发 rescan。CLI 同时等待 `ConfigChanged` 和 `SpaceUpdated` 两个事件，后者永远不来。

**修复**: scheduler 收到 `ConfigChanged { key: "all" }` 时自动发送 `Rescan` 命令，使壁纸目录与新配置同步。

### 7. `config show` 在线/离线输出格式不一致

**影响**: 在线时 non-JSON 模式也输出 JSON 格式（与 `--json` 行为相同），而离线时正确输出 TOML。

**根因**: `is_json()` 的 true/false 两个分支代码完全相同。

**修复**: 在线 non-JSON 分支改为将 daemon 返回的 JSON 反序列化为 `Config` 后输出 TOML 格式。

### 8. 离线 `config set` 不支持数组类型

**影响**: 离线时无法设置 `mpv_args`、`mpvpaper_args`、`swww_args` 等 `Vec<String>` 字段。

**修复**: `get_config_value` 和 `set_config_value` 添加数组字段分支。新增 `parse_string_array` 辅助函数，支持 JSON 数组格式（`'["--no-audio","--loop=inf"]'`）或逗号分隔格式（`--no-audio,--loop=inf`）。

---

## 🟢 Minor 修复

### 9. 离线 `config set` 路径不做波浪号展开

**影响**: 离线设置 `paths.video_dir = ~/Videos/xxx` 时 `~` 不展开，写入配置文件的是字面量 `~/Videos/xxx`。

**修复**: 4 个路径字段（`video_dir`/`image_dir`/`socket_path`/`pid_path`）统一使用 `expand_path()` 处理。

### 10. 离线 `config set` 无值域校验

**影响**: 可以设置 `interval = 0` 或 `threshold_percent = 200` 等无效值。

**修复**: 添加校验规则：
- `interval` / `check_interval`：必须 > 0
- `threshold_percent` / `recovery_percent`：必须在 0~100 之间

### 11. `next_time_point` 计算逻辑不统一

**影响**: `get_status` 使用 core 库的 `next_key_point()` 函数，而 `get_time_info` 手写了一段等效但冗余的逻辑。

**修复**: `get_time_info` 改为复用 `lianwall_core::wallpaper::next_key_point()` 函数。

---

## ❌ 误报排除

### ~~toggle_lock 两空间独立 toggle~~

**结论**: 视频扩展名（`.mp4`/`.mkv` 等）和图片扩展名（`.jpg`/`.png` 等）互斥，同一文件不会同时出现在两个空间，独立 toggle 不会导致不一致。

---

## 🔧 构建/元数据修复

### 12. `lianwall-daemon` Cargo.toml 未使用 workspace 继承

`version`、`edition`、`license`、`authors` 硬编码在自身 Cargo.toml 中。其中 `edition = "2021"` 与 workspace 的 `"2024"` 不一致。

**修复**: 统一使用 `version.workspace = true`、`edition.workspace = true` 等 workspace 继承。

### 13. `build.sh` 版本提取逻辑不可靠

构建脚本使用 `grep -A5 '[workspace.package]'` 提取版本号，因 Cargo.toml 格式变化导致提取失败。

**修复**: 简化为 `grep '^version' Cargo.toml | grep -v '^\[' | head -1`。

---

## 📁 变更文件清单

| 文件 | 变更类型 |
|------|----------|
| `crates/lianwall-daemon/src/state.rs` | 新增字段和方法 |
| `crates/lianwall-daemon/src/scheduler.rs` | 修复计时器行为、添加事件处理 |
| `crates/lianwall-daemon/src/handler/query.rs` | 修复状态查询数据源 |
| `crates/lianwall-daemon/src/handler/command.rs` | rescan 后保存扫描计数 |
| `crates/lianwall-daemon/src/main.rs` | 初始扫描后保存扫描计数 |
| `crates/lianwall-daemon/Cargo.toml` | workspace 继承 |
| `crates/lianwall-cli/src/handlers/config.rs` | 修复输出格式、添加数组/路径/校验支持 |
| `Cargo.toml` | 版本号 5.1.1 |
| `build.sh` | 简化版本提取 |
