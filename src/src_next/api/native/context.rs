use std::sync::{Mutex, OnceLock, RwLock};

use crate::api::native::error::ApiError;
use crate::core::manager::CoreManager;

static API_CONTEXT: OnceLock<RwLock<Option<ApiContext>>> = OnceLock::new();

pub struct ApiContext {
    pub manager: CoreManager,
}

impl ApiContext {
    fn new() -> Result<Self, ApiError> {
        let manager = CoreManager::new().map_err(|e| ApiError::track(e, "init"))?;

        Ok(Self { manager })
    }
}

/// 初始化 API 上下文
pub fn init() -> Result<(), ApiError> {
    let context_lock = API_CONTEXT.get_or_init(|| RwLock::new(None));
    let mut ctx = context_lock.write().unwrap();

    // 允许重新初始化
    *ctx = Some(ApiContext::new()?);
    Ok(())
}

/// 访问 API 上下文
pub fn with_context<F, R>(f: F) -> Result<R, ApiError>
where
    F: FnOnce(&mut ApiContext) -> Result<R, ApiError>,
{
    let context_lock = API_CONTEXT.get().ok_or(ApiError::NotInitialized)?;
    let mut ctx = context_lock.write().unwrap();
    let ctx_ref = ctx.as_mut().ok_or(ApiError::NotInitialized)?;
    f(ctx_ref)
}

/// 重置上下文
#[allow(dead_code)]
pub fn reset() -> Result<(), ApiError> {
    if let Some(context_lock) = API_CONTEXT.get() {
        let mut ctx = context_lock.write().unwrap();
        *ctx = None;
    }
    Ok(())
}
