# CLI JSON 输出参考

`lianwall` CLI 支持 `--json` 标志，以机器可读的 JSON 格式输出所有结果，适用于脚本集成和自动化场景。

## 本轮配置兼容性变化

静态参数字段及配置键改为 `image_engine.awww_args`，完整 JSON 配置也只输出 `awww_args`。
旧 TOML 的 `swww_args` 在读取时备份并迁移；这不是 CLI/socket 的旧键别名。
脚本需更新字段与 get/set 键，两键冲突返回错误。备份与恢复规则见 README。
配置解析或迁移失败时返回非零码和 stdout JSON 错误，不再悄悄连接默认 socket。

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

多数操作类命令包含 `"success": true`：

```json
{ "success": true, ...其他字段 }
```

查询类命令直接输出对应的数据结构，无 `"success"` 包装；`reload/rescan` 成功使用 `"status":"ok"`，见目录操作。

### 错误响应

业务命令和参数错误在 **stdout** 输出一份 JSON，exit code 非零；stderr 用于非 JSON 诊断，不应作为结果解析入口：

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
| `engine` | `string` | 当前活跃后端；开发版原生图片/视频功能可返回 `native`，停止后返回 `none` |
| `total_wallpapers` | `number` | 时间过滤后的活跃壁纸总数 |
| `locked_count` | `number` | 锁定数量 |
| `available_count` | `number` | 可用数量（未锁定且不在冷却中） |
| `scanned_count` | `number` | 扫描的壁纸总数（含非活跃） |
| `vram_used_mb` | `number` | 显存使用量（MB） |
| `vram_total_mb` | `number` | 显存总量（MB，0 表示无法检测） |
| `vram_degraded` | `boolean` | 是否处于显存降级状态 |
| `uptime_secs` | `number` | 守护进程运行时间（秒） |
| `protocol_version` | `number` | 协议版本号 |
| `next_switch_secs` | `number \| null` | 下次自动切换倒计时（秒）；暂停时为 null |
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

> 这些命令等待 daemon 的异步事件完成，成功使用 `"status":"ok"`。超时或连接错误返回通用错误 JSON 和非零退出码；扫描可能仍在后台继续，可另行查询状态。

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
  "success": false,
  "error": "Connection timeout"
}
```

#### `rescan` — 重新扫描壁纸目录

```bash
lianwall --json rescan
```

响应结构与 `reload` 相同。

| 字段 | 类型 | 说明 |
|------|------|------|
| `status` | `"ok"` | 成功时的操作结果；失败使用通用错误结构 |
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

### 轮换暂停命令

#### `rotation pause` — 暂停当前模式的自动轮换

仅修改 daemon 内存，不写配置文件；daemon 重启后自动恢复。

```bash
lianwall --json rotation pause
```

```json
{
  "success": true,
  "action": "pause"
}
```

#### `rotation resume` — 恢复当前模式的自动轮换

```bash
lianwall --json rotation resume
```

```json
{
  "success": true,
  "action": "resume"
}
```

#### `rotation status` — 查看轮换暂停状态

```bash
lianwall --json rotation status
```

```json
{
  "paused": true,
  "mode": "Video",
  "saved_interval": 600
}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `paused` | bool | 当前模式是否已暂停 |
| `mode` | string | 当前模式（Video/Image） |
| `saved_interval` | number\|null | 暂停前的切换间隔（秒），未暂停为 null |

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


## 订阅事件流（5.6.0）

`lianwall --json subscribe [事件...]` 输出 NDJSON：每行一个紧凑 JSON，立即 flush，
依次包含 `Subscribed`、初始 `Status`，之后是 `Event` 和调度更新/重同步用的 `Status`。
类型及 payload 与 [Daemon API](DAEMON-API.md) 一致；不输出提示文字、颜色或空行。
事件可用短名（如 `wallpaper`）或正式名（如 `wallpaper_changed`），未知名称直接报错。

```sh
lianwall --json subscribe wallpaper status
```

订阅初始化有超时；成功同步后，空闲连接可以一直等待，不再因 10 秒没有事件退出。
EOF、解析错误或广播丢事件导致的断开，都会输出通用错误 JSON 并以非零码退出。
CLI 不自动重连或补发事件，消费者应重新订阅并使用初始快照恢复；事件流不是持久化日志。

```python
import json
import subprocess

process = subprocess.Popen(
    ["lianwall", "--json", "subscribe", "all"],
    stdout=subprocess.PIPE, text=True,
)
for line in process.stdout:
    frame = json.loads(line)
    if frame.get("success") is False:
        print("订阅结束:", frame["error"])
        continue
    if frame.get("type") == "Status":
        snapshot = frame["payload"]  # 包含初始化快照和之后的倒计时快照
    elif frame.get("type") == "Event":
        event = frame["payload"]  # event + data
print("退出码:", process.wait())
```

### 本轮兼容性变化

- 错误 JSON 从 stderr 移至 stdout；脚本应检查退出码并解析 stdout，避免把两个通道拼接后解析。
- 配置 get/set 在线与离线均保持真实 JSON 类型，数字/布尔/数组不再因 daemon 未运行而变成字符串。
- `config set` 使用字段类型解析文本：字符串字段的 `123` 仍是字符串，数组支持 JSON 数组或逗号分隔。
- `reload/rescan` 的超时不再返回成功退出码或 `status=timeout`，统一使用错误结构。
- `start --foreground` 会接管终端输出，不能与 `--json` 混用；后台 start 支持 JSON。
- `--help/--version` 属于 CLI 元信息，仍使用原有人类可读输出；`--json` 不带业务命令返回 JSON 错误。


### `lianwall engine --json`：引擎发现与实际解码信息

新增独立命令，通过GetEngineInfo返回payload对象；不改变`status --json`的字段。
例如`configured_decode="auto"`而`native.decoder="software"`表示实际软件解码，
`native.fallback_reason`提供回退原因；不能仅从`engine="native"`推断硬解。
`native.phase="preparing"`时pending包含目标，current保留原生已确认源。
未创建原生所有者时native为null；图片的decoder及requested_decode为null。
能力发现、采样计数及错误语义见DAEMON-API的GetEngineInfo说明。
旧daemon不识别请求时沿用CLI标准JSON错误及非零退出码，不生成虚假能力结果。


### `lianwall outputs --json` 与诊断事件流

`outputs --json`返回GetOutputs的payload：available、protocols、outputs、truncated和error。
尺寸为模式像素，刷新率单位毫赫兹；integer_scale不是分数缩放的实际逻辑比例。
发现不可用也是有效查询结果（available=false，退出码0），不要只检查退出码来判断能否选择输出。

`lianwall subscribe engine --json`输出NDJSON：Subscribed、Status、EngineInfo初始快照，
随后是EngineChanged事件完整runtime。控制revision用于丢弃旧/重复快照；重连后必须重新建立基线。
`subscribe all`保留旧8类事件集合，`subscribe all engine`显式追加诊断，不向旧脚本注入新枚举。
流断开/事件丢失导致断开仍以JSON错误及非零退出码结束，由调用方重连同步。


## 收藏与使用统计（6.0 开发中）

```bash
lianwall library --json
lianwall library --offset 128 --limit 128 --json
lianwall favorite '/wallpapers/forest.jpg' --json
lianwall favorite '/wallpapers/forest.jpg' --remove --json
lianwall subscribe library --json
```

`library` 一次输出一个分页对象，包含 `schema_version`、`revision`、`entries`、`total`、`next_offset`、`persistence_error`。每项为 `path`、`favorite`、`added_at`（Unix秒）、`last_used`（Unix秒或null）、`play_count`、`display_seconds`。默认/最大每页128项，CLI拒绝1–128以外的limit；`next_offset=null`表示结束。多页revision变化时应从头读取，重连后重置revision基线。

`favorite` 解析为规范化绝对路径，缺失文件允许使用索引中保留的绝对路径。只修改收藏、不修改轮换锁定；成功输出 `{"success":true}`，失败沿用 `{"success":false,"error":"..."}` 与非零退出码。`subscribe library` 显式订阅 `LibraryChanged` 事件，既有 `subscribe all` 内容不变。

统计从首次发现/成功切换开始，视频循环及同图重新应用不增加次数；驻留秒数不是观看时长。daemon停止期间不计时，定期与正常关闭保存。资料索引不随缩略图清理而删除；移动文件不自动搬迁旧条目。详见 [Daemon API](DAEMON-API.md#收藏与使用统计60-开发中)。


## 显存读取测试与服务日志（6.0 开发中）

```bash
lianwall vram test --json
lianwall logs --level warn --limit 128 --json
```

`vram test`只测试已保存读取配置，不改变监控、壁纸模式或暂停状态。返回一个`VramTest` payload对象，包含`schema_version`、`backend`、`success`、读数、命令输出、退出码、超时/截断标记、耗时与error。查询成功但无可用后端、命令失败或数据无效时，**退出码仍为0，脚本需检查对象的`success`字段**；这是一次诊断结果，不能等同于成功读数。连接/协议错误沿用非零退出码与既有错误对象。GUI另支持测试未保存命令。

`logs`返回一个`Logs` payload对象，字段包括`schema_version`、`entries`、`path`、`persistence_error`、`dropped_disk_entries`、`active_filter`和`environment_override`。每条entry为`sequence/timestamp/level/line`；时间为UTC RFC3339，记录从旧到新。`--level`为最低严重程度，`--limit`为1–128，默认128。只读取最近内存快照，不能用它恢复过滤掉的消息或假定完整历史。持久日志路径、轮转限制和`RUST_LOG`优先规则见[Daemon API](DAEMON-API.md)。

### 时间段目录

空间条目的可选字段 `time_constraints` 按从外到内列出本地时间范围，例如 `["08:00–18:00", "12:00–13:30"]`。所有范围必须同时满足，含起点、不含终点；跨午夜范围延续到次日。无约束时省略该字段。

`GetLibrary` 的条目可附带 `scanned_time_constraints`，格式同上。只有本次扫描中仍存在且受时间限制的文件带此字段。图库将其与当前模式的 `GetSpace` 合并，用于浏览和收藏时段外的壁纸；轮换候选仍由 `GetSpace` 决定。
