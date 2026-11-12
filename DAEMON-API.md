# Lianwall Daemon API 文档

> 协议版本：3  
> 文档版本：5.6.0

## 📡 通信协议

### 传输层

- **协议**：Unix Domain Socket
- **地址**：默认 `/tmp/lianwall.sock`（可配置）
- **消息格式**：行分隔的 JSON（每条消息以 `\n` 结尾）
- **最大消息大小**：1 MB

### 请求 id（协议 v3 新增）

- 请求帧顶层可携带 `id` 字段（u64，客户端自增）：`{"id": 123, "cmd": "GetStatus"}`
- 响应帧回带相同的 `id`：`{"id": 123, "type": "Status", "payload": {...}}`，客户端按 id 精确匹配响应，不再依赖 FIFO 顺序
- 新客户端使用非零 `id`；`id=0` 用于服务端主动推送（Event、Subscribe 后的 immediate Status 同步）。
- 旧客户端不带 `id` 或使用 `id=0` 发送请求时，响应回带 `id: 0`，普通响应按请求顺序返回；事件通过 `type: "Event"` 区分，不能消耗普通响应队列。

### 请求、事件与完成顺序（5.6.0）

- 同一连接的命令按接收顺序进入全局串行队列。等待命令完成期间，连接继续读取后续请求并推送订阅事件。
- 非零 ID 的响应可以乱序返回。例如 Next 尚未完成时，后发 Ping 可先收到 Pong；客户端必须按 ID 匹配。
- 事件可能先于命令回执到达。事件是状态变化通知，不是某个请求的回执；不能据此认定另一个待处理请求也已完成。
- 查询读取当时可用的状态，不保证之前入队的命令已经执行；查询也可能等待共享状态锁。
- Subscribe 的 Subscribed 回执先发送，`immediate_sync=true` 时紧接着发送 ID=0 的 Status；false 时不额外发送 Status。
- 订阅 `status_changed` 或 `all` 后，调度器更新实际轮换期限时也会推送 ID=0 的 Status 快照。它不匹配普通 GetStatus 回调；客户端应使用它校准倒计时。旧客户端可忽略这种已知响应类型，不新增事件或协议字段。
- 客户端断开后，连接释放响应等待任务；已入队命令继续执行。重连后应查询实际状态，不能假定断线撤销了操作。

### 配置与关闭语义（5.6.0）

- `paths.mode` 是启动默认模式。SetConfig 修改它会保存配置，但不切换当前会话；当前模式以 Status 为准，实时切换使用 SetMode。
- 配置叶子字段在 daemon 与离线 CLI 共享类型、范围校验；写盘成功后才提交内存和 ConfigChanged，写盘失败保持内存不变。
- PauseRotation 只在内存临时暂停，其他配置的保存不会把临时的 interval=0 写盘。显式 SetConfig interval 则持久化该值并取消临时覆盖。
- 显式 interval=0 是配置暂停，Status 倒计时为 null、RotationStatus.paused=true；没有 saved_interval 时需设置正间隔恢复，而不是猜测之前的间隔。
- Shutdown 回执表示已受理；停止接受新命令，待执行命令返回未执行错误，尽量送完当前回执后关闭连接。关闭 hook 使用单独的总预算，不保证收到回执时所有清理已结束。

### 满载处理（5.6.0）

- 每连接最多 64 个尚未回收的命令响应等待任务。达到上限或全局命令队列已满时，立即返回 `internal_error`，消息明确包含 `request was not queued`，该次请求未入队执行。
- 未收到回执、读超时或断线与上述明确拒绝不同：结果可能未知，重试前应先确认状态。
- 为保持旧客户端 FIFO，服务端最多缓存 64 条等待前序响应的旧格式回执；超过上限会关闭该连接，释放等待资源。
- 以上限制不改变协议版本或 JSON 字段。事件仍受订阅过滤；广播缓冲丢事件时关闭连接，要求客户端重新订阅并读取快照，避免静默漏事件。

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
    "protocol_version": 3
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
    "protocol_version": 3,
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
| `current` | `string?` | 当前壁纸完整路径，启动后未切换过时为 `null` |
| `current_filename` | `string?` | 当前壁纸文件名，从 `current` 提取 |
| `engine` | `string` | 当前活跃引擎进程，`"mpvpaper"` / `"awww"` / `"swww"`（legacy）/ `"none"`（反映进程实际状态，非配置模式） |
| `total_wallpapers` | `number` | **当前模式**向量空间中的壁纸数（经时间过滤后的活跃数） |
| `locked_count` | `number` | 当前模式中被锁定的壁纸数 |
| `available_count` | `number` | 当前模式中可被选中的壁纸数（= `total` - `locked` - `in_cooldown`） |
| `scanned_count` | `number` | 目录扫描发现的壁纸文件总数（两个模式之和，含因时间约束未活跃的） |
| `vram_used_mb` | `number` | GPU 已用显存 (MB)，无 GPU 时为 0 |
| `vram_total_mb` | `number` | GPU 总显存 (MB)，无 GPU 时为 0 |
| `vram_degraded` | `boolean` | 是否处于显存降级状态（已自动切换到 Image 模式） |
| `uptime_secs` | `number` | daemon 进程运行时间（秒） |
| `protocol_version` | `number` | Socket 协议版本号 |
| `next_time_point` | `string?` | 下一个时间关键点 `"HH:MM"`，无时间约束壁纸时为 `null` |
| `time_points_count` | `number` | 时间关键点总数，0 表示所有壁纸均无时间约束 |
| `next_switch_secs` | `number \| null` | 距下次自动切换的剩余秒数；暂停时为 null，无自动切换期限 |

> **关系公式**：
> - `available_count = total_wallpapers - locked_count - cooldown_queue.len()`
> - `scanned_count >= total_wallpapers`（当无时间约束壁纸时两者相等）
>
> **数据源**：
> - `total_wallpapers`, `locked_count`, `available_count` → 从**当前模式**的向量空间计算
> - `scanned_count` → 从 `SharedState.scanned_counts` 读取（两个模式之和）
> - `next_switch_secs` → 从 `SharedState.next_switch` 计算剩余时间
> - `engine` → 检查 mpvpaper/awww 进程是否存活，而非读取配置模式

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
| `mode` | `WallMode` | 查询的模式（未指定时为当前模式） |
| `items` | `WallpaperPoint[]` | 向量空间中的壁纸列表（经时间过滤后的活跃壁纸） |
| `pointer_angle` | `number` | 当前指针角度 [0, 2π)，黄金角算法使用此值选择下一张壁纸 |
| `cooldown_size` | `number` | 当前冷却队列中的壁纸数（近期播放过，临时不参与选择） |
| `current_index` | `number?` | 当前壁纸在 items 中的索引，`null` 表示当前壁纸不在此空间中 |

**WallpaperPoint 结构**

| 字段 | 类型 | 描述 |
|------|------|------|
| `index` | `number` | 在 items 数组中的位置索引 |
| `filename` | `string` | 壁纸文件名 |
| `path` | `string` | 壁纸完整路径 |
| `angle` | `number` | 在向量空间中的角度 [0, 2π)，用于黄金角选择 |
| `locked` | `boolean` | 是否被用户锁定（锁定后不参与轮换） |
| `in_cooldown` | `boolean` | 是否在冷却队列中（近期播放过，临时不参与选择） |
| `is_current` | `boolean` | 是否为当前正在显示的壁纸 |

> **数据源**：从 `SharedState.video_space` 或 `SharedState.image_space`（根据请求的 mode）读取。
> 向量空间中的壁纸已经过时间约束过滤，只包含当前时间段活跃的壁纸。

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

**ModeSchedule 字段说明**

| 字段 | 类型 | 描述 |
|------|------|------|
| `scanned_count` | `number` | 目录扫描发现的该模式壁纸总数（含因时间约束未活跃的） |
| `active_count` | `number` | 当前时间段活跃且未锁定的壁纸数 |
| `time_points` | `string[]` | 所有时间关键点 `"HH:MM"` 格式，已排序 |
| `next_time_point` | `string?` | 下一个时间关键点 |
| `wallpaper_segments` | `WallpaperTimeSegment[]` | 壁纸时间分布（用于时间轴可视化） |

**WallpaperTimeSegment 结构**

| 字段 | 类型 | 描述 |
|------|------|------|
| `filename` | `string` | 壁纸文件名 |
| `path` | `string` | 壁纸完整路径 |
| `active_ranges` | `TimeRangeInfo[]` | 活跃时间范围列表，`all_day=true` 时为空数组 |
| `all_day` | `boolean` | 是否全天可用（无时间约束） |

> **数据源**：
> - `scanned_count` → `SharedState.scanned_counts`（各模式独立统计）
> - `active_count` → 从向量空间中过滤 `!locked` 的数量
> - `wallpaper_segments` → 从向量空间中读取每个壁纸的 `time_constraints`
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
| `paths.mode` | `string` | `"Video"` / `"Image"` | 启动默认模式（实时切换使用 SetMode） |
| `paths.video_dir` | `string` | - | 动态壁纸目录 |
| `paths.image_dir` | `string` | - | 静态壁纸目录 |
| `video_engine.interval` | `integer` | 10-86400 | 切换间隔（秒） |
| `video_engine.display` | `string` | - | 目标显示器 |
| `video_engine.mpvpaper_args` | `string[]` | - | mpvpaper 参数 |
| `video_engine.mpv_args` | `string[]` | - | mpv 参数 |
| `image_engine.interval` | `integer` | 10-86400 | 切换间隔（秒） |
| `image_engine.outputs` | `string` | - | 目标显示器 |
| `image_engine.awww_args` | `string[]` | - | awww 参数 |
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

#### 16. ReloadHooks

热更新重新加载 `~/.config/lianwall/hooks.toml`，无需重启 daemon。

**请求**
```json
{"cmd": "ReloadHooks"}
```

**响应**
```json
{"type": "Ok"}
```

**可能的错误**
- `config_error`: hooks.toml 不存在或解析失败

---

#### 17. ListHooks

列出当前加载的 hook 配置（Query 类型，可并发处理）。

**请求**
```json
{"cmd": "ListHooks"}
```

**响应**
```json
{
  "type": "HookList",
  "payload": [
    {
      "name": "notify-wallpaper",
      "on": "wallpaper_changed",
      "command": "notify-send 'LianWall' \"$LIANWALL_FILENAME\"",
      "mode": "image",
      "trigger": ["scheduled", "manual_next"],
      "timeout": 10,
      "enabled": true
    }
  ]
}
```

**HookInfo 字段说明**

| 字段 | 类型 | 描述 |
|------|------|------|
| `name` | `string` | 标识名（未配置时自动生成为 `hook(event_type)`） |
| `on` | `string` | 触发事件类型 |
| `command` | `string` | Shell 命令 |
| `mode` | `string?` | 模式过滤（`"video"` / `"image"`），`null` 表示不过滤 |
| `trigger` | `string[]?` | 触发原因过滤，`null` 表示不过滤 |
| `timeout` | `number` | 超时秒数（默认 10） |
| `enabled` | `boolean` | 是否启用 |

---

#### 18. Shutdown

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

#### 19. Subscribe

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

#### 20. Unsubscribe

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

#### 21. PauseRotation

暂停当前模式的自动轮换。interval 置 0，仅修改 daemon 内存，不写配置文件；daemon 重启后自动恢复。

**请求**
```json
{"cmd": "PauseRotation"}
```

**响应**
```json
{"type": "Ok"}
```

已暂停时返回 `InvalidRequest` 错误。

---

#### 22. ResumeRotation

恢复当前模式的自动轮换，还原暂停前的 interval。

**请求**
```json
{"cmd": "ResumeRotation"}
```

**响应**
```json
{"type": "Ok"}
```

未暂停时返回 `InvalidRequest` 错误。

---

#### 23. GetRotationStatus

查询当前模式的轮换暂停状态（Query，可并发）。

**请求**
```json
{"cmd": "GetRotationStatus"}
```

**响应**
```json
{
  "type": "RotationStatus",
  "payload": {
    "paused": true,
    "mode": "Video",
    "saved_interval": 600
  }
}
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
| `HookList` | ListHooks 响应 |
| `RotationStatus` | GetRotationStatus 响应 |

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
        {"field": "Engine", "value": "awww"}
      ]
    }
  }
}
```

**changes 字段枚举**

| field | value 类型 | 描述 |
|-------|-----------|------|
| `Mode` | `WallMode` | 模式变化（`"Video"` 或 `"Image"`） |
| `Engine` | `string` | 活跃引擎变化（`"mpvpaper"` / `"awww"` / `"swww"`（legacy）/ `"none"`） |
| `TotalWallpapers` | `number` | 向量空间壁纸总数变化 |
| `AvailableCount` | `number` | 可用壁纸数变化（已扣除锁定和冷却） |
| `LockedCount` | `number` | 锁定壁纸数变化 |
| `VramDegraded` | `boolean` | 显存降级状态变化 |

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

> **ReloadConfig 行为**：`key="all"`，`old_value` 和 `new_value` 为 `null`。
> ReloadConfig 会自动触发 Rescan（v5.1.1+），因此订阅者会先收到 `ConfigChanged`，随后收到 `SpaceUpdated` 事件。

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

**summary 字段说明**

| 字段 | 类型 | 描述 |
|------|------|------|
| `total` | `number` | 向量空间中的壁纸总数（时间过滤后的活跃数） |
| `available` | `number` | 可被选中的壁纸数（= `total` - `locked` - `in_cooldown`） |
| `locked` | `number` | 被用户锁定的壁纸数 |
| `in_cooldown` | `number` | 在冷却队列中的壁纸数 |

> **注意**：SpaceUpdated 事件按模式分别发布（先 Video 后 Image），每次 rescan 会收到两条。
> CLI 的 `rescan` 命令只等待第一条即返回。

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
| `engine_error` | 引擎错误（mpvpaper/awww 启动失败） |
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
← {"type":"Pong","payload":{"uptime_secs":3600,"protocol_version":3}}

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
| Command (ReloadHooks) | 5s |
| Query (ListHooks) | 2s |
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
| `"Image"` | 静态壁纸模式（使用 awww） |

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

### 静态参数键兼容性

`image_engine.awww_args` 替代旧键；Config JSON、GetConfig、SetConfig、配置 schema 和
ConfigChanged 事件统一使用新名，旧键不作为 API 别名。仅磁盘 TOML 读取支持一次迁移，
先备份 `config.toml.pre-awww.bak` 再原子替换。双键冲突拒绝加载；落盘迁移失败保留原文件，
允许读取内存迁移结果并记录警告。GUI 与脚本需配套升级，协议帧格式及协议版本保持不变。

### 连接资源与端点所有权

请求每帧最多 1 MiB（不含换行），超限或无效 UTF-8 关闭该连接；已入队命令仍可能继续执行。
响应写出/flush 共用 10 秒期限，持续不读的客户端将被断开；调用方应将断线后的未确认命令视为结果未知。
带 ID 响应按 ID 匹配；无 ID 的非法 JSON 错误同样遵守旧客户端 FIFO。

daemon 在初始化引擎/hook 前绑定端点，并持有 `<socket_path>.lock` 的非阻塞文件锁直至全部清理结束。
第二实例启动失败；普通文件/符号链接不删除，活跃监听者保留，仅清理连接明确拒绝的残留 socket。
锁文件保留在磁盘，锁本身随进程退出释放；运行期间不要删除锁文件。
端点清理核对 inode，只清理本实例创建的 socket。GUI 自定义端点入口为 `lianwall-gui --socket PATH`。
