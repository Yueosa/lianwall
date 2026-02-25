# Changelog - 5.3.0

> 发布日期：2026-02-26

---

## 📊 版本摘要

| 分类 | 数量 |
|------|------|
| 🔵 新功能 | 3 |
| 🟣 改进 | 1 |

---

## 🎯 概述

5.3.0 围绕 **VRAM 显存监控** 进行重磅扩展，新增两大能力：

- **方案 A — 自定义后端**：支持用户提供 Shell 脚本查询显存，突破此前仅支持 NVIDIA/AMD 的限制，Intel 及其他 GPU 用户也能使用自动显存监控。
- **方案 B — 手动覆盖**：新增 `lianwall vram` CLI 子命令组，允许脚本或手动操作直接强制降级/恢复，绕过自动检测。

两方案可同时使用，互不干扰。

---

## 🔵 新功能

### 1. VRAM 自定义后端（Custom Backend）

**背景**：5.2.0 及以前的 VRAM 监控仅支持 `nvidia-smi`（NVIDIA）和 `rocm-smi`（AMD）两种后端，Intel 及其他 GPU 用户无法使用自动显存监控。

**实现**：新增 `backend` 配置项，设为 `"custom"` 后 daemon 将周期性执行 `custom_command` 并解析 stdout 获取显存信息，复用现有的阈值/冷却/恢复决策逻辑。

#### 配置

```toml
# ~/.config/lianwall/config.toml
[vram]
enabled = true
backend = "auto"       # "auto"（自动检测，默认）或 "custom"（自定义命令）

# backend = "custom" 时生效
# command stdout 必须包含（顺序不限）：
#   used_mb=<整数MiB>
#   total_mb=<整数MiB>
# custom_command = ""

# Intel 示例（需安装 intel_gpu_top）：
# custom_command = "~/.config/lianwall/intel_vram.sh"
#
# NVIDIA 等效示例（可用于测试）：
# custom_command = "nvidia-smi --query-gpu=memory.used,memory.total --format=csv,noheader,nounits | awk -F', ' '{print \"used_mb=\"$1\"\\ntotal_mb=\"$2}'"
```

#### 脚本输出格式

自定义命令的 stdout 必须输出以下两行（顺序不限，大小写不敏感，多余行忽略）：

```
used_mb=1234
total_mb=8192
```

#### 启动验证

daemon 启动时（`gpu_monitor` 初始化阶段）进行验证：

| 情况 | 处理 |
|------|------|
| `backend = "custom"` 且 `custom_command` 为空 | 记录 error 日志，禁用 GPU Monitor |
| 试运行命令失败（非零退出码 / 解析失败） | 记录 error 日志，禁用 GPU Monitor |
| 试运行成功 | 正常启动，记录 info 日志 |

禁用为运行时行为，**不修改配置文件**，daemon 重启后重新尝试。

#### 动态配置支持

支持通过 `lianwall config set` 动态修改无需重启：

```bash
lianwall config set vram.backend custom
lianwall config set vram.custom_command "~/.config/lianwall/intel_vram.sh"
```

---

### 2. VRAM 手动覆盖（Manual Override）

**背景**：自定义后端适合周期性自动检测，但某些场景需要脚本直接触发降级/恢复（如 cron 按游戏进程判断、udev 规则响应 GPU 事件等）。

**实现**：新增 `Request::VramOverride` Socket 命令，可直接强制切换模式并保持该状态，直到手动 reset 或 daemon 重启。

#### 覆盖状态

| 状态 | 说明 |
|------|------|
| `None`（默认） | 自动检测，由 GPU Monitor 决策 |
| `Some(true)` | 强制降级（Image 模式），自动检测循环跳过 |
| `Some(false)` | 强制升级（Video 模式），自动检测循环跳过 |

覆盖状态仅存在内存中，**daemon 重启后自动清除**，避免用户忘记 reset 永久卡在某模式。

#### 状态查询

`GetStatus` 响应新增 `vram_override` 字段：

```json
{
  "vram_override": null,        // None：自动
  "vram_override": true,        // 强制降级
  "vram_override": false        // 强制升级
}
```

---

### 3. CLI `lianwall vram` 子命令组

新增 `lianwall vram` 子命令，方便在脚本中手动控制显存状态：

```bash
lianwall vram downgrade    # 强制切换到 Image 模式（忽略真实显存用量）
lianwall vram upgrade      # 强制切换回 Video 模式
lianwall vram reset        # 清除手动覆盖，恢复自动检测
lianwall vram status       # 查看显存使用量 + 覆盖状态
```

`vram status` 示例输出：

```
──────────── VRAM Status ────────────
  Usage      : 3200/8192 MB (61% free)
  Auto Status: Normal (Video mode allowed)
  Override   : None (auto)
```

`--json` 模式：

```json
{
  "vram_used_mb": 3200,
  "vram_total_mb": 8192,
  "vram_degraded": false,
  "vram_override": null
}
```

#### 脚本集成示例

```bash
# 游戏启动时强制降级
/usr/bin/game_launcher && lianwall vram downgrade

# 游戏退出后恢复
trap "lianwall vram reset" EXIT
```

---

## 🟣 改进

### 4. 向前兼容：无感升级旧配置

新增的 `backend` 和 `custom_command` 字段均使用 `#[serde(default)]`，旧版 `config.toml` 不含这些字段时 daemon 自动填充默认值（`backend = "auto"`、`custom_command = ""`），**不报错、不修改配置文件**，完全透明升级。

---

## 📁 变更文件清单

### `lianwall-core`

| 文件 | 变更 |
|------|------|
| `crates/lianwall-core/src/config/struct.rs` | 新增 `VramBackend` 枚举；`VramConfig` 新增 `backend`、`custom_command` 字段 |
| `crates/lianwall-core/src/config/default.rs` | 默认 TOML 新增 `backend = "auto"` 及自定义命令注释模板 |
| `crates/lianwall-core/src/gpu/struct.rs` | `GpuBackend` 新增 `Custom { command: String }` 变体 |
| `crates/lianwall-core/src/gpu/async_ops.rs` | 新增 `query_custom_async()`、`parse_custom_output()`；`query_vram()` 新增 `Custom` 分支 |
| `crates/lianwall-core/src/gpu/monitor.rs` | 新增 `init_with_config(config)`；`check()` 改用 `.clone()` 适配非 Copy 后端 |
| `crates/lianwall-core/src/gpu/mod.rs` | 导出 `init_with_config`；`query_vram_sync()` 支持 `Custom` 后端 |
| `crates/lianwall-core/src/socket/protocol.rs` | 新增 `Request::VramOverride`、`VramOverrideAction` 枚举；`StatusInfo` 新增 `vram_override` 字段 |
| `crates/lianwall-core/src/socket/mod.rs` | 导出 `VramOverrideAction` |

### `lianwall-daemon`

| 文件 | 变更 |
|------|------|
| `crates/lianwall-daemon/src/state.rs` | `SharedState` 新增 `vram_override: RwLock<Option<bool>>` |
| `crates/lianwall-daemon/src/scheduler.rs` | 改用 `init_with_config()`；新增 Custom 后端启动验证；loop 检查 `vram_override` 跳过自动检测 |
| `crates/lianwall-daemon/src/handler/command.rs` | 路由 `VramOverride`；实现 `handle_vram_override()`；`SetConfig` 支持 `vram.backend` / `vram.custom_command` |
| `crates/lianwall-daemon/src/handler/query.rs` | `GetStatus` 响应加入 `vram_override`；`GetConfig` / `get_modifiable_keys()` 加入两个新键 |
| `crates/lianwall-daemon/src/connection.rs` | `get_request_timeout()` 新增 `VramOverride`（10s）|

### `lianwall-cli`

| 文件 | 变更 |
|------|------|
| `crates/lianwall-cli/src/commands.rs` | 新增 `Command::Vram`、`VramAction` 枚举 |
| `crates/lianwall-cli/src/main.rs` | 路由 `Command::Vram` |
| `crates/lianwall-cli/src/handlers/vram.rs` | 新增：`handle_vram()` 完整实现 |
| `crates/lianwall-cli/src/handlers/mod.rs` | 导出 `handle_vram` |
| `crates/lianwall-cli/src/client.rs` | 新增 `vram_override(action)` 方法 |
