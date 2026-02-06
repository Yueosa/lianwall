# Changelog - 5.1.0

> 发布日期：2026-02-06

## 🎯 概述

本版本通过全面的 TODO 审计，修复了 8 个遗留问题，完善了 Daemon 的核心功能。主要改进包括：配置持久化、VRAM 状态集成、事件驱动调度器、完整的可配置键列表等。

## ✅ 修复的问题

### 1. 配置持久化 (#1)

**问题**：`SetConfig` 命令只修改内存中的配置，Daemon 重启后配置丢失。

**修复**：
- `handle_set_config()` 在修改内存配置后，自动调用 `lianwall_core::config::update()` 保存到文件
- 支持的配置键：`image_engine.interval`、`video_engine.interval`、`vram.enabled` 等

**影响文件**：
- `crates/lianwall-daemon/src/handler/command.rs`

---

### 2. Request::Restart 移除 (#2)

**问题**：`handle_restart` 当前只停止 Daemon，无法真正重启（进程无法自我重生）。

**决议**：从 Socket 协议中移除 `Request::Restart`。

**原因**：
- Daemon 无法自我重启（进程结束后无法启动自己）
- 重启逻辑应由客户端（CLI/GUI）负责：`shutdown` + 启动新进程
- CLI 的 `lianwall restart` 命令保留，内部实现为 stop + start

**影响文件**：
- `crates/lianwall-core/src/socket/protocol.rs` - 移除 `Request::Restart`
- `crates/lianwall-daemon/src/handler/command.rs` - 移除 `handle_restart()`
- `crates/lianwall-daemon/src/connection.rs` - 移除超时配置

---

### 3. VRAM 状态集成到 StatusInfo (#3)

**问题**：`StatusInfo` 中的 VRAM 字段始终为 0，无法反映真实 GPU 状态。

**修复**：
- 在 `SharedState` 中添加 `GpuSnapshot` 结构和 `gpu_snapshot` 字段
- GPU Monitor Task 定期更新 `gpu_snapshot`
- `get_status()` 读取 `gpu_snapshot` 填充 StatusInfo

**新增结构**：
```rust
pub struct GpuSnapshot {
    pub vram_info: Option<VramInfo>,
    pub degraded: bool,
    pub backend: GpuBackend,
    pub updated_at: Instant,
}
```

**影响文件**：
- `crates/lianwall-daemon/src/state.rs` - 添加 `GpuSnapshot`、`gpu_snapshot` 字段
- `crates/lianwall-daemon/src/handler/query.rs` - 读取真实 VRAM 信息
- `crates/lianwall-daemon/src/scheduler.rs` - GPU Monitor 更新快照

---

### 4. 时间调度信息集成 (#4)

**问题**：`StatusInfo.next_time_point` 始终为 `None`，时间调度信息缺失。

**修复**：
- 在 `SharedState` 中添加 `time_points: RwLock<BTreeSet<TimePoint>>` 缓存
- 扫描壁纸时收集所有时间点
- `get_status()` 计算 `next_time_point` 和 `time_points_count`

**影响文件**：
- `crates/lianwall-daemon/src/state.rs` - 添加 `time_points` 字段
- `crates/lianwall-daemon/src/main.rs` - 启动时收集时间点
- `crates/lianwall-daemon/src/handler/command.rs` - Rescan 后更新时间点
- `crates/lianwall-daemon/src/handler/query.rs` - 计算下一个时间点

---

### 5. Scheduler 集成 GPU 状态检查 (#5)

**问题**：`should_switch()` 函数未使用 GPU 状态做决策，低显存时仍会切换视频壁纸。

**修复**：
- `should_switch()` 读取 `GpuSnapshot` 检查降级状态
- 如果已降级且当前是 Video 模式，跳过切换
- 如果 VRAM 剩余低于阈值，跳过切换

**逻辑**：
```rust
async fn should_switch(state: &SharedState) -> bool {
    if config.vram.enabled {
        let gpu_snapshot = state.get_gpu_snapshot().await;
        
        // 已降级时不切换视频壁纸
        if gpu_snapshot.degraded && mode == WallMode::Video {
            return false;
        }
        
        // VRAM 不足时不切换
        if vram_info.free_percent < config.vram.threshold_percent {
            return false;
        }
    }
    true
}
```

**影响文件**：
- `crates/lianwall-daemon/src/scheduler.rs`

---

### 6. 配置变更事件监听 (#6)

**问题**：Scheduler 通过轮询检查配置变更，最坏情况要等一个完整 interval 才能响应。

**修复**：
- Scheduler 订阅 `EventBus`，监听 `ConfigReloaded` 和 `ModeChanged` 事件
- 收到事件时立即更新定时器间隔
- 移除了轮询检查配置的代码

**新增功能**：
- `get_interval_for_mode()` - 根据模式返回 Video/Image 对应的间隔
- 模式切换时自动切换间隔（Video 和 Image 可配置不同间隔）

**影响文件**：
- `crates/lianwall-daemon/src/scheduler.rs`

---

### 7. 可修改配置键列表 (#7)

**问题**：`ConfigSnapshot.modifiable_keys` 始终为 `None`，GUI 不知道哪些配置可以修改。

**修复**：
- 实现 `get_modifiable_keys()` 函数，返回所有可修改的配置键
- 每个键包含：类型、描述、默认值、约束条件

**支持的配置键（18 个）**：

| 分类 | 配置键 | 类型 | 约束 |
|------|--------|------|------|
| **paths** | `paths.mode` | string | enum: Video/Image |
| | `paths.video_dir` | string | - |
| | `paths.image_dir` | string | - |
| **video_engine** | `video_engine.interval` | integer | 10-86400 |
| | `video_engine.display` | string | - |
| | `video_engine.mpvpaper_args` | array | - |
| | `video_engine.mpv_args` | array | - |
| **image_engine** | `image_engine.interval` | integer | 10-86400 |
| | `image_engine.outputs` | string | - |
| | `image_engine.swww_args` | array | - |
| **vram** | `vram.enabled` | boolean | - |
| | `vram.threshold_percent` | number | 5.0-50.0 |
| | `vram.recovery_percent` | number | 20.0-80.0 |
| | `vram.check_interval` | integer | 1-60 |
| | `vram.cooldown_seconds` | integer | 10-600 |
| **daemon** | `daemon.socket_path` | string | - |
| | `daemon.pid_path` | string | - |
| | `daemon.log_level` | string | enum: error/warn/info/debug/trace |

**影响文件**：
- `crates/lianwall-daemon/src/handler/query.rs`

---

### 8. 事件硬编码值修复 (#8)

**问题**：多个事件字段是硬编码的，不反映真实状态。

| 字段 | 之前 | 之后 |
|------|------|------|
| `mode` | `WallMode::Video` | 从事件/状态获取 |
| `available` | `video + image` | 计算未锁定数 |
| `locked` | `0` | 计算锁定数 |
| `in_cooldown` | `0` | 计算冷却中数 |
| `action` | `Downgrade` | 根据状态判断 |

**修复**：
- `Event::SpaceUpdated` 携带 mode/total/available/locked/in_cooldown
- `Event::GpuStateUpdated` 携带 action/vram_info
- 事件转换逻辑使用真实数据

**影响文件**：
- `crates/lianwall-daemon/src/event.rs` - Event 结构变更
- `crates/lianwall-daemon/src/connection.rs` - 事件转换使用真实值
- `crates/lianwall-daemon/src/handler/command.rs` - 发布事件时填充真实数据

---

## 🔧 技术细节

### GPU Monitor 改进

GPU Monitor Task 现在使用 `lianwall_core::gpu::check()` 做降级/升级决策：

```rust
// 初始化 VramState
let vram_state = lianwall_core::gpu::init();

// 定期检查
let action = lianwall_core::gpu::check(&mut vram_state, &config.vram)?;

match action {
    VramAction::Downgrade => { /* 切换到 Image 模式 */ }
    VramAction::Upgrade => { /* 切换回 Video 模式 */ }
    VramAction::Keep => { /* 保持现状 */ }
}
```

### Scheduler 事件驱动

```rust
let mut event_rx = event_bus.subscribe();

loop {
    tokio::select! {
        _ = timer.tick() => { /* 定时切换 */ }
        
        Ok(event) = event_rx.recv() => {
            match event {
                Event::ConfigReloaded => { /* 立即更新 interval */ }
                Event::ModeChanged { to, .. } => { /* 切换 Video/Image interval */ }
                _ => {}
            }
        }
        
        _ = shutdown_rx.recv() => break,
    }
}
```

---

## 📁 影响的文件

| 文件 | 变更类型 |
|------|----------|
| `Cargo.toml` | 版本号 5.0.0 → 5.1.0 |
| `crates/lianwall-core/src/socket/protocol.rs` | 移除 `Request::Restart` |
| `crates/lianwall-daemon/src/state.rs` | 添加 `GpuSnapshot`、`time_points` |
| `crates/lianwall-daemon/src/event.rs` | Event 结构增强 |
| `crates/lianwall-daemon/src/scheduler.rs` | GPU 检查、事件驱动 |
| `crates/lianwall-daemon/src/connection.rs` | 事件转换修复、移除 Restart 超时 |
| `crates/lianwall-daemon/src/handler/query.rs` | VRAM 信息、时间点、可配置键 |
| `crates/lianwall-daemon/src/handler/command.rs` | 配置持久化、移除 Restart |
| `crates/lianwall-daemon/src/main.rs` | 启动时收集时间点 |

---

## ⬆️ 升级指南

### 从 5.0.0 升级

1. 替换二进制文件
2. 重启 Daemon：`lianwall restart`

### 破坏性变更

- **移除 `Request::Restart`**：如果你的 GUI 直接调用 Socket 发送 Restart 请求，需要改为 Shutdown + 启动新进程

---
