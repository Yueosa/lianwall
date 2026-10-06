# Changelog - 5.5.2

> 发布日期：2026-10-07

## 概述

5.5.2 是性能修复版本，落地 `PLAN-performance-fixes.md`：壁纸数量较大时的 rescan 复杂度、
高频状态查询的内存开销，以及选择算法热路径上的一次多余分配。

无协议、配置或行为变更，可直接替换二进制升级。

---

## 性能修复

### 1. rescan 恢复锁定状态从 O(n²) 降为 O(n)

**问题**：
- `rebuild_space` 恢复锁定状态与播放历史时，对每张新壁纸线性扫描整个持久化列表
- 壁纸数量达到几千张时，一次 rescan 触发百万级路径比较

**修复**：
- 先按路径建立 HashMap 再查表，复杂度降为 O(n)
- 行为完全不变，锁定状态、`last_played`、当前壁纸索引的恢复逻辑保持一致

**影响文件**：`crates/lianwall-core/src/wallpaper/space.rs`

### 2. GetStatus 查询不再深拷贝整个壁纸空间

**问题**：
- `GetStatus` 每次查询都完整克隆视频和图片两个壁纸空间（含全部路径字符串）
- 实际只用到当前模式空间的三个计数（总数 / 锁定数 / 冷却数）
- GUI 高频轮询状态时是持续的内存与 CPU 浪费

**修复**：
- `SharedState` 新增 `get_space_summary(mode)`，只读取计数标量，不克隆列表
- `GetStatus` 改为按当前模式取一次摘要，返回字段与数值口径不变

**影响文件**：`crates/lianwall-daemon/src/state.rs`、`crates/lianwall-daemon/src/handler/query.rs`

### 3. 选择算法热路径减少一次 Vec 分配

- `sample_biased_candidate` 原为 scores、weights 各分配一个 Vec
- 改为 scores 原地转换为 softmax 权重，每次切换少一次分配

**影响文件**：`crates/lianwall-core/src/algorithm/selector.rs`

### 4. 清理误导性注释

- 移除 `state.rs` 中已裁决不做的「锁粒度优化（dashmap）」TODO

---

## 明确不做（沿用计划裁决）

- `GetSpace` 全量快照的深拷贝：GUI 打开列表的真实需求，收益有限，暂不改
- `PlaybackHistory::trim` 的 `remove(0)`：上限 100 条，无实际影响
- dashmap 锁粒度重构：并发量低，tokio RwLock 足够

---

## 升级说明

- 配置文件、Socket 协议、CLI 行为均无变更，直接替换二进制即可
- 与 `lianwall-gui` 版本无耦合，无需同步升级
