# Lianwall Daemon API 文档

> 协议版本：2  
> 文档版本：5.1.0

## 📡 通信协议

### 传输层

- **协议**：Unix Domain Socket
- **地址**：默认 `/tmp/lianwall.sock`（可配置）
- **消息格式**：行分隔的 JSON（每条消息以 `\n` 结尾）
- **最大消息大小**：1 MB

### 消息分类

| 类型 | 描述 | 执行方式 |
|------|------|----------|
| **Query** | 无状态查询 | 可并发处理 |
| **Command** | 状态修改 | 排队串行执行 |
| **Subscribe** | 订阅管理 | 立即处理 |
| **Event** | 服务端推送 | 主动推送 |

---

## 📥 请求 (Request)

所有请求使用 `cmd` 字段标识类型（internally tagged enum）。

### Query 请求（可并发）

#### 1. Ping

心跳检测，验证 daemon 是否运行。

**请求**
```json
{"cmd": "Ping"}
```

**响应**
```json
{
  "type": "Pong",
  "payload": {
    "uptime_secs": 3600,
    "protocol_version": 2
  }
}
```

---

#### 2. GetStatus

获取 daemon 完整状态。

**请求**
```json
{"cmd": "GetStatus"}
```

**响应**
```json
{
  "type": "Status",
  "payload": {
    "mode": "Video",
    "current": "/path/to/wallpaper.mp4",
    "current_filename": "wallpaper.mp4",
    "engine": "mpvpaper",
    "total_wallpapers": 42,
    "locked_count": 3,
    "available_count": 35,
    "scanned_count": 120,
    "vram_used_mb": 2048,
    "vram_total_mb": 8192,
    "vram_degraded": false,
    "uptime_secs": 3600,
    "protocol_version": 2,
    "next_time_point": "18:00",
    "time_points_count": 4,
    "next_switch_secs": 300
  }
}
```

**字段说明**

| 字段 | 类型 | 描述 |
|------|------|------|
| `mode` | `WallMode` | 当前运行模式，取值见 [WallMode 枚举](#wallmode) |
| `current` | `string?` | 当前壁纸完整路径 |
| `current_filename` | `string?` | 当前壁纸文件名 |
| `engine` | `string` | 当前引擎 `"mpvpaper"` / `"swww"` / `"none"` |
| `total_wallpapers` | `number` | 当前模式活跃壁纸数 |
| `locked_count` | `number` | 锁定的壁纸数 |
| `available_count` | `number` | 可用壁纸数（未锁定） |
| `scanned_count` | `number` | 扫描的壁纸总数（含不活跃） |
| `vram_used_mb` | `number` | 已用显存 (MB) |
| `vram_total_mb` | `number` | 总显存 (MB) |
| `vram_degraded` | `boolean` | 是否处于显存降级状态 |
| `uptime_secs` | `number` | daemon 运行时间（秒） |
| `protocol_version` | `number` | 协议版本 |
| `next_time_point` | `string?` | 下一个时间关键点 "HH:MM" |
| `time_points_count` | `number` | 时间关键点总数 |
| `next_switch_secs` | `number?` | 下次壁纸切换倒计时（秒，根据当前模式选择 interval） |

---

#### 3. GetSpace

获取向量空间快照。

**请求**
```json
{"cmd": "GetSpace"}
// 或指定模式
{"cmd": "GetSpace", "mode": "Video"}
```

**参数**

| 参数 | 类型 | 必需 | 描述 |
|------|------|------|------|
| `mode` | `WallMode` | 否 | 不传则使用当前模式，取值见 [WallMode 枚举](#wallmode) |

**响应**
```json
{
  "type": "Space",
  "payload": {
    "mode": "Video",
    "items": [
      {
        "index": 0,
        "filename": "sunset.mp4",
        "path": "/videos/sunset.mp4",
        "angle": 0.618,
        "locked": false,
        "in_cooldown": false,
        "is_current": true
      }
    ],
    "pointer_angle": 0.618,
    "cooldown_size": 5,
    "current_index": 0
  }
}
```

**字段说明**

| 字段 | 类型 | 描述 |
|------|------|------|
| `mode` | `string` | 查询的模式 |
| `items` | `WallpaperPoint[]` | 壁纸列表 |
| `pointer_angle` | `number` | 当前指针角度 [0, 2π) |
| `cooldown_size` | `number` | 冷却队列大小 |
| `current_index` | `number?` | 当前壁纸索引 |

**WallpaperPoint 结构**

| 字段 | 类型 | 描述 |
|------|------|------|
| `index` | `number` | 索引位置 |
| `filename` | `string` | 文件名 |
| `path` | `string` | 完整路径 |
| `angle` | `number` | 角度 [0, 2π) |
| `locked` | `boolean` | 是否锁定 |
| `in_cooldown` | `boolean` | 是否在冷却中 |
| `is_current` | `boolean` | 是否当前壁纸 |

---

#### 4. GetTimeInfo

获取时间调度信息（用于时间轴可视化）。

**请求**
```json
{"cmd": "GetTimeInfo"}
```

**响应**
```json
{
  "type": "TimeInfo",
  "payload": {
    "current_time": "14:30",
    "video_schedule": {
      "scanned_count": 50,
      "active_count": 30,
      "time_points": ["06:00", "12:00", "18:00", "22:00"],
      "next_time_point": "18:00",
      "wallpaper_segments": [
        {
          "filename": "morning.mp4",
          "path": "/videos/morning.mp4",
          "active_ranges": [
            {"start": "06:00", "end": "12:00", "crosses_midnight": false}
          ],
          "all_day": false
        },
        {
          "filename": "default.mp4",
          "path": "/videos/default.mp4",
          "active_ranges": [],
          "all_day": true
        }
      ]
    },
    "image_schedule": { ... }
  }
}
```

---

#### 5. GetConfig

获取配置。

**请求**
```json
// 获取全部配置
{"cmd": "GetConfig"}

// 获取指定配置项
{"cmd": "GetConfig", "key": "video_engine.interval"}
```

**参数**

| 参数 | 类型 | 必需 | 描述 |
|------|------|------|------|
| `key` | `string` | 否 | 配置键，不传则返回全部 |

**响应（全部配置）**
```json
{
  "type": "Config",
  "payload": {
    "key": null,
    "value": { /* 完整配置 JSON */ },
    "modifiable_keys": [
      {
        "key": "video_engine.interval",
        "value_type": "integer",
        "description": "动态壁纸切换间隔（秒）",
        "default": 600,
        "constraints": {
          "min": 10,
          "max": 86400
        }
      }
    ]
  }
}
```

---

### Command 请求（串行执行）

#### 6. Next

切换到下一张壁纸（使用黄金角算法选择）。

**请求**
```json
{"cmd": "Next"}
```

**高级用法**（内部使用，客户端通常不需要）
```json
{"cmd": "Next", "trigger_hint": "scheduled"}
```

**参数**

| 参数 | 类型 | 必需 | 描述 |
|------|------|------|------|
| `trigger_hint` | `WallpaperTrigger` | 否 | 触发原因，默认 `manual_next`，取值见 [WallpaperTrigger 枚举](#wallpapertrigger) |

**响应**
```json
{"type": "Ok"}
```

**可能的错误**
- `empty_space`: 没有可用壁纸

---

#### 7. Prev

切换到上一张壁纸（从 daemon 层历史栈弹出）。

> **注意**：Prev 不依赖向量空间，可以播放不在当前空间中的壁纸，支持跨模式回退。

**请求**
```json
{"cmd": "Prev"}
```

**响应**
```json
{"type": "Ok"}
```

**可能的错误**
- `no_history`: 历史栈为空，无法回退
- `engine_error`: 壁纸文件不存在或引擎启动失败

---

#### 8. SetWallpaper

指定特定壁纸。

**请求**
```json
{"cmd": "SetWallpaper", "path": "/path/to/wallpaper.mp4"}
```

**参数**

| 参数 | 类型 | 必需 | 描述 |
|------|------|------|------|
| `path` | `string` | 是 | 壁纸文件路径 |

**响应**
```json
{"type": "Ok"}
```

**可能的错误**
- `not_found`: 文件不存在
- `engine_error`: 引擎启动失败

---

#### 9. SetMode

切换模式。

**请求**
```json
{"cmd": "SetMode", "mode": "Image"}
```

**参数**

| 参数 | 类型 | 必需 | 描述 |
|------|------|------|------|
| `mode` | `WallMode` | 是 | 目标模式，取值见 [WallMode 枚举](#wallmode) |

**响应**
```json
{"type": "Ok"}
```

---

#### 10-12. Lock / Unlock / ToggleLock

锁定/解锁/切换壁纸锁定状态。

**请求**
```json
{"cmd": "Lock", "path": "/path/to/wallpaper.mp4"}
{"cmd": "Unlock", "path": "/path/to/wallpaper.mp4"}
{"cmd": "ToggleLock", "path": "/path/to/wallpaper.mp4"}
```

**参数**

| 参数 | 类型 | 必需 | 描述 |
|------|------|------|------|
| `path` | `string` | 是 | 壁纸文件路径 |

**响应**
```json
{"type": "Ok"}
```

**可能的错误**
- `not_found`: 壁纸不在当前向量空间中

---

#### 13. SetConfig

修改配置（立即生效并持久化）。

**请求**
```json
{"cmd": "SetConfig", "key": "video_engine.interval", "value": 300}
```

**参数**

| 参数 | 类型 | 必需 | 描述 |
|------|------|------|------|
| `key` | `string` | 是 | 配置键 |
| `value` | `any` | 是 | 新值（类型根据键决定） |

**支持的配置键（18 个）**

| 键 | 类型 | 约束 | 描述 |
|----|------|------|------|
| `paths.mode` | `string` | `"Video"` / `"Image"` | 运行模式 |
| `paths.video_dir` | `string` | - | 动态壁纸目录 |
| `paths.image_dir` | `string` | - | 静态壁纸目录 |
| `video_engine.interval` | `integer` | 10-86400 | 切换间隔（秒） |
| `video_engine.display` | `string` | - | 目标显示器 |
| `video_engine.mpvpaper_args` | `string[]` | - | mpvpaper 参数 |
| `video_engine.mpv_args` | `string[]` | - | mpv 参数 |
| `image_engine.interval` | `integer` | 10-86400 | 切换间隔（秒） |
| `image_engine.outputs` | `string` | - | 目标显示器 |
| `image_engine.swww_args` | `string[]` | - | swww 参数 |
| `vram.enabled` | `boolean` | - | 启用显存监控 |
| `vram.threshold_percent` | `number` | 5.0-50.0 | 降级阈值 |
| `vram.recovery_percent` | `number` | 20.0-80.0 | 恢复阈值 |
| `vram.check_interval` | `integer` | 1-60 | 检测间隔（秒） |
| `vram.cooldown_seconds` | `integer` | 10-600 | 冷却时间 |
| `daemon.socket_path` | `string` | - | Socket 路径 |
| `daemon.pid_path` | `string` | - | PID 文件路径 |
| `daemon.log_level` | `string` | `error`/`warn`/`info`/`debug`/`trace` | 日志级别 |

**响应**
```json
{"type": "Ok"}
```

**可能的错误**
- `invalid_request`: 无效的键或值
- `config_error`: 保存配置失败

---

#### 14. Rescan

重新扫描壁纸目录（异步执行）。

**请求**
```json
{"cmd": "Rescan"}
```

**响应**
```json
{"type": "Ok"}
```

> **注意**：立即返回 Ok，扫描在后台进行。订阅 `ScanProgress` 和 `SpaceUpdated` 事件获取进度。

---

#### 15. ReloadConfig

从文件重新加载配置。

**请求**
```json
{"cmd": "ReloadConfig"}
```

**响应**
```json
{"type": "Ok"}
```

---

#### 16. Shutdown

关闭 daemon。

**请求**
```json
{"cmd": "Shutdown"}
```

**响应**
```json
{"type": "Ok"}
```

---

### Subscribe 请求

#### 17. Subscribe

订阅事件。

**请求**
```json
{
  "cmd": "Subscribe",
  "events": ["wallpaper_changed", "status_changed"],
  "immediate_sync": true
}
```

**参数**

| 参数 | 类型 | 必需 | 描述 |
|------|------|------|------|
| `events` | `EventType[]` | 是 | 要订阅的事件类型列表 |
| `immediate_sync` | `boolean` | 否 | 订阅后立即推送当前状态 |

**EventType 枚举值**

| 值 | 描述 |
|----|------|
| `wallpaper_changed` | 壁纸切换 |
| `status_changed` | 状态变化（模式、引擎等） |
| `config_changed` | 配置变化 |
| `space_updated` | 向量空间更新 |
| `vram_changed` | 显存状态变化 |
| `time_point_reached` | 时间点触发 |
| `scan_progress` | 扫描进度 |
| `error` | 错误发生 |
| `all` | 所有事件 |

**响应**
```json
{
  "type": "Subscribed",
  "payload": {
    "session_id": "conn-1",
    "subscribed_events": ["wallpaper_changed", "status_changed"]
  }
}
```

> `immediate_sync: true` 时，会在 Subscribed 响应后立即推送一条 GetStatus 结果。

---

#### 18. Unsubscribe

取消订阅。

**请求**
```json
{"cmd": "Unsubscribe"}
```

**响应**
```json
{"type": "Unsubscribed"}
```

---

## 📤 响应 (Response)

所有响应使用 `type` 字段标识类型。

### 基础响应

| type | 描述 |
|------|------|
| `Ok` | 操作成功 |
| `Error` | 操作失败 |
| `Pong` | Ping 响应 |

### 数据响应

| type | 描述 |
|------|------|
| `Status` | GetStatus 响应 |
| `Space` | GetSpace 响应 |
| `TimeInfo` | GetTimeInfo 响应 |
| `Config` | GetConfig 响应 |

### 订阅响应

| type | 描述 |
|------|------|
| `Subscribed` | 订阅成功 |
| `Unsubscribed` | 取消订阅成功 |
| `Event` | 事件推送 |

---

## 📢 事件 (Event)

订阅后通过 `Response::Event` 推送。

### 1. WallpaperChanged

壁纸切换。

```json
{
  "type": "Event",
  "payload": {
    "event": "WallpaperChanged",
    "data": {
      "path": "/videos/sunset.mp4",
      "filename": "sunset.mp4",
      "mode": "Video",
      "trigger": "scheduled"
    }
  }
}
```

**trigger 枚举值**

| 值 | 描述 |
|----|------|
| `scheduled` | 定时切换 |
| `manual_next` | 用户手动 Next |
| `manual_prev` | 用户手动 Prev |
| `manual_set` | 用户指定壁纸 |
| `mode_switch` | 模式切换后的首张 |
| `vram_downgrade` | 显存降级触发 |
| `vram_upgrade` | 显存恢复触发 |
| `time_point_refresh` | 时间点触发重建 |
| `daemon_start` | daemon 启动 |

---

### 2. StatusChanged

状态变化。

```json
{
  "type": "Event",
  "payload": {
    "event": "StatusChanged",
    "data": {
      "changes": [
        {"field": "Mode", "value": "Image"},
        {"field": "Engine", "value": "swww"}
      ]
    }
  }
}
```

**changes 字段枚举**

| field | value 类型 | 描述 |
|-------|-----------|------|
| `Mode` | `string` | 模式变化 |
| `Engine` | `string` | 引擎变化 |
| `TotalWallpapers` | `number` | 壁纸总数变化 |
| `AvailableCount` | `number` | 可用数量变化 |
| `LockedCount` | `number` | 锁定数量变化 |
| `VramDegraded` | `boolean` | 降级状态变化 |

---

### 3. ConfigChanged

配置变化。

```json
{
  "type": "Event",
  "payload": {
    "event": "ConfigChanged",
    "data": {
      "key": "video_engine.interval",
      "old_value": 600,
      "new_value": 300
    }
  }
}
```

> ReloadConfig 时 `key="all"`，`old_value` 和 `new_value` 为 null。

---

### 4. SpaceUpdated

向量空间更新。

```json
{
  "type": "Event",
  "payload": {
    "event": "SpaceUpdated",
    "data": {
      "mode": "Video",
      "reason": "rescanned",
      "summary": {
        "total": 50,
        "available": 45,
        "locked": 3,
        "in_cooldown": 5
      }
    }
  }
}
```

**reason 枚举值**

| 值 | 描述 |
|----|------|
| `lock_changed` | 壁纸锁定/解锁 |
| `rescanned` | 目录重新扫描 |
| `time_point_refresh` | 时间点刷新 |
| `config_changed` | 配置变更 |

---

### 5. VramChanged

显存状态变化。

```json
{
  "type": "Event",
  "payload": {
    "event": "VramChanged",
    "data": {
      "action": "downgrade",
      "used_mb": 7500,
      "total_mb": 8192,
      "free_percent": 8.4
    }
  }
}
```

**action 枚举值**

| 值 | 描述 |
|----|------|
| `downgrade` | 降级到静态壁纸 |
| `upgrade` | 恢复到动态壁纸 |

---

### 6. TimePointReached

时间点触发。

```json
{
  "type": "Event",
  "payload": {
    "event": "TimePointReached",
    "data": {
      "time": "18:00",
      "next_time": "22:00"
    }
  }
}
```

---

### 7. ScanProgress

扫描进度（流式推送）。

```json
{
  "type": "Event",
  "payload": {
    "event": "ScanProgress",
    "data": {
      "mode": "Video",
      "dirs_scanned": 5,
      "files_found": 42,
      "completed": false
    }
  }
}
```

---

### 8. Error

错误事件。

```json
{
  "type": "Event",
  "payload": {
    "event": "Error",
    "data": {
      "code": "engine_error",
      "message": "mpvpaper failed to start",
      "recoverable": true
    }
  }
}
```

---

## ❌ 错误码 (ErrorCode)

| 错误码 | 描述 |
|--------|------|
| `unknown` | 未知错误 |
| `invalid_request` | 无效请求（命令不存在或参数错误） |
| `not_found` | 资源不存在（壁纸路径等） |
| `engine_error` | 引擎错误（mpvpaper/swww 启动失败） |
| `config_error` | 配置错误（无效配置值） |
| `permission_denied` | 权限错误 |
| `timeout` | 操作超时 |
| `empty_space` | 向量空间为空 |
| `no_history` | 没有历史记录（prev 无法回退） |
| `already_subscribed` | 已经订阅 |
| `not_subscribed` | 未订阅 |
| `internal_error` | 内部错误 |

---

## 📝 示例会话

### 基本查询
```
→ {"cmd":"Ping"}
← {"type":"Pong","payload":{"uptime_secs":3600,"protocol_version":2}}

→ {"cmd":"GetStatus"}
← {"type":"Status","payload":{...}}
```

### 切换壁纸
```
→ {"cmd":"Next"}
← {"type":"Ok"}
```

### 订阅事件
```
→ {"cmd":"Subscribe","events":["all"],"immediate_sync":true}
← {"type":"Subscribed","payload":{"session_id":"conn-1","subscribed_events":[...]}}
← {"type":"Status","payload":{...}}

// 之后收到事件推送
← {"type":"Event","payload":{"event":"WallpaperChanged","data":{...}}}
```

### 修改配置
```
→ {"cmd":"SetConfig","key":"video_engine.interval","value":300}
← {"type":"Ok"}

// 订阅者会收到
← {"type":"Event","payload":{"event":"ConfigChanged","data":{"key":"video_engine.interval","old_value":600,"new_value":300}}}
```

---

## ⏱️ 超时设置

| 请求类型 | 超时 |
|----------|------|
| Query (Ping, GetStatus, GetConfig) | 2s |
| Query (GetSpace, GetTimeInfo) | 5s |
| Command (Next, Prev, SetWallpaper) | 5s |
| Command (SetMode) | 10s |
| Command (Lock, Unlock, ToggleLock) | 2s |
| Command (SetConfig, ReloadConfig) | 5s |
| Command (Rescan) | 60s |
| Command (Shutdown) | 10s |
| Subscribe | 5s |

---

## 📚 类型定义

### WallMode

运行模式枚举。

| 值 | 描述 |
|----|------|
| `"Video"` | 动态壁纸模式（使用 mpvpaper） |
| `"Image"` | 静态壁纸模式（使用 swww） |

### WallpaperTrigger

壁纸切换触发原因枚举。

| 值 | 描述 |
|----|------|
| `scheduled` | 定时切换（interval 到期） |
| `manual_next` | 用户手动 Next |
| `manual_prev` | 用户手动 Prev |
| `manual_set` | 用户指定壁纸（SetWallpaper） |
| `mode_switch` | 模式切换后的首张壁纸 |
| `vram_downgrade` | 显存降级触发 |
| `vram_upgrade` | 显存恢复触发 |
| `time_point_refresh` | 时间点触发重建空间 |
| `daemon_start` | daemon 启动时应用 |

### EventType

可订阅的事件类型枚举。

| 值 | 描述 |
|----|------|
| `wallpaper_changed` | 壁纸切换 |
| `status_changed` | 状态变化（模式、引擎等） |
| `config_changed` | 配置变化 |
| `space_updated` | 向量空间更新 |
| `vram_changed` | 显存状态变化 |
| `time_point_reached` | 时间点触发 |
| `scan_progress` | 扫描进度 |
| `error` | 错误发生 |
| `all` | 所有事件（订阅时会展开为上述所有类型） |

### SpaceUpdateReason

向量空间更新原因枚举。

| 值 | 描述 |
|----|------|
| `lock_changed` | 壁纸锁定/解锁 |
| `rescanned` | 目录重新扫描 |
| `time_point_refresh` | 时间点刷新 |
| `config_changed` | 配置变更（目录改变） |

### VramAction

显存动作枚举。

| 值 | 描述 |
|----|------|
| `downgrade` | 降级到静态壁纸 |
| `upgrade` | 恢复到动态壁纸 |

### StatusChange

状态变化字段枚举（用于 StatusChanged 事件）。

| field | value 类型 | 描述 |
|-------|-----------|------|
| `Mode` | `WallMode` | 模式变化 |
| `Engine` | `string` | 引擎变化 |
| `TotalWallpapers` | `number` | 壁纸总数变化 |
| `AvailableCount` | `number` | 可用数量变化 |
| `LockedCount` | `number` | 锁定数量变化 |
| `VramDegraded` | `boolean` | 降级状态变化 |
