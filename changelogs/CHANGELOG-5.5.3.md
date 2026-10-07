# Changelog - 5.5.3

> 发布日期：2026-10-07

## 概述

5.5.3 是 5.5.2 发布当天实测反馈的修复版本,共三项:

1. **恢复轮换后 GUI 倒计时卡死**:daemon 状态更新竞态修复
2. **视频切换黑闪/卡顿**:固定等待改为首帧检测自适应等待
3. **daemon 日志全哑**:日志过滤器漏掉 lib crate,启动后 handler 日志全部丢失(存量老 bug)

只改 daemon(lianwalld),CLI 无代码变更(版本号随 workspace 同步)。**GUI 无需更新**,继续搭配 `lianwall-gui ≥ 1.5.0` 即可。

---

## 一、轮换状态查询竞态(GUI 倒计时卡 0:00)

**问题**:暂停轮换时 scheduler 把 `next_switch` 置为当前时间(剩余 0)。点恢复时,daemon 先广播 `ConfigChanged`,scheduler 异步重建 timer 之后才更新 `next_switch`。GUI 收到事件立即查询状态,抢在 scheduler 之前读到旧值 0,倒计时卡死在 0:00;scheduler 之后写入的新值没有再通知客户端,永远不会恢复。

**修复**:pause / resume / SetConfig(修改当前模式 interval)的 handler 在发布事件**之前**同步写入 `next_switch`,任何时刻客户端查询都能读到正确值:

- pause:`next_switch = now`(剩余 0)
- resume:`next_switch = now + saved_interval`
- SetConfig 当前模式 interval:`next_switch = now + interval`

GUI 侧无需改动。

**影响文件**:`lianwall-daemon/src/handler/command.rs`

## 二、mpvpaper 切换首帧检测

**问题**:Video→Video / Image→Video 切换时,旧代码固定等待 600ms / 800ms 再杀旧引擎。实测 4K60 视频冷启动到首帧渲染约 470-620ms 且随系统负载波动:慢于固定值时旧引擎被提前杀,黑屏闪烁;快于固定值时白等,双 4K 解码叠加时间拉长,画面卡顿。

**修复**:

- 捕获新 mpvpaper 的 stdout/stderr(后台 drain task 持续读取,防止管道写满阻塞 mpv)
- 检测到首帧渲染标志(`VO:` 初始化行 / `V:` 播放状态行)立即杀旧引擎,重叠窗口自适应压缩到真实冷启动时间
- 2 秒超时兜底:`--quiet` 等输出被禁用的场景检测不到时,退化为接近原来的固定等待行为,功能不中断

**影响文件**:`lianwall-daemon/src/handler/command.rs`

## 三、daemon 日志过滤器修复

**问题**:handler / scheduler / connection 等模块都在 lib crate `lianwall_daemon` 里,但默认日志过滤器只写了 bin target `lianwalld`,导致 daemon 启动后的所有 handler 日志(切换、暂停/恢复、配置变更等)被静默丢弃,只有 main.rs 的启动/关闭日志可见。这是存量老 bug,非 5.5.2 引入。

**修复**:默认过滤器补上 `lianwall_daemon=info`。

**影响文件**:`lianwall-daemon/src/main.rs`

---

## 升级建议

- `lianwalld` **必升**(三个修复全在 daemon)
- `lianwall` CLI 随版本同步即可,无行为变化
- `lianwall-gui` 无需更新,`≥ 1.5.0` 即可;GUI 倒计时卡死在升级 daemon 后自动恢复
- 协议版本不变(v3),任意 5.5.x 的 CLI / daemon / GUI 1.5.0 可自由组合
