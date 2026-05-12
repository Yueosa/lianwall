//! # Algorithm 模块
//!
//! 基于向量空间的壁纸选择算法。
//!
//! ## 核心概念
//! - **黄金角遍历**: 指针每次旋转 137.508°，数学保证均匀遍历
//! - **动态冷却**: 根据壁纸数量自动计算冷却窗口，防止短期重复
//! - **顺时针偏置概率选择**: 右侧近点概率更高，左侧候选带固定惩罚
//!
//! ## 导出接口
//! - 选择: `select_next`, `select_next_with_config`
//! - 常量: `GOLDEN_ANGLE`
//! - 工具: `calc_cooldown`, `angular_distance`
//!
//! ## 历史管理
//! 播放历史已移至 daemon 层的 PlaybackHistory（浏览器式前进/后退模型）

mod golden;
mod selector;
mod r#struct;

pub use golden::{angular_distance, calc_cooldown, GOLDEN_ANGLE};
pub use selector::{select_next, select_next_with_config};
pub use r#struct::{SelectOutput, SelectionConfig};
