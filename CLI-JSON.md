# CLI JSON 输出参考

`lianwall` CLI 支持 `--json` 标志，以机器可读的 JSON 格式输出所有结果，适用于脚本集成和自动化场景。

## 快速上手

```bash
# 以 JSON 格式输出
lianwall --json <命令>

# 示例
lianwall --json status
lianwall --json next
lianwall --json config get paths.mode
```

`--no-color` 仅影响非 JSON 的终端彩色输出，在 `--json` 模式下无效。

---

## 通用约定

### 成功响应

所有操作类命令（会改变状态的命令）均包含 `"success": true`：

```json
{ "success": true, ...其他字段 }
```

查询类命令直接输出对应的数据结构，无 `"success"` 包装。

### 错误响应

任何命令出错时，均输出统一的错误结构（exit code 非零）：

```json
{
  "success": false,
  "error": "错误描述信息"
}
```

---

## 命令参考

### 查询命令

#### `status` — 守护进程状态

```bash
lianwall --json status
```

直接输出 `StatusInfo` 结构：

```json
{
  "mode": "Video",
  "current": "/home/user/wallpapers/video/nature.mp4",
  "current_filename": "nature.mp4",
  "engine": "mpvpaper",
  "total_wallpapers": 42,
  "locked_count": 3,
  "available_count": 38,
  "scanned_count": 50,
  "vram_used_mb": 1024,
  "vram_total_mb": 4096,
  "vram_degraded": false,
  "uptime_secs": 3600,
  "protocol_version": 2,
  "next_switch_secs": 287,
  "next_time_point": "22:00",
  "time_points_count": 3
}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `mode` | `"Video" \| "Image"` | 当前壁纸模式 |
| `current` | `string \| null` | 当前壁纸完整路径 |
| `current_filename` | `string \| null` | 当前壁纸文件名 |
| `engine` | `string` | 当前引擎名称 |
| `total_wallpapers` | `number` | 时间过滤后的活跃壁纸总数 |
| `locked_count` | `number` | 锁定数量 |
| `available_count` | `number` | 可用数量（未锁定且不在冷却中） |
| `scanned_count` | `number` | 扫描的壁纸总数（含非活跃） |
| `vram_used_mb` | `number` | 显存使用量（MB） |
| `vram_total_mb` | `number` | 显存总量（MB，0 表示无法检测） |
| `vram_degraded` | `boolean` | 是否处于显存降级状态 |
| `uptime_secs` | `number` | 守护进程运行时间（秒） |
| `protocol_version` | `number` | 协议版本号 |
| `next_switch_secs` | `number \| null` | 下次切换倒计时（秒） |
| `next_time_point` | `string \| null` | 下一个时间关键点（`"HH:MM"` 格式） |
| `time_points_count` | `number` | 时间关键点总数 |

---

#### `space` — 向量空间快照

```bash
lianwall --json space          # 当前模式
lianwall --json space --video  # 视频模式
lianwall --json space --image  # 图片模式
```

直接输出 `SpaceSnapshot` 结构：

```json
{
  "mode": "Video",
  "items": [
    {
      "index": 0,
      "filename": "nature.mp4",
      "path": "/home/user/wallpapers/video/nature.mp4",
      "angle": 2.399963,
      "locked": false,
      "in_cooldown": true,
      "is_current": true
    }
  ],
  "pointer_angle": 2.399963,
  "cooldown_size": 5,
  "current_index": 0
}
```

**`items[]` 元素字段：**

| 字段 | 类型 | 说明 |
|------|------|------|
| `index` | `number` | 在向量空间中的索引 |
| `filename` | `string` | 文件名（不含路径） |
| `path` | `string` | 完整路径 |
| `angle` | `number` | 分配的角度值 `[0, 2π)` |
| `locked` | `boolean` | 是否被锁定 |
| `in_cooldown` | `boolean` | 是否在冷却队列中 |
| `is_current` | `boolean` | 是否是当前壁纸 |

---

#### `time` — 时间调度信息

```bash
lianwall --json time
```

直接输出 `TimeScheduleInfo` 结构：

```json
{
  "current_time": "14:30",
  "video_schedule": {
    "scanned_count": 30,
    "active_count": 20,
    "time_points": ["00:00", "08:00", "22:00"],
    "next_time_point": "22:00",
    "wallpaper_segments": []
  },
  "image_schedule": {
    "scanned_count": 50,
    "active_count": 50,
    "time_points": [],
    "next_time_point": null,
    "wallpaper_segments": []
  }
}
```

---

#### `config show` — 显示完整配置

```bash
lianwall --json config show
```

直接输出完整的 `Config` 结构（JSON 格式）：

```json
{
  "paths": {
    "mode": "Video",
    "video_dir": "/home/user/wallpapers/video",
    "image_dir": "/home/user/wallpapers/image"
  },
  "video_engine": { ... },
  "image_engine": { ... },
  "vram": { ... }
}
```

---

#### `config get` — 获取单个配置项

```bash
lianwall --json config get paths.mode
```

```json
{
  "key": "paths.mode",
  "value": "Video"
}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `key` | `string` | 配置键名（点分隔） |
| `value` | `any` | 配置值（类型取决于具体键） |

---

#### `hook list` — 列出 Hook 配置

```bash
lianwall --json hook list
```

直接输出 `HookInfo[]` 数组：

```json
[
  {
    "name": "notify-on-change",
    "on": "wallpaper_changed",
    "command": "notify-send 'Wallpaper changed' '$LIANWALL_CURRENT_FILENAME'",
    "mode": "Video",
    "trigger": ["next", "prev"],
    "timeout": 10,
    "enabled": true
  }
]
```

**`HookInfo` 字段：**

| 字段 | 类型 | 说明 |
|------|------|------|
| `name` | `string` | Hook 标识名 |
| `on` | `string` | 触发事件名 |
| `command` | `string` | 执行的 Shell 命令 |
| `mode` | `string \| null` | 模式过滤（`"Video"` / `"Image"`，`null` 表示不过滤） |
| `trigger` | `string[] \| null` | 触发原因过滤，`null` 表示不过滤 |
| `timeout` | `number` | 超时秒数（默认 10） |
| `enabled` | `boolean` | 是否启用 |

---

### 壁纸控制命令

#### `next` — 切换下一张

```bash
lianwall --json next
```

```json
{
  "success": true,
  "current": "/home/user/wallpapers/video/forest.mp4",
  "current_filename": "forest.mp4",
  "mode": "Video"
}
```

#### `prev` — 切换上一张（历史回退）

```bash
lianwall --json prev
```

响应结构与 `next` 相同：

```json
{
  "success": true,
  "current": "/home/user/wallpapers/video/ocean.mp4",
  "current_filename": "ocean.mp4",
  "mode": "Video"
}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `success` | `true` | 操作成功 |
| `current` | `string \| null` | 切换后当前壁纸的完整路径 |
| `current_filename` | `string \| null` | 切换后当前壁纸的文件名 |
| `mode` | `"Video" \| "Image"` | 当前模式 |

---

#### `switch` — 切换模式（Video ↔ Image）

```bash
lianwall --json switch
```

```json
{
  "success": true,
  "mode": "Image",
  "current": "/home/user/wallpapers/image/landscape.jpg",
  "current_filename": "landscape.jpg"
}
```

#### `mode` — 指定切换模式

```bash
lianwall --json mode video
lianwall --json mode image
```

响应结构与 `switch` 相同：

```json
{
  "success": true,
  "mode": "Video",
  "current": "/home/user/wallpapers/video/nature.mp4",
  "current_filename": "nature.mp4"
}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `success` | `true` | 操作成功 |
| `mode` | `"Video" \| "Image"` | 切换后的模式 |
| `current` | `string \| null` | 切换后当前壁纸的完整路径 |
| `current_filename` | `string \| null` | 切换后当前壁纸的文件名 |

---

#### `set` — 设置指定壁纸

```bash
lianwall --json set /home/user/wallpapers/video/custom.mp4
```

```json
{
  "success": true,
  "path": "/home/user/wallpapers/video/custom.mp4",
  "current_filename": "custom.mp4",
  "mode": "Video"
}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `success` | `true` | 操作成功 |
| `path` | `string` | 设置的壁纸完整路径 |
| `current_filename` | `string` | 壁纸文件名 |
| `mode` | `"Video" \| "Image"` | daemon 自动识别后的实际模式 |

> `mode` 由 daemon 根据文件扩展名自动确定，可能与设置前不同。

---

### 锁定命令

#### `lock` — 锁定壁纸

```bash
lianwall --json lock /home/user/wallpapers/video/favorite.mp4
```

```json
{
  "success": true,
  "path": "/home/user/wallpapers/video/favorite.mp4",
  "filename": "favorite.mp4",
  "locked": true
}
```

#### `unlock` — 解锁壁纸

```bash
lianwall --json unlock /home/user/wallpapers/video/favorite.mp4
```

```json
{
  "success": true,
  "path": "/home/user/wallpapers/video/favorite.mp4",
  "filename": "favorite.mp4",
  "locked": false
}
```

#### `toggle-lock` — 切换锁定状态

```bash
lianwall --json toggle-lock /home/user/wallpapers/video/favorite.mp4
```

```json
{
  "success": true,
  "path": "/home/user/wallpapers/video/favorite.mp4",
  "filename": "favorite.mp4"
}
```

> `toggle-lock` 响应不包含 `locked` 字段，如需获取切换后的锁定状态，可在之后调用 `lianwall --json space` 查询。

| 字段 | 类型 | 说明 |
|------|------|------|
| `success` | `true` | 操作成功 |
| `path` | `string` | 壁纸完整路径 |
| `filename` | `string` | 壁纸文件名 |
| `locked` | `boolean` | （`lock`/`unlock` 专有）操作后的锁定状态 |

---

### 守护进程生命周期

#### `start` — 启动守护进程

```bash
lianwall --json start
```

**成功启动：**

```json
{
  "success": true,
  "pid": 12345
}
```

**已在运行：**

```json
{
  "success": true,
  "already_running": true
}
```

#### `stop` — 停止守护进程

```bash
lianwall --json stop
```

**成功停止：**

```json
{
  "success": true
}
```

**本来未运行：**

```json
{
  "success": true,
  "already_stopped": true
}
```

#### `restart` — 重启守护进程

```bash
lianwall --json restart
```

响应结构与 `start` 相同（调用内部 start 逻辑）：

```json
{
  "success": true,
  "pid": 12346
}
```

---

### 目录操作

> 这些命令等待 daemon 的异步事件完成，响应使用 `"status"` 字段（`"ok"` 或 `"timeout"`）而不是 `"success"`。

#### `reload` — 重新加载配置并扫描目录

```bash
lianwall --json reload
```

**成功：**

```json
{
  "status": "ok",
  "total": 80,
  "available": 73,
  "locked": 7,
  "video": {
    "total": 30,
    "available": 27,
    "locked": 3
  },
  "image": {
    "total": 50,
    "available": 46,
    "locked": 4
  }
}
```

**超时（30 秒内未收到 daemon 确认）：**

```json
{
  "status": "timeout",
  "message": "Timed out waiting for daemon confirmation"
}
```

#### `rescan` — 重新扫描壁纸目录

```bash
lianwall --json rescan
```

响应结构与 `reload` 相同。

| 字段 | 类型 | 说明 |
|------|------|------|
| `status` | `"ok" \| "timeout"` | 操作结果 |
| `total` | `number` | 壁纸总数（video + image） |
| `available` | `number` | 可用数量 |
| `locked` | `number` | 锁定数量 |
| `video` | `object \| null` | 视频模式详情 |
| `image` | `object \| null` | 图片模式详情 |

---

### 配置操作

#### `config set` — 设置配置项

```bash
lianwall --json config set video_engine.interval 600
```

```json
{
  "success": true,
  "key": "video_engine.interval",
  "old_value": 300,
  "new_value": 600
}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `success` | `true` | 操作成功 |
| `key` | `string` | 配置键名（点分隔） |
| `old_value` | `any` | 修改前的值 |
| `new_value` | `any` | 修改后的值 |

> 如果 daemon 未运行，修改直接写入配置文件，响应结构相同。

#### `config reset` — 重置配置为默认值

```bash
lianwall --json config reset
```

```json
{
  "success": true,
  "config": {
    "paths": { ... },
    "video_engine": { ... },
    "image_engine": { ... },
    "vram": { ... }
  }
}
```

> 在 `--json` 模式下，`config reset` 跳过交互确认，直接执行重置。

---

### Hook 命令

#### `hook reload` — 重新加载 hooks.toml

```bash
lianwall --json hook reload
```

```json
{
  "success": true
}
```

---

## 使用示例

### 脚本：监控壁纸切换

```bash
#!/bin/bash
result=$(lianwall --json next)
if echo "$result" | jq -e '.success' > /dev/null 2>&1; then
    filename=$(echo "$result" | jq -r '.current_filename')
    echo "已切换到: $filename"
else
    error=$(echo "$result" | jq -r '.error')
    echo "切换失败: $error" >&2
    exit 1
fi
```

### 脚本：获取当前壁纸模式

```bash
mode=$(lianwall --json status | jq -r '.mode')
echo "当前模式: $mode"
```

### 脚本：批量锁定壁纸

```bash
for file in ~/wallpapers/favorites/*.mp4; do
    result=$(lianwall --json lock "$file")
    echo "$result" | jq -r '"锁定: \(.filename)"'
done
```

### 脚本：检查 daemon 是否运行

```bash
if lianwall --json status > /dev/null 2>&1; then
    echo "daemon 正在运行"
else
    echo "daemon 未运行"
fi
```
