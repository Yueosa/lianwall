use thiserror::Error;

#[derive(Debug, Error)]
pub enum AlgorithmError {
    #[error("壁纸列表为空")]
    EmptyList,

    #[error("无效的索引: {index}, 列表长度: {length}")]
    InvalidIndex { index: usize, length: usize },

    #[error("无效的配置: {field} = {value}, 原因: {reason}")]
    InvalidConfig {
        field: String,
        value: String,
        reason: String,
    },

    #[error("权重计算错误: {reason}")]
    WeightCalculation { reason: String },
}
