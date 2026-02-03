use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugTrace {
    /// 模块路径（如 "api::next"）
    pub module: String,
    /// 输入参数（JSON）
    pub input: serde_json::Value,
    /// 输出结果（JSON）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<serde_json::Value>,
    /// 错误信息
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// 执行时间（毫秒）
    pub duration_ms: u64,
    /// 子调用栈
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<DebugTrace>,
}

thread_local! {
    static DEBUG_CONTEXT: RefCell<DebugContext> = RefCell::new(DebugContext::new());
}

struct DebugContext {
    enabled: bool,
    stack: Vec<DebugTraceBuilder>,
}

struct DebugTraceBuilder {
    module: String,
    input: serde_json::Value,
    start_time: Instant,
    children: Vec<DebugTrace>,
}

impl DebugContext {
    fn new() -> Self {
        Self {
            enabled: false,
            stack: Vec::new(),
        }
    }
}

/// 启用 Debug 模式
pub fn enable_debug() {
    DEBUG_CONTEXT.with(|ctx| {
        ctx.borrow_mut().enabled = true;
    });
}

/// 禁用 Debug 模式
pub fn disable_debug() {
    DEBUG_CONTEXT.with(|ctx| {
        let mut ctx = ctx.borrow_mut();
        ctx.enabled = false;
        ctx.stack.clear();
    });
}

/// 检查 Debug 模式是否启用
pub fn is_debug_enabled() -> bool {
    DEBUG_CONTEXT.with(|ctx| ctx.borrow().enabled)
}

/// 进入函数追踪
pub fn trace_enter(module: &str, input: serde_json::Value) {
    DEBUG_CONTEXT.with(|ctx| {
        let mut ctx = ctx.borrow_mut();
        if ctx.enabled {
            ctx.stack.push(DebugTraceBuilder {
                module: module.to_string(),
                input,
                start_time: Instant::now(),
                children: Vec::new(),
            });
        }
    });
}

/// 退出函数追踪（成功）
pub fn trace_exit(output: serde_json::Value) {
    DEBUG_CONTEXT.with(|ctx| {
        let mut ctx = ctx.borrow_mut();
        if ctx.enabled && !ctx.stack.is_empty() {
            let builder = ctx.stack.pop().unwrap();
            let duration_ms = builder.start_time.elapsed().as_millis() as u64;

            let trace = DebugTrace {
                module: builder.module,
                input: builder.input,
                output: Some(output),
                error: None,
                duration_ms,
                children: builder.children,
            };

            // 添加到父级或保存为根
            if let Some(parent) = ctx.stack.last_mut() {
                parent.children.push(trace);
            }
        }
    });
}

/// 退出函数追踪（失败）
pub fn trace_exit_error(error: &str) {
    DEBUG_CONTEXT.with(|ctx| {
        let mut ctx = ctx.borrow_mut();
        if ctx.enabled && !ctx.stack.is_empty() {
            let builder = ctx.stack.pop().unwrap();
            let duration_ms = builder.start_time.elapsed().as_millis() as u64;

            let trace = DebugTrace {
                module: builder.module,
                input: builder.input,
                output: None,
                error: Some(error.to_string()),
                duration_ms,
                children: builder.children,
            };

            if let Some(parent) = ctx.stack.last_mut() {
                parent.children.push(trace);
            }
        }
    });
}

/// 获取完整的 Debug 追踪
pub fn get_trace() -> Vec<DebugTrace> {
    DEBUG_CONTEXT.with(|ctx| {
        let ctx = ctx.borrow();
        if ctx.enabled && !ctx.stack.is_empty() {
            // 将剩余的栈转换为追踪记录
            ctx.stack
                .iter()
                .map(|builder| DebugTrace {
                    module: builder.module.clone(),
                    input: builder.input.clone(),
                    output: None,
                    error: Some("未完成".to_string()),
                    duration_ms: builder.start_time.elapsed().as_millis() as u64,
                    children: builder.children.clone(),
                })
                .collect()
        } else {
            Vec::new()
        }
    })
}

/// 清空追踪记录
pub fn clear_trace() {
    DEBUG_CONTEXT.with(|ctx| {
        ctx.borrow_mut().stack.clear();
    });
}

/// Debug 守卫（RAII）
pub struct DebugGuard {
    module: String,
}

impl DebugGuard {
    pub fn new(module: &str, input: serde_json::Value) -> Self {
        trace_enter(module, input);
        Self {
            module: module.to_string(),
        }
    }

    pub fn success(self, output: serde_json::Value) {
        trace_exit(output);
        std::mem::forget(self); // 防止 Drop
    }

    pub fn error(self, error: &str) {
        trace_exit_error(error);
        std::mem::forget(self);
    }
}

impl Drop for DebugGuard {
    fn drop(&mut self) {
        // 异常退出（panic 等）
        trace_exit_error("异常退出");
    }
}
