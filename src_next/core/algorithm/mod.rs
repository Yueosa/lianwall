//! 壁纸选择算法模块
//!
//! ## 核心特性
//! - **零和博弈权重系统**：选中者惩罚均分给其他壁纸，总权重守恒
//! - **动态扰动选择**：扰动幅度与权重成比例，打破权重僵化
//! - **自动归一化**：平均权重超过阈值时按比例缩放
//! - **周期性洗牌**：定期重置部分壁纸权重，引入随机性
//!
//! ## 设计原则
//! - **无状态函数**：所有状态由外部管理（Runtime 模块）
//! - **零和博弈**：选中惩罚 = 其他奖励总和，避免通货膨胀
//! - **结果透明**：返回归一化/洗牌等操作的触发信息
//!
//! ## 使用示例
//! ```rust
//! use crate::core::algorithm::{select, update_weights};
//! use crate::core::algorithm::{AlgorithmSelectInput, AlgorithmUpdateInput, WeightUpdateConfig};
//!
//! // 1. 选择壁纸
//! let select_result = select(AlgorithmSelectInput {
//!     records: weight_records,
//!     tolerance: 10.0,
//!     perturbation_ratio: 0.03,
//! }).unwrap();
//!
//! println!("选中: {:?}", select_result.selected_path);
//!
//! // 2. 更新权重
//! let update_result = update_weights(AlgorithmUpdateInput {
//!     records: weight_records,
//!     selected_index: select_result.selected_index,
//!     config: WeightUpdateConfig {
//!         select_penalty: 20.0,
//!         normalization_threshold: 500.0,
//!         normalization_target: 100.0,
//!         shuffle_period: 100,
//!         shuffle_intensity: 0.1,
//!         base_weight: 100.0,
//!     },
//!     selection_count: current_count,
//! }).unwrap();
//!
//! if update_result.normalized {
//!     println!("触发归一化");
//! }
//! if update_result.shuffled {
//!     println!("触发洗牌，重置 {} 张壁纸", update_result.shuffle_count.unwrap());
//! }
//! ```

mod error;
mod initializer;
mod selector;
mod stats;
mod r#struct;
mod weight;

// 导出核心函数
pub use initializer::initialize;
pub use selector::select;
pub use stats::get_stats;
pub use weight::update_weights;

// 导出错误类型
pub use error::AlgorithmError;

// 导出结构体
pub use r#struct::{
    AlgorithmInitInput, AlgorithmInitOutput, AlgorithmSelectInput, AlgorithmSelectOutput,
    AlgorithmStatsOutput, AlgorithmUpdateInput, AlgorithmUpdateOutput, WeightRecord,
    WeightUpdateConfig,
};
