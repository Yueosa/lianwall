//! FFI 层（C ABI 接口）
//!
//! 为 GUI 程序（如 Flutter）提供 C 兼容的 FFI 接口。
//!
//! ## 设计原则
//! - 所有函数返回 JSON 字符串（`*mut c_char`）
//! - 错误也以 JSON 格式返回：`{"error": "错误信息"}`
//! - 调用方必须使用 `lw_free_string()` 释放返回的字符串
//! - mode 参数使用整数：0 = Video, 1 = Image, -1 = 当前模式
//!
//! ## FFI 接口列表
//!
//! ### 初始化
//! - `lw_init() -> i32` (0=成功, 非0=失败)
//!
//! ### 生命周期管理
//! - `lw_start() -> *mut c_char`
//! - `lw_stop() -> *mut c_char`
//! - `lw_next() -> *mut c_char`
//! - `lw_switch_mode() -> *mut c_char`
//! - `lw_reload() -> *mut c_char`
//! - `lw_status() -> *mut c_char`
//!
//! ### 壁纸管理
//! - `lw_list(mode: i32) -> *mut c_char`
//! - `lw_list_time_ranges(mode: i32) -> *mut c_char`
//! - `lw_lock(mode: i32, path: *const c_char) -> *mut c_char`
//! - `lw_unlock(mode: i32, path: *const c_char) -> *mut c_char`
//! - `lw_stats(mode: i32) -> *mut c_char`
//!
//! ### 配置操作
//! - `lw_config_show() -> *mut c_char`
//! - `lw_config_get(key: *const c_char) -> *mut c_char`
//! - `lw_config_set(key: *const c_char, value: *const c_char) -> *mut c_char`
//! - `lw_config_reset() -> *mut c_char`
//!
//! ### 系统操作
//! - `lw_diagnose() -> *mut c_char`
//!
//! ### 内存管理
//! - `lw_free_string(ptr: *mut c_char)`

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::PathBuf;

use crate::api::native;
use crate::core::runtime::RunMode;

// ============================================================================
// 辅助函数
// ============================================================================

/// 将 Result 转换为 JSON 字符串指针
fn result_to_json<T: serde::Serialize, E: std::fmt::Display>(result: Result<T, E>) -> *mut c_char {
    let json = match result {
        Ok(data) => serde_json::to_string(&data).unwrap_or_else(|e| {
            format!(r#"{{"error":"JSON 序列化失败: {}"}}"#, e)
        }),
        Err(e) => format!(r#"{{"error":"{}"}}"#, e.to_string().replace('"', "\\\"").replace('\n', "\\n")),
    };

    CString::new(json)
        .unwrap_or_else(|_| CString::new(r#"{"error":"CString 转换失败"}"#).unwrap())
        .into_raw()
}

/// 将 mode 整数转换为 Option<RunMode>
/// - 0 = Video
/// - 1 = Image
/// - 其他（如 -1）= None（使用当前模式）
fn mode_from_int(mode: i32) -> Option<RunMode> {
    match mode {
        0 => Some(RunMode::Video),
        1 => Some(RunMode::Image),
        _ => None,
    }
}

/// 从 C 字符串指针读取 Rust String
/// 
/// # Safety
/// 调用方必须确保 ptr 是有效的以 null 结尾的 C 字符串
unsafe fn c_str_to_string(ptr: *const c_char) -> Result<String, &'static str> {
    if ptr.is_null() {
        return Err("空指针");
    }
    unsafe {
        CStr::from_ptr(ptr)
            .to_str()
            .map(|s| s.to_string())
            .map_err(|_| "无效的 UTF-8 字符串")
    }
}

// ============================================================================
// 内存管理
// ============================================================================

/// 释放由 FFI 函数返回的字符串内存
///
/// # Safety
/// 只能用于释放本 FFI 模块返回的字符串指针
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lw_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe { drop(CString::from_raw(ptr)); }
    }
}

// ============================================================================
// 初始化
// ============================================================================

/// 初始化 LianWall 上下文
///
/// 必须在调用其他 FFI 函数之前调用。
///
/// # 返回值
/// - 0: 成功
/// - 非0: 失败
#[unsafe(no_mangle)]
pub extern "C" fn lw_init() -> i32 {
    match native::init() {
        Ok(_) => 0,
        Err(_) => 1,
    }
}

// ============================================================================
// 生命周期管理
// ============================================================================

/// 启动守护进程
///
/// 注意：此函数会阻塞！在 GUI 中应在单独线程中调用。
#[unsafe(no_mangle)]
pub extern "C" fn lw_start() -> *mut c_char {
    let result = native::start(false);
    result_to_json(result)
}

/// 停止守护进程
#[unsafe(no_mangle)]
pub extern "C" fn lw_stop() -> *mut c_char {
    let result = native::stop(false);
    result_to_json(result)
}

/// 切换下一张壁纸
#[unsafe(no_mangle)]
pub extern "C" fn lw_next() -> *mut c_char {
    let result = native::next(false);
    result_to_json(result)
}

/// 切换模式（Video ↔ Image）
#[unsafe(no_mangle)]
pub extern "C" fn lw_switch_mode() -> *mut c_char {
    let result = native::switch_mode(false);
    result_to_json(result)
}

/// 热重载壁纸目录
#[unsafe(no_mangle)]
pub extern "C" fn lw_reload() -> *mut c_char {
    let result = native::reload(None, false);
    result_to_json(result)
}

/// 获取当前状态
#[unsafe(no_mangle)]
pub extern "C" fn lw_status() -> *mut c_char {
    let result = native::status(false);
    result_to_json(result)
}

// ============================================================================
// 壁纸管理
// ============================================================================

/// 列出壁纸
///
/// # 参数
/// - mode: 0=Video, 1=Image, -1=当前模式
#[unsafe(no_mangle)]
pub extern "C" fn lw_list(mode: i32) -> *mut c_char {
    let result = native::list(mode_from_int(mode), false);
    result_to_json(result)
}

/// 列出时间段目录
///
/// # 参数
/// - mode: 0=Video, 1=Image, -1=当前模式
#[unsafe(no_mangle)]
pub extern "C" fn lw_list_time_ranges(mode: i32) -> *mut c_char {
    let result = native::list_time_ranges(mode_from_int(mode), false);
    result_to_json(result)
}

/// 锁定壁纸
///
/// # 参数
/// - mode: 0=Video, 1=Image
/// - path: 壁纸文件的绝对路径（C 字符串）
///
/// # Safety
/// path 必须是有效的以 null 结尾的 UTF-8 字符串
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lw_lock(mode: i32, path: *const c_char) -> *mut c_char {
    let run_mode = match mode_from_int(mode) {
        Some(m) => m,
        None => return result_to_json::<(), _>(Err("mode 必须是 0 (Video) 或 1 (Image)")),
    };

    let path_str = match unsafe { c_str_to_string(path) } {
        Ok(s) => s,
        Err(e) => return result_to_json::<(), _>(Err(e)),
    };

    let result = native::lock(run_mode, PathBuf::from(path_str), false);
    result_to_json(result)
}

/// 解锁壁纸
///
/// # 参数
/// - mode: 0=Video, 1=Image
/// - path: 壁纸文件的绝对路径（C 字符串）
///
/// # Safety
/// path 必须是有效的以 null 结尾的 UTF-8 字符串
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lw_unlock(mode: i32, path: *const c_char) -> *mut c_char {
    let run_mode = match mode_from_int(mode) {
        Some(m) => m,
        None => return result_to_json::<(), _>(Err("mode 必须是 0 (Video) 或 1 (Image)")),
    };

    let path_str = match unsafe { c_str_to_string(path) } {
        Ok(s) => s,
        Err(e) => return result_to_json::<(), _>(Err(e)),
    };

    let result = native::unlock(run_mode, PathBuf::from(path_str), false);
    result_to_json(result)
}

/// 获取统计信息
///
/// # 参数
/// - mode: 0=Video, 1=Image, -1=当前模式
#[unsafe(no_mangle)]
pub extern "C" fn lw_stats(mode: i32) -> *mut c_char {
    let result = native::stats(mode_from_int(mode), false);
    result_to_json(result)
}

// ============================================================================
// 配置操作
// ============================================================================

/// 显示完整配置
#[unsafe(no_mangle)]
pub extern "C" fn lw_config_show() -> *mut c_char {
    let result = native::config_show(false);
    result_to_json(result)
}

/// 获取配置项
///
/// # 参数
/// - key: 配置键（如 "weight.base"）
///
/// # Safety
/// key 必须是有效的以 null 结尾的 UTF-8 字符串
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lw_config_get(key: *const c_char) -> *mut c_char {
    let key_str = match unsafe { c_str_to_string(key) } {
        Ok(s) => s,
        Err(e) => return result_to_json::<(), _>(Err(e)),
    };

    let result = native::config_get(&key_str, false);
    result_to_json(result)
}

/// 设置配置项
///
/// # 参数
/// - key: 配置键（如 "weight.base"）
/// - value: 配置值
///
/// # Safety
/// key 和 value 必须是有效的以 null 结尾的 UTF-8 字符串
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lw_config_set(key: *const c_char, value: *const c_char) -> *mut c_char {
    let key_str = match unsafe { c_str_to_string(key) } {
        Ok(s) => s,
        Err(e) => return result_to_json::<(), _>(Err(e)),
    };

    let value_str = match unsafe { c_str_to_string(value) } {
        Ok(s) => s,
        Err(e) => return result_to_json::<(), _>(Err(e)),
    };

    let result = native::config_set(&key_str, &value_str, false);
    result_to_json(result)
}

/// 重置配置为默认值
#[unsafe(no_mangle)]
pub extern "C" fn lw_config_reset() -> *mut c_char {
    let result = native::config_reset(false);
    result_to_json(result)
}

// ============================================================================
// 系统操作
// ============================================================================

/// 系统诊断
#[unsafe(no_mangle)]
pub extern "C" fn lw_diagnose() -> *mut c_char {
    let result = native::diagnose(false);
    result_to_json(result)
}
