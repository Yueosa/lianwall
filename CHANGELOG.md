# Changelog

所有版本的变更记录。

## [6.0.0](changelogs/CHANGELOG-6.0.0.md) — 2026-11-15

- 内置Vulkan图片/视频引擎、自动硬解/软件回退与跨媒体GPU淡化；awww/mpvpaper改为可选后端。
- daemon统一引擎诊断、输出发现、收藏/统计、显存测试和服务日志；CLI与JSON接口同步扩展。
- 新配置默认内置引擎，旧配置保留原后端；首次运行必须选择明确的单个目标输出。
- 配套GUI2.0.0；已验证Hyprland/NVIDIA单输出SDR，具体ABI、迁移与限制见完整日志。

## 已发布版本

| 版本 | 日期 | 主要变化 |
|------|------|----------|
| [6.0.0](changelogs/CHANGELOG-6.0.0.md) | 2026-11-15 | 内置图片/视频引擎、GPU过渡、收藏与诊断接口 |
| [5.6.0](changelogs/CHANGELOG-5.6.0.md) | 2026-11-12 | 外部引擎事务切换、通信与状态收敛、JSON/hook、原子文件与配置迁移 |
| [5.5.3](changelogs/CHANGELOG-5.5.3.md) | 2026-10-07 | 轮换状态竞态修复(GUI 倒计时卡死)、mpvpaper 首帧检测切换、daemon 日志修复 |
| [5.5.2](changelogs/CHANGELOG-5.5.2.md) | 2026-10-07 | mpvpaper 常驻 IPC 换片、切换动画优化、性能修复、Socket 协议 v3、暂停轮换 |
| [5.5.1](changelogs/CHANGELOG-5.5.1.md) | 2026-08-08 | 壁纸切换超时改为性能探针，不再误报业务 Timeout |
| [5.5.0](changelogs/CHANGELOG-5.5.0.md) | 2026-05-13 | 顺时针偏置概率选择算法、算法参数配置、GUI 联动稳定性修复 |
| [5.4.0](changelogs/CHANGELOG-5.4.0.md) | 2026-03-25 | awww / swww 双版本兼容，优先使用 awww |
| [5.3.0](changelogs/CHANGELOG-5.3.0.md) | 2026-02-26 | VRAM 自定义后端（Intel 等非主流 GPU）、手动覆盖 Socket 命令、`lianwall vram` 子命令组 |
| [5.2.0](changelogs/CHANGELOG-5.2.0.md) | 2026-02-25 | Hook 系统、视频切换无闪烁、CLI `--json` 全面结构化输出 |
| 5.1.6 | 2026-02-24 | daemon与cli的版本号同步（功能变更已合并到5.2.0） |
| 5.1.5 | 2026-02-23 | daemon与cli的版本号同步（功能变更已合并到5.2.0） |
| [5.1.4](changelogs/CHANGELOG-5.1.4.md) | 2026-02-23 | SetMode 引擎失败时模式回滚，防止级联错误 |
| [5.1.3](changelogs/CHANGELOG-5.1.3.md) | 2026-02-07 | 冷却回退排除当前壁纸、daemon 自动创建默认配置 |
| [5.1.2](changelogs/CHANGELOG-5.1.2.md) | 2026-02-07 | SetMode 事件缺失修复、冷却队列耗尽回退、浏览器式播放历史重构 |
| [5.1.1](changelogs/CHANGELOG-5.1.1.md) | 2026-02-07 | 全面审计修复：状态数据、调度器计时器、reload 超时、config 命令等 11 项 bug 修复 |
| [5.1.0](changelogs/CHANGELOG-5.1.0.md) | 2026-02-06 | TODO 审计修复、配置持久化、VRAM 集成、事件驱动调度 |
| [5.0.0](changelogs/CHANGELOG-5.0.0.md) | 2026-02-06 | CLI/Daemon 双文件分离、Socket V2 协议、订阅模式 |
| [4.0.0](changelogs/CHANGELOG-4.0.0.md) | 2026-02-05 | 黄金角算法、Daemon 架构、时间段目录 |
| 3.0.0 | 2026-02-03 | 零和博弈权重系统、显存监控 |

---

详细变更日志请查看 [changelogs/](changelogs/) 目录。
