# Changelog - 5.1.3

> 发布日期：2026-02-07

---

## 📊 版本摘要

| 分类 | 数量 |
|------|------|
| 🟡 Medium 修复 | 1 |
| 🟢 Minor 修复 | 1 |

---

## 🎯 概述

修复冷却回退选中当前壁纸的 bug，以及 daemon 首次启动时配置文件不存在直接报错退出的问题。

---

## 🟡 Medium 修复

### 1. 冷却回退未排除当前壁纸

**影响**: 当大部分壁纸被锁定、可用壁纸全部处于冷却队列中时，`Next` 反复选中当前正在播放的壁纸，点击多次都不切换。

**根因**: 5.1.2 新增的冷却回退逻辑遍历冷却队列选择最早进入的未锁定壁纸，但没有排除 `current_index`。当当前壁纸恰好是冷却队列中最早的条目时，每次都选中它自己。

**修复**: 冷却回退改为两轮遍历：

1. **第一轮**: 跳过 `current_index`，选冷却最久的非当前壁纸
2. **第二轮**（仅当只剩 1 张可用壁纸时触发）: 允许选中当前壁纸

```rust
if best_idx.is_none() {
    // 优先选择非当前壁纸
    for &idx in &space.cooldown_queue {
        if idx < space.items.len() && !space.items[idx].locked && Some(idx) != space.current_index {
            return Some(idx);
        }
    }
    // 最终回退：只剩 1 张可用壁纸时允许选中当前壁纸
    for &idx in &space.cooldown_queue {
        if idx < space.items.len() && !space.items[idx].locked {
            return Some(idx);
        }
    }
}
```

**修改文件**: `crates/lianwall-core/src/algorithm/selector.rs`

---

## 🟢 Minor 修复

### 2. Daemon 首次启动不自动创建配置文件

**影响**: 全新安装后首次运行 `lianwall start` 或 `lianwalld`，因 `~/.config/lianwall/config.toml` 不存在而直接报错退出。用户必须手动创建配置文件才能启动。

**根因**: `main.rs` 使用 `config::read()` 加载配置——该函数只做读取，文件不存在时返回 IO 错误。而 `config::create()` 函数已经实现了"不存在则创建默认配置"的逻辑，但 daemon 没有调用它。

**修复**: 将 `config::read(ConfigReadInput { ... })` 替换为 `config::create(ConfigCreateInput { ... })`。首次启动时自动创建默认配置文件并记录日志：

```
Config not found, created default at "/home/user/.config/lianwall/config.toml"
```

**修改文件**: `crates/lianwall-daemon/src/main.rs`

---

## 📁 变更文件清单

| 文件 | 变更类型 |
|------|----------|
| `Cargo.toml` | 版本号 5.1.3 |
| `crates/lianwall-core/src/algorithm/selector.rs` | 冷却回退排除当前壁纸 |
| `crates/lianwall-daemon/src/main.rs` | `config::read` → `config::create` |
