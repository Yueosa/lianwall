# Changelog - 5.1.0

> 发布日期：2026-02-06

## 🎯 概述

本版本通过全面的 TODO 审计，修复了多个核心逻辑问题，完善了 Daemon 的功能。主要改进包括：黄金角算法修复、GPU 降级壁纸切换、启动状态恢复、时间约束过滤、事件订阅过滤等。

## ✅ 第一轮修复（TODO 审计）

### 1. 配置持久化

**问题**：`SetConfig` 命令只修改内存中的配置，Daemon 重启后配置丢失。

**修复**：
- `handle_set_config()` 在修改内存配置后，自动调用 `lianwall_core::config::update()` 保存到文件

---

### 2. Request::Restart 移除

**决议**：从 Socket 协议中移除 `Request::Restart`。

**原因**：
- Daemon 无法自我重启（进程结束后无法启动自己）
- 重启逻辑应由客户端（CLI/GUI）负责：`shutdown` + 启动新进程

---

### 3. VRAM 状态集成到 StatusInfo

**问题**：`StatusInfo` 中的 VRAM 字段始终为 0。

**修复**：
- 在 `SharedState` 中添加 `GpuSnapshot` 结构
- GPU Monitor Task 定期更新 `gpu_snapshot`
- `get_status()` 读取真实 VRAM 信息

---

### 4. 时间调度信息集成

**问题**：`StatusInfo.next_time_point` 始终为 `None`。

**修复**：
- 扫描壁纸时收集所有时间点到 `time_points` 缓存
- `get_status()` 计算 `next_time_point` 和 `time_points_count`

---

### 5. Scheduler 集成 GPU 状态检查

**问题**：`should_switch()` 未使用 GPU 状态做决策。

**修复**：
- 读取 `GpuSnapshot` 检查降级状态
- 已降级或 VRAM 不足时跳过视频壁纸切换

---

### 6. 配置变更事件监听

**问题**：Scheduler 通过轮询检查配置变更。

**修复**：
- Scheduler 订阅 `EventBus`，监听 `ConfigReloaded` 和 `ModeChanged` 事件
- 收到事件时立即更新定时器间隔

---

### 7. 可修改配置键列表（18 个）

**问题**：`ConfigSnapshot.modifiable_keys` 始终为 `None`。

**修复**：实现 `get_modifiable_keys()` 返回完整的可配置键列表。

---

### 8. 事件硬编码值修复

**问题**：多个事件字段是硬编码的。

**修复**：`Event::SpaceUpdated` 和 `Event::GpuStateUpdated` 使用真实数据。

---

## ✅ 第二轮修复（Daemon 逻辑审计）

### 9. Next/Prev 黄金角算法修复 🔴

**问题**：
- `handle_next()` 使用简单的 `(i + 1) % len` 递增索引
- `handle_prev()` 使用简单的 `(i - 1)` 递减索引
- 完全忽略了 `lianwall_core::algorithm` 的黄金角算法
- **后果**：Next+Prev 循环会污染历史栈，无法正确回退

**修复**：
- `handle_next()` 调用 `algorithm::select_next()` 黄金角选择
- `handle_prev()` 调用 `algorithm::select_previous()` 从历史栈 pop
- 新增 `ErrorCode::NoHistory` 错误码

**行为变化**：
- Next：使用黄金角算法选择下一张，当前壁纸入栈
- Prev：从历史栈弹出，无历史时返回 `no_history` 错误

---

### 10. GPU 降级壁纸切换修复 🔴

**问题**：
- GPU Monitor 检测到需要降级时，只修改了 `mode`
- 没有实际切换壁纸
- **后果**：模式变了但壁纸没变，GUI 显示与实际不一致

**修复**：
- `gpu_monitor()` 接收 `cmd_tx` 参数
- Downgrade/Upgrade 后发送 `Request::Next` 到命令队列
- 更新 main.rs 传递 `cmd_tx` 给 GPU Monitor

**行为变化**：
- 降级：Video → Image 模式切换 + 自动应用一张图片壁纸
- 升级：Image → Video 模式切换 + 自动应用一张视频壁纸

---

### 11. 启动状态恢复 🔴

**问题**：
- Daemon 启动后不会自动应用壁纸
- 用户必须手动执行 `next` 或等待 scheduler 触发

**修复**：
- `ModeData` 添加 `current_path` 字段持久化当前壁纸
- `rebuild_space()` 恢复 `current_index`
- `export_to_persisted()` 导出 `current_path`
- 启动时使用 `rebuild_space()` + `load_weights()` 恢复状态
- 启动后检查：有上次记录则恢复，无则 `Next` 选新的
- 关闭时 `save_weights()` 保存状态

**行为变化**：
- 启动：恢复上次壁纸，或自动选择一张新的
- 关闭：保存当前状态到 `~/.cache/lianwall/weights.json`

---

### 12. 时间约束过滤实现 🟡

**问题**：
- `lianwall_core::wallpaper::filter_active()` 函数存在但从未被调用
- 时间约束（早晨/下午/夜晚壁纸组）完全失效

**修复**：
- 启动时调用 `filter_active()` 过滤壁纸
- `handle_rescan()` 也调用 `filter_active()` 并使用 `rebuild_space()`
- Scheduler 添加时间点监听，到达时间点时自动 `Rescan`
- 新增 `Event::TimePointReached` 内部事件

**行为变化**：
- 启动：根据当前时间过滤壁纸构建空间
- 运行中：到达新时间段自动重建空间
- 日志：输出时间过滤统计和下一个时间点信息

---

### 13. Subscribe 事件过滤实现 🟢

**问题**：
- `Subscribe { events }` 的 events 参数被忽略
- 订阅后会收到所有事件，无法选择性订阅

**修复**：
- `ConnectionState` 添加 `subscribed_events: HashSet<EventType>`
- Subscribe 时调用 `EventType::expand()` 展开 All
- 事件推送时检查 `subscribed_events` 进行过滤
- 添加 `event_to_type()` 函数映射内部事件到 EventType

**行为变化**：
- 订阅时可以指定感兴趣的事件类型
- 只推送订阅列表中的事件

---

### 14. immediate_sync 参数实现 🟢

**问题**：
- `Subscribe { immediate_sync }` 参数未使用
- 订阅时不会立即发送当前状态

**修复**：
- `ConnectionState` 添加 `pending_sync: bool`
- Subscribe 处理时保存 `immediate_sync` 标志
- 发送 Subscribed 响应后检查并发送 GetStatus 结果

**行为变化**：
- `immediate_sync: true` 时，订阅成功后立即推送当前状态
- GUI 可以在订阅时获取初始状态，无需额外请求

---

## 🆕 新增 API

### ErrorCode

```rust
pub enum ErrorCode {
    // ... existing
    NoHistory,  // 没有历史记录（prev 无法回退）
}
```

### Event (内部)

```rust
pub enum Event {
    // ... existing
    TimePointReached,  // 时间点到达（触发重建向量空间）
}
```

### ModeData (持久化)

```rust
pub struct ModeData {
    pub pointer: f64,
    pub current_path: Option<PathBuf>,  // 新增：当前壁纸路径
    pub items: Vec<PersistedRecord>,
}
```

---

## 📁 影响的文件

| 文件 | 变更类型 |
|------|----------|
| `Cargo.toml` | 版本号 5.0.0 → 5.1.0 |
| `crates/lianwall-core/src/socket/protocol.rs` | 移除 Restart，添加 NoHistory |
| `crates/lianwall-core/src/wallpaper/struct.rs` | ModeData 添加 current_path |
| `crates/lianwall-core/src/wallpaper/space.rs` | rebuild_space 恢复 current_index |
| `crates/lianwall-daemon/src/state.rs` | GpuSnapshot、time_points |
| `crates/lianwall-daemon/src/event.rs` | TimePointReached 事件 |
| `crates/lianwall-daemon/src/scheduler.rs` | 时间点监听、GPU cmd_tx |
| `crates/lianwall-daemon/src/connection.rs` | 事件过滤、immediate_sync |
| `crates/lianwall-daemon/src/handler/query.rs` | VRAM、时间点、可配置键 |
| `crates/lianwall-daemon/src/handler/command.rs` | Next/Prev 算法、Rescan 过滤 |
| `crates/lianwall-daemon/src/main.rs` | 启动恢复、关闭保存、时间过滤 |

---

## ⬆️ 升级指南

### 从 5.0.0 升级

1. 替换二进制文件
2. 重启 Daemon：`lianwall restart`

### 破坏性变更

- **移除 `Request::Restart`**：需要改为 Shutdown + 启动新进程
- **Prev 行为变更**：不再是简单的索引减一，而是真正的历史回退

---
