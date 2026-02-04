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
//! - **内容哈希初始化**：基于文件内容前 64KB 计算哈希，均匀映射到 [weight_min, weight_max] 区间
//! - **零和博弈权重系统**：选中惩罚均分给其他壁纸，总权重守恒
//! - **Top-N + 哈希亲和度选择**：从权重最高的 N% 壁纸中，选择与系统种子最亲和的
//! - **混合哈希算法**：前 x 字节与种子异或（随机性），后 8-x 字节保持原哈希（确定性）
//! - **自动归一化**：平均权重超过阈值时按比例缩放
//! - **周期性洗牌**：定期重置部分壁纸权重，引入随机性
//! - **精度控制**：所有权重精确到 2 位小数，约束在 [weight_min, weight_max] 区间
//!
//! ## 设计原则
//! - **无状态函数**：所有状态由外部管理（Manager/Runtime）
//! - **零和博弈**：选中惩罚 = 其他奖励总和，避免通货膨胀
//! - **确定性 + 随机性平衡**：哈希亲和度提供可预测性，混合字节数控制随机程度
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
