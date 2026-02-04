//! # Algorithm 模块
//!
//! 基于向量空间的壁纸选择算法。
//!
//! ## 核心概念
//! - **黄金角遍历**: 指针每次旋转 137.508°，数学保证均匀遍历
//! - **动态冷却**: 根据壁纸数量自动计算冷却窗口，防止短期重复
//! - **最近邻选择**: 选择距离指针最近的可用壁纸
//!
//! ## 导出接口
//! - 选择: `select_next`, `select_previous`
//! - 常量: `GOLDEN_ANGLE`
//! - 工具: `calc_cooldown`, `angular_distance`

mod golden;
mod selector;
mod r#struct;

pub use golden::{angular_distance, calc_cooldown, GOLDEN_ANGLE};
pub use selector::{select_next, select_previous};
pub use r#struct::SelectOutput;
