# Changelog - 5.5.2

> 发布日期：2026-10-07

## 概述

5.5.2 是集中优化版本，落地了全部 5 个 PLAN 文档，对应 GitHub issue 的四项优化加一个 bonus 功能：

1. **mpvpaper 常驻 + IPC 换片**：视频壁纸切换从秒级冷启动降为毫秒级热切换
2. **切换动画**：图片默认过渡改 outer，视频支持 fade 淡入
3. **大量壁纸性能**：rescan 去 O(n²)，高频状态查询免深拷贝
4. **Socket 协议 v3**：请求/响应带 request id，响应精确匹配不错位
5. **暂停轮换**（bonus）：`lianwall rotation pause/resume/status`

**建议与 `lianwall-gui ≥ 1.5.0` 配套使用**(GUI 同步适配了协议 v3 与暂停轮换按钮;旧版 GUI 连接新 daemon 仍可工作,但混跑旧 daemon 时退回 FIFO 匹配语义)。

---

## 一、mpvpaper 常驻 + IPC 换片(PLAN-engine-ipc)

**问题**：视频壁纸每次切换都是「kill 旧 mpvpaper → spawn 新 mpvpaper」,mpv + 硬解冷启动开销大，切换延迟明显。

**修复**:
- mpvpaper 启动时注入 `--input-ipc-server`，常驻单个进程；切换时通过 mpv JSON IPC 发 `loadfile` 换片，不再产生新进程
- IPC 失败（进程崩溃、socket 不可用、超时 2s）自动回退原冷启动路径，功能不中断
- 新增配置 `video_engine.ipc_socket`(默认 `/tmp/lianwall-mpv.sock`)。**旧配置文件无此字段时自动启用**；显式设空字符串可禁用，退回每次冷启动
- daemon 退出时自动清理 IPC socket 残留文件

**影响文件**:`lianwall-core/src/config/{struct,default}.rs`、`lianwall-daemon/src/ipc.rs`(新增)、`handler/command.rs`、`handler/query.rs`、`main.rs`

## 二、切换动画优化(PLAN-transition-animation)

- **图片**：默认 `--transition-type` 从 fade 改为 **outer**(fade 在深色壁纸间过渡不明显)。只影响新生成的配置，已有配置不变
- **视频**：新增配置 `video_engine.transition`(`none`/`fade`,默认 `none`)。设为 `fade` 后，新视频首 0.4s 淡入（mpv lavfi 滤镜）；与 IPC 常驻配合，热切换时淡入自动重播，无黑屏
- 注意：fade 需要回读解码帧，与 `--hwdec=auto` 硬解同用可能掉帧，请按机器性能开启

**影响文件**:`lianwall-core/src/config/{struct,default}.rs`、`lianwall-daemon/src/handler/{command,query}.rs`

## 三、大量壁纸性能修复(PLAN-performance-fixes)

- **rescan 恢复锁定状态 O(n²) → O(n)**：改用 HashMap 查表，几千张壁纸的 rescan 从百万级比较降为线性
- **GetStatus 不再深拷贝整个壁纸空间**：改用计数摘要，GUI 高频轮询不再每次克隆两个空间的全量路径
- 选择算法热路径减少一次 Vec 分配

**影响文件**:`lianwall-core/src/wallpaper/space.rs`、`algorithm/selector.rs`、`lianwall-daemon/src/state.rs`、`handler/query.rs`

## 四、Socket 协议 v3(PLAN-socket-protocol)

**问题**：请求-响应靠 FIFO 顺序匹配，一个请求超时/丢失就导致后续响应全部错位（GUI 侧风险最大）；文档声称的长度前缀帧与实际 NDJSON 实现脱节；存在两套死代码。

**修复**:
- **协议版本 2 → 3**：请求帧可带 `id`(u64 客户端自增），响应回带相同 `id`，客户端按 id 精确匹配，不再依赖顺序
- `id` 缺失或为 0 = 服务端主动推送（Event、订阅同步状态）或旧客户端，完全向后兼容
- 删除死代码 `codec.rs`、`client_legacy.rs` 与 `is_query/is_command/is_subscription` 方法；模块文档修正为 NDJSON 描述
- `DAEMON-API.md` 同步升级 v3

**影响文件**:`lianwall-core/src/socket/*`、`lianwall-daemon/src/connection.rs`、`lianwall-cli/src/client.rs`、`DAEMON-API.md`

## 五、暂停/恢复轮换(PLAN-rotation-pause)

```bash
lianwall rotation pause    # 暂停当前模式的自动轮换
lianwall rotation resume   # 恢复
lianwall rotation status   # 查看状态
```

- 暂停只影响当前模式，状态存 daemon 内存（不写配置文件）,daemon 重启后自动恢复
- scheduler 对 interval=0 做了防 panic 处理（零间隔计时器会崩溃）
- `config set video_engine.interval 0` 现在也是合法值（等价于持久化暂停）
- GUI 1.5.0 仪表盘新增暂停/恢复按钮

**影响文件**:`lianwall-core/src/socket/protocol.rs`、`lianwall-daemon/src/{state,scheduler}.rs`、`handler/{command,query}.rs`、`lianwall-cli` 多文件

---

## 升级说明

- **配置文件**：向后兼容，无需手动修改。旧配置自动启用 mpv IPC;`transition` 默认 `none` 行为同现状
- **协议**：v3 完全向后兼容 v2 客户端（id 兜底 0)；建议 CLI/daemon/GUI 同步升级
- **AUR**:`lianwall-bin` + `lianwalld-bin` + `lianwall-gui-bin`(1.5.0）建议一起更新
