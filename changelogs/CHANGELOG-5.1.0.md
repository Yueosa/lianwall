# Changelog - 5.1.0

> 发布日期：2026-02-06

## 🎯 概述

本版本通过全面的 TODO 审计和 Socket API 审计，修复了多个核心逻辑问题，完善了 Daemon 的功能。主要改进包括：黄金角算法修复、GPU 降级壁纸切换、启动状态恢复、时间约束过滤、事件订阅过滤、Socket API 完整性修复等。

---

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
- Scheduler 订阅 `EventBus`，监听 `ConfigChanged` 和 `ModeChanged` 事件
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

## ✅ 第三轮修复（Socket API 审计）

### 15. SetConfig 支持全部 18 个配置键 🔴

**问题**：
- `GetConfig` 返回 18 个可配置键
- `SetConfig` 只支持 4 个（image_engine.interval, video_engine.interval, paths.video_dir, paths.image_dir）
- API 语义不一致

**修复**：扩展 `handle_set_config()` 支持全部 18 个键：

| 分类 | 键名 |
|------|------|
| paths | `paths.mode`, `paths.video_dir`, `paths.image_dir` |
| video_engine | `video_engine.interval`, `video_engine.display`, `video_engine.mpvpaper_args`, `video_engine.mpv_args` |
| image_engine | `image_engine.interval`, `image_engine.outputs`, `image_engine.swww_args` |
| vram | `vram.enabled`, `vram.threshold_percent`, `vram.recovery_percent`, `vram.check_interval`, `vram.cooldown_seconds` |
| daemon | `daemon.socket_path`, `daemon.pid_path`, `daemon.log_level` |

---

### 16. TimePointReached 事件推送 🔴

**问题**：
- 内部 `Event::TimePointReached` 在 `event_to_response()` 中返回 `None`
- 订阅的客户端无法得知向量空间何时重建

**修复**：
- 内部 `Event::TimePointReached` 添加 `time` 和 `next_time` 字段
- `event_to_response()` 正确转换为 `SocketEvent::TimePointReached { time, next_time }`

---

### 17. WallpaperChanged.trigger 正确设置 🟡

**问题**：
- `trigger` 始终硬编码为 `WallpaperTrigger::Scheduled`
- GUI 无法区分手动切换和自动切换

**修复**：
- `Request::Next/Prev` 添加 `trigger_hint: Option<WallpaperTrigger>` 字段
- 内部 `Event::WallpaperChanged` 添加 `trigger` 字段
- 各调用点传入正确的 trigger

| 场景 | trigger |
|------|---------|
| 用户 Socket `Next` | `ManualNext` |
| 用户 Socket `Prev` | `ManualPrev` |
| Scheduler 定时切换 | `Scheduled` |
| GPU 降级 | `VramDowngrade` |
| GPU 恢复 | `VramUpgrade` |
| Daemon 启动 | `DaemonStart` |

---

### 18. ConfigChanged 事件携带 old/new value 🟡

**问题**：
- `ConfigChanged` 事件的 `old_value` 和 `new_value` 始终为 `null`
- GUI 无法做增量更新

**修复**：
- 内部 `Event::ConfigReloaded` 改为 `Event::ConfigChanged { key, old_value, new_value }`
- `SetConfig`: 先获取旧值，更新后发布带具体 old/new 的事件
- `ReloadConfig`: key="all"，old_value/new_value 为 null
- 新增 `get_config_value()` 辅助函数提取配置键当前值

---

### 19. GetTimeInfo 完整实现 🟡

**问题**：
- `wallpaper_segments` 始终为空数组
- `time_points` 始终为空数组
- GUI 无法绘制时间轴可视化

**修复**：
- `WallpaperRecord` 添加 `time_constraints` 字段
- `build_space`/`rebuild_space` 接收 `Vec<ScannedWallpaper>` 保留时间约束
- `get_time_info()` 完整实现返回：
  - `time_points`: 所有关键时间点列表
  - `next_time_point`: 下一个时间点
  - `wallpaper_segments`: 每个壁纸的活跃时间段（含 all_day 标志）
- `TimeRange::crosses_midnight()` 新方法判断是否跨天

---

## ✅ 第四轮修复（代码质量审计）

### 20. Lock/Unlock/ToggleLock 代码重复 🟢

**问题**：三个函数有大量重复的模式判断、空间读写、事件发布逻辑。

**修复**：
- 新增 `LockAction` 枚举（Lock/Unlock/Toggle）
- 抽取 `modify_lock_state()` 统一处理锁定逻辑
- 抽取 `publish_space_updated_event()` 统一发布事件
- 额外改进：壁纸不存在时返回 `NotFound` 错误

---

### 21. apply_wallpaper 文件存在性检查 🟢

**问题**：`apply_wallpaper()` 直接传路径给 mpvpaper/swww，未检查文件存在性。

**修复**：在函数开头添加 `path.exists()` 检查，不存在时返回明确错误。

---

### 22. next_switch_secs 模式选择 Bug 🔴

**问题**：`GetStatus` 返回的 `next_switch_secs` 始终使用 `image_engine.interval`，即使当前是 Video 模式。

**修复**：根据当前模式选择正确的 interval：
```rust
let next_switch_secs = match mode {
    WallMode::Video => config.video_engine.interval,
    WallMode::Image => config.image_engine.interval,
};
```

---

### 23. Rescan 后壁纸有效性处理 🔴

**问题**：
- 之前的修复错误地清除了 `engine.current`
- 正确行为：空间重建后当前壁纸不在空间中，应保持显示旧壁纸，等待 interval 或 next

**修复**：
- 移除清除 `engine.current` 的逻辑
- 只输出警告日志
- 屏幕继续显示旧壁纸，`current_index = None`
- 下次 interval 或用户 next 时自动选择新壁纸

---

### 24. Prev 历史栈移至 Daemon 层 🔴

**问题**：
- `WallpaperSpace.history` 存储的是索引（index）
- 向量空间重建后索引可能指向错误的壁纸
- Prev 无法播放不在当前空间的壁纸

**修复**：
- `SharedState` 新增 `wallpaper_history: RwLock<VecDeque<PathBuf>>`
- `handle_next()` 将当前路径压入 daemon 层历史栈
- `handle_prev()` 从历史栈弹出路径，直接调用 `apply_wallpaper`
- Prev 不再依赖向量空间，可以播放任何历史壁纸
- 支持跨模式回退（Video → Image 或反向）

**行为变化**：
| 场景 | 旧行为 | 新行为 |
|------|--------|--------|
| Prev 壁纸不在空间 | 索引错误或失败 | 直接播放路径 |
| Prev 跨模式 | 只能回退当前模式 | 可以回退到任意模式 |
| 空间重建后 Prev | 可能播放错误壁纸 | 始终播放正确壁纸 |

---

## 🆕 新增 API

### ErrorCode

```rust
pub enum ErrorCode {
    // ... existing
    NoHistory,  // 没有历史记录（prev 无法回退）
}
```

### Request 变更

```rust
pub enum Request {
    Next {
        #[serde(default)]
        trigger_hint: Option<WallpaperTrigger>,  // 新增
    },
    Prev {
        #[serde(default)]
        trigger_hint: Option<WallpaperTrigger>,  // 新增
    },
    // ... existing
}
```

### Event (内部)

```rust
pub enum Event {
    ConfigChanged {  // 替代 ConfigReloaded
        key: String,
        old_value: serde_json::Value,
        new_value: serde_json::Value,
    },
    TimePointReached {
        time: String,       // 新增
        next_time: Option<String>,  // 新增
    },
    WallpaperChanged {
        path: PathBuf,
        mode: WallMode,
        trigger: WallpaperTrigger,  // 新增
    },
    // ... existing
}
```

### WallpaperRecord (运行时)

```rust
pub struct WallpaperRecord {
    pub path: PathBuf,
    pub angle: f64,
    pub locked: bool,
    pub last_played: Option<u64>,
    pub time_constraints: Vec<TimeRange>,  // 新增
}
```

### ModeData (持久化)

```rust
pub struct ModeData {
    pub pointer: f64,
    pub current_path: Option<PathBuf>,  // 新增
    pub items: Vec<PersistedRecord>,
}
```

### TimeRange (新增方法)

```rust
impl TimeRange {
    pub fn crosses_midnight(&self) -> bool;  // 新增
}
```

### SharedState (Daemon 层新增)

```rust
pub struct SharedState {
    // ... existing
    /// 壁纸历史栈（存储路径，用于 Prev 操作）
    pub wallpaper_history: RwLock<VecDeque<PathBuf>>,  // 新增
}
```

---

## 📁 影响的文件

| 文件 | 变更类型 |
|------|----------|
| `Cargo.toml` | 版本号 5.0.0 → 5.1.0 |
| `crates/lianwall-core/src/socket/protocol.rs` | 移除 Restart，添加 NoHistory，Next/Prev trigger_hint |
| `crates/lianwall-core/src/wallpaper/struct.rs` | WallpaperRecord 添加 time_constraints，ModeData 添加 current_path |
| `crates/lianwall-core/src/wallpaper/space.rs` | build_space/rebuild_space 接收 ScannedWallpaper |
| `crates/lianwall-core/src/wallpaper/time_range.rs` | TimeRange::crosses_midnight() |
| `crates/lianwall-core/src/algorithm/selector.rs` | 测试更新 |
| `crates/lianwall-daemon/src/state.rs` | GpuSnapshot、time_points、wallpaper_history |
| `crates/lianwall-daemon/src/event.rs` | ConfigChanged 替代 ConfigReloaded，TimePointReached 添加字段，WallpaperChanged 添加 trigger |
| `crates/lianwall-daemon/src/scheduler.rs` | 时间点监听、GPU cmd_tx、ConfigChanged 监听 |
| `crates/lianwall-daemon/src/connection.rs` | 事件过滤、immediate_sync、event_to_response 更新 |
| `crates/lianwall-daemon/src/handler/query.rs` | VRAM、时间点、可配置键、GetTimeInfo、next_switch_secs 模式选择 |
| `crates/lianwall-daemon/src/handler/command.rs` | Next/Prev 历史栈、LockAction 抽取、apply_wallpaper 检查、Rescan 改进 |
| `crates/lianwall-daemon/src/main.rs` | 启动恢复、关闭保存、时间过滤 |
| `crates/lianwall-cli/src/client.rs` | Next/Prev trigger_hint 传参 |
| `crates/lianwall-core/src/socket/client_legacy.rs` | Next/Prev trigger_hint 传参 |
| `crates/lianwall-core/src/socket/codec.rs` | 测试更新 |

---

## ⬆️ 升级指南

### 从 5.0.0 升级

1. 替换二进制文件
2. 重启 Daemon：`lianwall restart`

### 破坏性变更

- **移除 `Request::Restart`**：需要改为 Shutdown + 启动新进程
- **Prev 行为变更**：历史栈移至 daemon 层，存储路径而非索引，支持跨模式回退
- **ConfigReloaded → ConfigChanged**：内部事件结构变更

### JSON 协议兼容性

- `Request::Next` 和 `Request::Prev` 新增可选字段 `trigger_hint`，旧客户端不传此字段仍可正常工作
- 事件推送格式变化，订阅客户端需要处理新的字段

---
