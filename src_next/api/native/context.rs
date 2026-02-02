use std::sync::{Mutex, OnceLock};

use crate::api::native::error::ApiError;
use crate::core::manager::CoreManager;

static API_CONTEXT: OnceLock<Mutex<ApiContext>> = OnceLock::new();

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
    API_CONTEXT
        .set(Mutex::new(ApiContext::new()?))
        .map_err(|_| ApiError::NotInitialized)?;
    Ok(())
}

/// 访问 API 上下文
pub fn with_context<F, R>(f: F) -> Result<R, ApiError>
where
    F: FnOnce(&mut ApiContext) -> Result<R, ApiError>,
{
    let context = API_CONTEXT.get().ok_or(ApiError::NotInitialized)?;
    let mut ctx = context.lock().unwrap();
    f(&mut ctx)
}
