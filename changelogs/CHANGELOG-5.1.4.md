# Changelog - 5.1.4

> 发布日期：2026-02-23

---

## 📊 版本摘要

| 分类 | 数量 |
|------|------|
| 🔴 Critical 修复 | 1 |

---

## 🎯 概述

修复 `SetMode` 命令在引擎切换失败时导致模式状态不一致的严重 bug。该问题会导致模式切换后所有后续命令（Next / Prev / SetMode）持续返回 Error，直到守护进程重启。

---

## 🔴 Critical 修复

### 1. SetMode 引擎失败导致模式状态不一致（级联错误）

**影响**: 从 Video 切换到 Image 模式时，如果 swww-daemon 未能在 200ms 窗口内启动（或 `swww img` 命令失败），`SetMode` 返回 Error。但此时内部模式已被提前写入为 Image，导致：

1. 后续所有 `Next` 命令走 Image 路径 → swww 仍不可用 → 持续返回 `EngineError`
2. 再次发送 `SetMode Image` 被判定为"模式未变" → 直接返回 Ok，不重试 apply
3. 用户无法通过任何命令恢复，只能重启守护进程

反向（Image→Video）同理：若 mpvpaper 启动失败，模式已提交为 Video，后续操作全部失败。

**根因**: `handle_set_mode()` 在第 224 行先执行 `*state.engine.mode.write().await = mode`（提交模式），然后才调用 `apply_wallpaper()`。当 apply 失败返回 Error 时，模式变更已生效且无回滚，状态永久不一致。

**修复**: 将模式提交推迟到 `apply_wallpaper()` 成功之后。执行顺序改为：

1. 选择目标壁纸路径（不修改任何状态）
2. 调用 `apply_wallpaper()` 启动新引擎
3. **仅在 apply 成功后**才提交 `mode`、`current`、播放历史和事件

失败时保持 `old_mode` 不变，后续命令继续在原模式下正常工作。

```rust
// 修复前（先提交，后 apply）:
*state.engine.mode.write().await = mode;      // ← 提前提交
let path = select_wallpaper(...);
if let Err(e) = apply_wallpaper(...).await {   // ← 失败时模式已脏
    return Response::error(...);
}

// 修复后（先 apply，后提交）:
let path = select_wallpaper(...);              // ← 只读操作
if let Err(e) = apply_wallpaper(...).await {
    return Response::error(...);               // ← 失败时模式未变
}
*state.engine.mode.write().await = mode;       // ← 成功后才提交
```

**修改文件**: `crates/lianwall-daemon/src/handler/command.rs`

---

## 📁 变更文件清单

| 文件 | 变更类型 |
|------|----------|
| `Cargo.toml` | 版本号 5.1.4 |
| `crates/lianwall-daemon/src/handler/command.rs` | SetMode 模式提交延迟至 apply 成功后 |
