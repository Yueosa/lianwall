//!

/// 选择结果
#[derive(Debug, Clone)]
pub struct SelectOutput {
    /// 选中的壁纸索引
    pub index: usize,
    /// 新的指针位置
    pub new_pointer: f64,
}
