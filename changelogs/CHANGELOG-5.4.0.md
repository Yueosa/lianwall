# Changelog - 5.4.0

> 发布日期：2026-03-25

---

## 📊 版本摘要

| 分类 | 数量 |
|------|------|
| 🟢 兼容性修复 | 1 |

---

## 🎯 概述

5.4.0 为 **awww 兼容性更新**。

ArchLinux 上游将 `swww` 包替换为 `awww`，所有 `swww*` 命令更名为 `awww*`。本版本对图片引擎层进行了兼容性改造，**优先使用 awww，自动降级到 swww**，两者均不可用时才报错，实现无缝过渡，用户无需手动干预。

---

## 🟢 兼容性修复

### 1. 图片引擎兼容 awww / swww 双版本

**背景**：ArchLinux 将 `swww` 包彻底替换为 `awww`，`swww-daemon`、`swww img`、`swww clear`、`swww query` 等命令全部更名为 `awww-daemon`、`awww img`、`awww clear`、`awww query`。原版本硬编码 `swww` 命令，在更新后的系统上无法启动图片模式。

**改动**：

- 新增 `detect_image_bin()` 函数，启动时按优先级探测可用命令：
  1. 尝试 `awww` → 可用则使用
  2. 尝试 `swww` → 可用则使用
  3. 两者均不可用 → 返回错误 `awww or swww not found`
- `EngineState` 新增 `image_bin` 字段，在 `init()` 阶段检测一次并缓存，后续所有命令（daemon 启动、`img`、`clear`、`query`）均从该字段取二进制名称，避免重复检测。
- daemon 生命周期管理兼容双版本：`pkill` 时同时尝试杀死 `awww-daemon` 和 `swww-daemon`，防止残留进程。
- `is_any_swww_daemon_running()` 改为依次尝试 `awww query` 和 `swww query`，任一成功即视为 daemon 在运行。

**影响范围**：`crates/lianwall-core/src/engine/async_ops.rs`，其余代码和配置文件（`swww_args` 配置键等）保持不变，已有配置无需修改。

**AUR 包更新**：`lianwalld-bin` 的 `depends` 由 `swww` 改为 `awww`。
