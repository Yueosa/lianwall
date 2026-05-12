//! 选择器返回值与参数

/// 选择策略参数
#[derive(Debug, Clone, Copy)]
pub struct SelectionConfig {
    /// 左侧候选的固定惩罚项，单位为“当前平均角间距”
    pub bias_lambda: f64,
    /// softmax 温度；越小越接近确定性最优
    pub temperature: f64,
}

impl Default for SelectionConfig {
    fn default() -> Self {
        Self {
            bias_lambda: 0.35,
            temperature: 0.28,
        }
    }
}

/// 选择结果
#[derive(Debug, Clone)]
pub struct SelectOutput {
    /// 选中的壁纸索引
    pub index: usize,
    /// 新的指针位置
    pub new_pointer: f64,
}
