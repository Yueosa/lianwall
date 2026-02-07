# Changelog - 5.1.2

> 发布日期：2026-02-07

---

## 📊 版本摘要

| 分类 | 数量 |
|------|------|
| 🔴 Critical 修复 | 1 |
| 🟡 Medium 修复 | 1 |
| 🔵 重构 | 1 |

---

## 🎯 概述

修复了两个影响壁纸切换可靠性的关键 bug，并完成了播放历史系统的架构重构。

- **SetMode 事件缺失**：切换模式后 GUI 不更新预览（只发了 `ModeChanged`，漏发 `WallpaperChanged`）
- **冷却队列耗尽**：壁纸全部锁定仅剩少量可用时，`Next` 命令静默失败
- **浏览器式播放历史**：将原来的双层栈式 prev/next 替换为统一的浏览器前进/后退模型

---

## 🔴 Critical 修复

### 1. SetMode 不发布 WallpaperChanged 事件

**影响**: 切换视频/图片模式后，GUI 预览画面不更新。用户必须手动点击 Next 才能看到新模式的壁纸。

**根因**: `handle_set_mode` 在应用壁纸后只发布了 `ModeChanged` 事件，没有发布 `WallpaperChanged` 事件。GUI 依赖 `WallpaperChanged` 来刷新预览，因此切换模式后预览停留在旧壁纸。

此外，当目标模式空间的 `current_index` 为 `None`（首次进入该模式）时，`handle_set_mode` 什么也不做，不选壁纸也不切换引擎。

**修复**:

| 变更 | 说明 |
|------|------|
| 补发 `WallpaperChanged` | 应用壁纸后同时发布 `ModeChanged` 和 `WallpaperChanged { trigger: ModeSwitch }` |
| 处理空 `current_index` | 当目标空间无当前壁纸时，调用 `select_next` 选出一张 |
| 调度器重置 | `ModeSwitch` 加入 scheduler 的计时器重置触发列表 |

**修改文件**:

| 文件 | 变更 |
|------|------|
| `handler/command.rs` | `handle_set_mode` 补发事件、处理 None current_index |
| `scheduler.rs` | `ModeSwitch` 加入计时器重置 match 分支 |

---

## 🟡 Medium 修复

### 2. 冷却队列耗尽时 Next 静默失败

**影响**: 当用户锁定了大部分壁纸，仅剩少数几张可用时（可用数 ≤ 冷却窗口大小），所有未锁定壁纸都处于冷却队列中。此时 `find_nearest_available` 返回 `None`，`select_next` 失败，`Next` 命令返回 `EmptySpace` 错误——但实际上空间并不为空，只是壁纸全在冷却中。

**根因**: `find_nearest_available` 的主循环同时排除了锁定和冷却中的壁纸。当可用壁纸数量 ≤ 冷却窗口大小时，所有候选都被排除，返回 `None`。

**修复**: 在 `find_nearest_available` 末尾增加回退逻辑——当主循环无结果时，遍历冷却队列，选择最早进入冷却的（即冷却最久的）未锁定壁纸，且排除当前正在播放的壁纸（避免 Next 选中同一张）。仅当只剩 1 张可用壁纸时，才允许选中当前壁纸。

```rust
// 回退：所有未锁定壁纸都在冷却中
if best_idx.is_none() {
    // 优先选择非当前壁纸（避免 Next 选中同一张）
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

## 🔵 重构：浏览器式播放历史

### 3. 双层栈式历史 → 统一的浏览器前进/后退模型

**旧设计问题**:

系统存在 **两套独立的历史机制**，职责割裂且行为不一致：

| 层级 | 数据结构 | 位置 | 用途 |
|------|----------|------|------|
| Core 层 | `WallpaperSpace.history: Vec<usize>` | selector.rs | `select_previous` 弹出索引 |
| Daemon 层 | `SharedState.wallpaper_history: RwLock<VecDeque<PathBuf>>` | state.rs | `handle_prev` 弹出路径 |

实际运行时 `handle_prev` 只使用 daemon 层历史，core 层的 `select_previous` 从未被调用。两套历史各自 push/pop，容易不同步。且栈模型只支持后退——按一次 Prev 后无法再前进回去，历史记录被永久弹出。

**新设计**: 浏览器式前进/后退（`PlaybackHistory`）

```
[壁纸A] → [壁纸B] → [壁纸C] → [壁纸D]
                                  ↑ cursor
```

核心数据结构（`state.rs`）：

```rust
pub struct PlaybackHistory {
    entries: Vec<PathBuf>,       // 有序历史记录
    cursor: Option<usize>,       // 当前光标位置
}
```

**行为规则**:

| 操作 | 条件 | 行为 |
|------|------|------|
| Next | 光标在末尾 | 算法选出新壁纸 → `push()` 追加到末尾 |
| Next | 光标不在末尾 | `forward()` → 光标前进一步，播放 `entries[cursor]` |
| Prev | 光标 > 0 | `backward()` → 光标后退一步，播放 `entries[cursor]` |
| Prev | 光标 = 0 | 返回 `NoHistory` 错误 |
| 定时切换 / 模式切换 / SetWallpaper | — | `push()` → 截断光标之后的前进历史，追加新壁纸 |

**容量限制**: 最大 100 条，超出时从前端（最旧端）移除，光标相应调整。

**浏览器截断语义**: 在历史中间位置执行非导航操作时（如定时切换触发了新壁纸），光标之后的"前进历史"会被截断——与浏览器在历史中间位置点击新链接时丢弃前进历史的行为一致。

```
[A] → [B] → [C] → [D]
        ↑ cursor（Prev 两次后）

此时定时切换选出 [E]：
[A] → [B] → [E]
              ↑ cursor（C、D 被截断）
```

**辅助改动**: 新增 `detect_mode()` 函数，从文件扩展名检测壁纸类型。Prev/Next 导航历史时可能遇到不同类型的壁纸（视频/图片混合），需要自动切换引擎模式。

**删除的代码**:

| 删除项 | 位置 |
|--------|------|
| `WallpaperSpace.history: Vec<usize>` | `crates/lianwall-core/src/wallpaper/struct.rs` |
| `select_previous()` 函数 | `crates/lianwall-core/src/algorithm/selector.rs` |
| `select_next()` 中的历史推入逻辑 | `crates/lianwall-core/src/algorithm/selector.rs` |
| `SharedState.wallpaper_history` | `crates/lianwall-daemon/src/state.rs` |
| `MAX_HISTORY_SIZE` (core 层) | `crates/lianwall-core/src/algorithm/selector.rs` |
| `select_previous` 导出 | `crates/lianwall-core/src/algorithm/mod.rs` |

**新增的代码**:

| 新增项 | 位置 |
|--------|------|
| `PlaybackHistory` 结构体 + 5 个方法 | `crates/lianwall-daemon/src/state.rs` |
| `detect_mode()` 辅助函数 | `crates/lianwall-daemon/src/handler/command.rs` |

**修改文件**:

| 文件 | 变更 |
|------|------|
| `crates/lianwall-core/src/wallpaper/struct.rs` | 移除 `history` 字段 |
| `crates/lianwall-core/src/wallpaper/space.rs` | 移除 `history: Vec::new()` 初始化（2 处） |
| `crates/lianwall-core/src/algorithm/selector.rs` | 移除 `select_previous`、移除 `select_next` 中历史推入、移除 `MAX_HISTORY_SIZE` |
| `crates/lianwall-core/src/algorithm/mod.rs` | 移除 `select_previous` 导出 |
| `crates/lianwall-daemon/src/state.rs` | 新增 `PlaybackHistory`，替换 `wallpaper_history` |
| `crates/lianwall-daemon/src/handler/command.rs` | 重写 `handle_next`/`handle_prev`，新增 `detect_mode`，`handle_set_wallpaper`/`handle_set_mode` 追加历史 |

---

## 📁 变更文件清单

| 文件 | 变更类型 |
|------|----------|
| `Cargo.toml` | 版本号 5.1.2 |
| `crates/lianwall-core/src/wallpaper/struct.rs` | 移除 `history` 字段 |
| `crates/lianwall-core/src/wallpaper/space.rs` | 移除历史初始化 |
| `crates/lianwall-core/src/algorithm/selector.rs` | 移除 `select_previous`、历史推入；新增冷却回退 |
| `crates/lianwall-core/src/algorithm/mod.rs` | 移除 `select_previous` 导出 |
| `crates/lianwall-daemon/src/state.rs` | 新增 `PlaybackHistory`，替换旧历史 |
| `crates/lianwall-daemon/src/handler/command.rs` | 重写 next/prev/set_mode/set_wallpaper |
| `crates/lianwall-daemon/src/scheduler.rs` | `ModeSwitch` 计时器重置 |
