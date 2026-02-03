//! Algorithm 模块：壁纸权重选择与更新的统一入口。
//!
//! ## 公共接口（函数签名）
//! - initialize(input: AlgorithmInitInput) -> AlgorithmInitOutput
//! - select(input: AlgorithmSelectInput) -> Result<AlgorithmSelectOutput, AlgorithmError>
//! - update_weights(input: AlgorithmUpdateInput) -> Result<AlgorithmUpdateOutput, AlgorithmError>
//! - get_stats(records: &[WeightRecord]) -> AlgorithmStatsOutput
//!
//! ## 输入/输出结构体
//! - AlgorithmInitInput / AlgorithmInitOutput
//! - AlgorithmSelectInput / AlgorithmSelectOutput
//! - AlgorithmUpdateInput / AlgorithmUpdateOutput
//! - AlgorithmStatsOutput
//! - WeightRecord / WeightUpdateConfig
//!
//! ## 核心特性
//! - **差异化初始权重**：基于文件修改时间 + 随机扰动（±30%），避免同批文件权值相同
//! - **零和博弈权重系统**：选中惩罚均分给其他壁纸，总权重守恒
//! - **动态扰动选择**：扰动幅度与权重成比例，打破权重僵化
//! - **自动归一化**：平均权重超过阈值时按比例缩放
//! - **周期性洗牌**：定期重置部分壁纸权重，引入随机性
//!
//! ## 设计原则
//! - **无状态函数**：所有状态由外部管理（Manager/Runtime）
//! - **零和博弈**：选中惩罚 = 其他奖励总和，避免通货膨胀
//! - **结果透明**：返回归一化/洗牌等操作的触发信息

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
