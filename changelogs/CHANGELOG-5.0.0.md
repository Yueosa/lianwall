# Changelog - 5.0.0

> 发布日期：2026-02-06

## 🎉 主要变化

### CLI / Daemon 双文件分离

v4.0.0 采用单文件架构（`lianwall` 内含 CLI 和 Daemon），v5.0.0 彻底分离：

| 二进制 | 用途 | 大小 |
|--------|------|------|
| `lianwall` | CLI 客户端 | ~1.3 MB |
| `lianwalld` | 守护进程 | ~1.5 MB |

**为什么分离？**

- **职责清晰**：CLI 只负责发送命令，Daemon 负责状态管理
- **独立升级**：可以单独升级 CLI 而不影响运行中的 Daemon
- **体积更小**：CLI 不再包含 tokio 运行时，启动更快
- **便于调试**：Daemon 可以独立运行、附加调试器
- **AUR 友好**：支持拆分成 `lianwall-bin` 和 `lianwalld-bin` 两个包

### Socket V2 协议

重写了整个通信协议，更加健壮和易于扩展：

| 特性 | V1 (4.0.0) | V2 (5.0.0) |
|------|------------|------------|
| 帧格式 | 行分隔 JSON | 长度前缀帧 (4 字节 + JSON) |
| 最大消息 | 无限制 | 16 MB |
| 订阅模式 | ❌ | ✅ |
| 事件推送 | ❌ | ✅ |
| 错误码 | 字符串 | 枚举类型 |

**长度前缀帧** 解决了以下问题：
- 大 JSON 消息在网络上被分片
- 难以区分消息边界
- 无法有效处理二进制数据

### 订阅模式 (Subscribe)

GUI 和脚本现在可以订阅事件，实时接收状态变化：

```rust
// 订阅壁纸变化和模式切换
client.subscribe(vec![
    EventType::WallpaperChanged,
    EventType::ModeChanged,
], true)?;

// 接收事件（阻塞）
loop {
    let event = client.receive_event()?;
    match event {
        Event::WallpaperChanged { path, mode } => { ... }
        Event::ModeChanged { mode } => { ... }
        _ => {}
    }
}
```

支持的事件类型：

| 事件 | 触发时机 |
|------|----------|
| `WallpaperChanged` | 壁纸切换（next/prev/set） |
| `ModeChanged` | 模式切换 |
| `SpaceRefreshed` | 时间点刷新、rescan |
| `ConfigChanged` | 配置修改 |
| `EngineStatus` | 引擎状态变化 |
| `VramStatus` | VRAM 状态变化、降级事件 |
| `ScanProgress` | 目录扫描进度 |

### 新增命令

#### `rescan` - 重新扫描壁纸目录

```bash
lianwall rescan
```

与 `reload` 的区别：
- `rescan`：只重新扫描目录，发现新壁纸
- `reload`：重新读取配置文件，如果目录配置变了也会触发扫描

#### `subscribe` - 订阅事件（调试用）

```bash
lianwall subscribe --events wallpaper,mode --sync
```

CLI 内置的订阅调试工具，用于验证事件推送是否正常。

## 🏗️ 架构变化

```
v4.0.0:
┌─────────────────────────────────────┐
│            lianwall                  │
│  ┌─────────────┬─────────────────┐  │
│  │   CLI mode  │  Daemon mode    │  │
│  │  (default)  │  (--daemon)     │  │
│  └─────────────┴─────────────────┘  │
└─────────────────────────────────────┘

v5.0.0:
┌─────────────────┐    ┌─────────────────┐
│    lianwall     │    │    lianwalld    │
│  (CLI client)   │───▶│    (Daemon)     │
│                 │    │                 │
│  • 轻量级       │    │  • Tokio 异步   │
│  • 同步 IO      │    │  • 状态管理     │
│  • 无 Tokio     │    │  • 定时任务     │
└─────────────────┘    └─────────────────┘
```

### 模块结构变化

```
v4.0.0:
crates/
└── lianwall-cli/
    ├── main.rs
    ├── commands.rs
    ├── handlers.rs
    └── daemon/          # Daemon 嵌入 CLI
        ├── handler.rs
        ├── scheduler.rs
        └── server.rs

v5.0.0:
crates/
├── lianwall-cli/        # 独立 CLI
│   ├── main.rs
│   ├── commands.rs
│   ├── handlers.rs
│   ├── client.rs        # Socket 客户端封装
│   └── subscribe.rs     # 订阅命令
│
└── lianwall-daemon/     # 独立 Daemon
    ├── main.rs
    ├── handler.rs
    ├── scheduler.rs
    └── server.rs
```

### Daemon 查找逻辑

`lianwall start` 会自动查找 `lianwalld`：

1. **同目录优先**：先检查 `lianwall` 所在目录是否有 `lianwalld`
2. **PATH 查找**：如果同目录没有，使用 `which lianwalld` 查找

这样支持以下场景：
- 开发测试：两个二进制放同一目录
- 系统安装：两个二进制都在 `/usr/bin/` 或 `~/.local/bin/`
- AUR 安装：`lianwall-bin` 依赖 `lianwalld-bin`

## 📦 安装变化

### 手动安装

```bash
# v4.0.0 - 单文件
cp lianwall_4.0.0_linux_x86_64 ~/.local/bin/lianwall

# v5.0.0 - 双文件
cp lianwall_5.0.0_linux_x86_64 ~/.local/bin/lianwall
cp lianwalld_5.0.0_linux_x86_64 ~/.local/bin/lianwalld
```

### AUR

```bash
# 安装 CLI（自动拉取 lianwalld-bin 依赖）
paru -S lianwall-bin
```

### 一键安装脚本

```bash
curl -fsSL https://raw.githubusercontent.com/Yueosa/lianwall/main/install.sh | bash
```

## 🚀 升级指南

从 v4.0.0 升级到 v5.0.0：

1. **停止旧 Daemon**
   ```bash
   lianwall stop
   ```

2. **替换二进制**
   ```bash
   cp lianwall_5.0.0_linux_x86_64 ~/.local/bin/lianwall
   cp lianwalld_5.0.0_linux_x86_64 ~/.local/bin/lianwalld
   chmod +x ~/.local/bin/lianwall ~/.local/bin/lianwalld
   ```

3. **启动新版本**
   ```bash
   lianwall start
   lianwall status
   ```

**配置文件兼容**：v5.0.0 完全兼容 v4.0.0 的配置文件，无需修改。

## 🐛 问题修复

- 修复：大 JSON 响应在网络上被截断的问题（长度前缀帧）
- 修复：并发连接时事件丢失的问题（每个连接独立的事件队列）
- 优化：CLI 启动速度提升（移除 Tokio 运行时）

## ⚠️ 破坏性变化

- **Socket 协议不兼容**：v5.0.0 的 CLI 无法与 v4.0.0 的 Daemon 通信（反之亦然）
- **必须同时升级**：`lianwall` 和 `lianwalld` 需要同时升级到 v5.0.0
