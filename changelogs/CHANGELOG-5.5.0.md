# Changelog - 5.5.0

> 发布日期：2026-05-13

## 概述

5.5.0 是一轮面向“随机感”和 GUI 联动稳定性的功能更新。

本版本将原有的对称最近邻黄金角选择器升级为顺时针偏置的概率选择模型，在保留黄金角全局遍历特性的同时，减少“指针顺时针旋转却频繁向左翻转”的违和感；同时补齐算法配置项、持久化随机状态，并解决 GUI 反馈中暴露出的状态联动问题。

---

## 功能优化

### 1. 顺时针偏置概率选择算法

**背景**：
- 旧算法采用对称最近邻规则
- 在数学上天然会出现约 50% 左右的左翻选择
- 冷却和锁定会进一步放大“顺时针推进但结果回头”的观感问题

**更新**：
- 核心选择器切换为“顺时针偏置 + softmax 概率采样”
- 右侧近点拥有更高概率，左侧候选仅在足够接近时才有机会胜出
- 保留黄金角指针推进与硬冷却窗口机制

**效果**：
- 保留随机性
- 降低无方向感的来回跳动
- 让壁纸切换更符合“顺时针扫过空间”的主观直觉

### 2. 新增算法配置段

新增 `[algorithm]` 配置段：

```toml
[algorithm]
strategy = "clockwise_biased_softmax"
bias_lambda = 0.35
temperature = 0.28
```

**说明**：
- 旧配置文件无需重建，缺失字段会自动使用默认值
- 新参数已接入 daemon 查询、daemon 配置修改、CLI `config get/set`

### 3. 选择器随机状态持久化

**更新**：
- 为每个模式空间增加 `selector_nonce`
- 将概率采样的随机步进状态纳入持久化文件

**效果**：
- 算法在重启后不会完全丢失采样上下文
- 更利于调试、测试和长期行为复现

---

## 问题修复

### 1. 手动/历史跨模式切换事件链补全

修复以下场景中 `ModeChanged` 事件缺失的问题：

- `SetWallpaper`
- 历史前进
- 历史后退

并同步修复目标空间 `current_index` 未更新导致的 GUI 高亮滞后问题。

### 2. 对外引擎命名统一为 awww

**修复**：
- daemon 状态输出、事件映射与文档中的静态壁纸引擎名称统一改为 `awww`
- 继续兼容旧的 `swww` 可执行文件和内部配置键名 `swww_args`

### 3. 配置和算法讨论文档补充

新增算法分析文档，记录：

- 问题起因
- 数学验证过程
- 现有算法的行为结论
- 新算法的定义与实现边界

---

## 影响的文件

- `crates/lianwall-core/src/algorithm/*`
- `crates/lianwall-core/src/config/*`
- `crates/lianwall-core/src/wallpaper/*`
- `crates/lianwall-daemon/src/handler/command.rs`
- `crates/lianwall-daemon/src/handler/query.rs`
- `crates/lianwall-cli/src/handlers/config.rs`
- `ALGORITHM.md`

---

## 升级说明

- 旧配置文件可以直接沿用，无需重建
- 若要显式调参，可手动补充 `[algorithm]` 段或使用 `lianwall config set`
- 建议与 `lianwall-gui 1.4.3` 配套升级