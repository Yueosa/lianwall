use serde_json::json;
use std::fs;
use std::path::PathBuf;

use crate::api::native::debug::{self, DebugGuard};
use crate::api::native::error::ApiError;
use crate::api::native::r#struct::*;
use crate::core::config::{create, delete, read, update, ConfigCreateInput, ConfigDeleteInput, ConfigReadInput, ConfigUpdateInput};

/// 获取配置项
pub fn config_get(key: &str, debug: bool) -> Result<ApiResponse<ApiConfigGetOutput>, ApiError> {
    if debug {
        debug::enable_debug();
    }

    let guard = DebugGuard::new("api::config_get", json!({"key": key}));

    let result = (|| {
        let output = read(ConfigReadInput { path: None }).map_err(|e| ApiError::track(e.into(), "config_get"))?;
        let config = output.config;

        let value = match key {
            "paths.mode" => config.paths.mode,
            "paths.video_dir" => config.paths.video_dir.display().to_string(),
            "paths.image_dir" => config.paths.image_dir.display().to_string(),
            "video_engine.interval" => config.video_engine.interval.to_string(),
            "image_engine.interval" => config.image_engine.interval.to_string(),
            "weight.weight_min" => config.weight.weight_min.to_string(),
            "weight.weight_max" => config.weight.weight_max.to_string(),
            "weight.select_penalty" => config.weight.select_penalty.to_string(),
            "weight.top_n_percent" => config.weight.top_n_percent.to_string(),
            "weight.hash_mix_bytes" => config.weight.hash_mix_bytes.to_string(),
            "weight.seed_reset_hours" => config.weight.seed_reset_hours.to_string(),
            "weight.normalization_threshold" => config.weight.normalization_threshold.to_string(),
            "weight.normalization_target" => config.weight.normalization_target.to_string(),
            "weight.shuffle_period" => config.weight.shuffle_period.to_string(),
            "weight.shuffle_intensity" => config.weight.shuffle_intensity.to_string(),
            "vram.enabled" => config.vram.enabled.to_string(),
            "vram.threshold_percent" => config.vram.threshold_percent.to_string(),
            "vram.recovery_percent" => config.vram.recovery_percent.to_string(),
            _ => return Err(ApiError::InvalidConfigKey(key.to_string())),
        };

        Ok(ApiConfigGetOutput {
            key: key.to_string(),
            value,
        })
    })();

    match result {
        Ok(output) => {
            guard.success(json!({"value": &output.value}));
            let debug_info = if debug {
                Some(ApiDebugInfo {
                    total_duration_ms: debug::get_trace().first().map(|t| t.duration_ms).unwrap_or(0),
                    trace: debug::get_trace(),
                })
            } else {
                None
            };
            debug::disable_debug();
            Ok(ApiResponse::success(output, debug_info))
        }
        Err(e) => {
            guard.error(&e.to_string());
            debug::disable_debug();
            Err(e)
        }
    }
}

/// 设置配置项
pub fn config_set(
    key: &str,
    value: &str,
    debug: bool,
) -> Result<ApiResponse<ApiConfigSetOutput>, ApiError> {
    if debug {
        debug::enable_debug();
    }

    let guard = DebugGuard::new("api::config_set", json!({"key": key, "value": value}));

    let result = (|| {
        let output = read(ConfigReadInput { path: None }).map_err(|e| ApiError::track(e.into(), "config_set"))?;
        let mut config = output.config;

        // 记录旧值
        let old_value = match key {
            "paths.mode" => config.paths.mode.clone(),
            "paths.video_dir" => config.paths.video_dir.display().to_string(),
            "paths.image_dir" => config.paths.image_dir.display().to_string(),
            "video_engine.interval" => config.video_engine.interval.to_string(),
            "image_engine.interval" => config.image_engine.interval.to_string(),
            "weight.weight_min" => config.weight.weight_min.to_string(),
            "weight.weight_max" => config.weight.weight_max.to_string(),
            "weight.select_penalty" => config.weight.select_penalty.to_string(),
            "weight.top_n_percent" => config.weight.top_n_percent.to_string(),
            "weight.hash_mix_bytes" => config.weight.hash_mix_bytes.to_string(),
            "weight.seed_reset_hours" => config.weight.seed_reset_hours.to_string(),
            "weight.normalization_threshold" => config.weight.normalization_threshold.to_string(),
            "weight.normalization_target" => config.weight.normalization_target.to_string(),
            "weight.shuffle_period" => config.weight.shuffle_period.to_string(),
            "weight.shuffle_intensity" => config.weight.shuffle_intensity.to_string(),
            "vram.enabled" => config.vram.enabled.to_string(),
            "vram.threshold_percent" => config.vram.threshold_percent.to_string(),
            "vram.recovery_percent" => config.vram.recovery_percent.to_string(),
            _ => return Err(ApiError::InvalidConfigKey(key.to_string())),
        };

        // 设置新值
        match key {
            "paths.mode" => config.paths.mode = value.to_string(),
            "paths.video_dir" => config.paths.video_dir = PathBuf::from(value),
            "paths.image_dir" => config.paths.image_dir = PathBuf::from(value),
            "video_engine.interval" => {
                config.video_engine.interval = value.parse().map_err(|_| {
                    ApiError::InvalidConfigValue {
                        key: key.to_string(),
                        value: value.to_string(),
                        reason: "必须是正整数".to_string(),
                    }
                })?
            }
            "image_engine.interval" => {
                config.image_engine.interval = value.parse().map_err(|_| {
                    ApiError::InvalidConfigValue {
                        key: key.to_string(),
                        value: value.to_string(),
                        reason: "必须是正整数".to_string(),
                    }
                })?
            }
            "weight.weight_min" => {
                config.weight.weight_min = value.parse().map_err(|_| ApiError::InvalidConfigValue {
                    key: key.to_string(),
                    value: value.to_string(),
                    reason: "必须是浮点数".to_string(),
                })?
            }
            "weight.weight_max" => {
                config.weight.weight_max = value.parse().map_err(|_| ApiError::InvalidConfigValue {
                    key: key.to_string(),
                    value: value.to_string(),
                    reason: "必须是浮点数".to_string(),
                })?
            }
            "weight.select_penalty" => {
                config.weight.select_penalty = value.parse().map_err(|_| ApiError::InvalidConfigValue {
                    key: key.to_string(),
                    value: value.to_string(),
                    reason: "必须是浮点数".to_string(),
                })?
            }
            "weight.top_n_percent" => {
                config.weight.top_n_percent = value.parse().map_err(|_| ApiError::InvalidConfigValue {
                    key: key.to_string(),
                    value: value.to_string(),
                    reason: "必须是 0.0-1.0 的浮点数".to_string(),
                })?
            }
            "weight.hash_mix_bytes" => {
                config.weight.hash_mix_bytes = value.parse().map_err(|_| ApiError::InvalidConfigValue {
                    key: key.to_string(),
                    value: value.to_string(),
                    reason: "必须是 0-8 的整数".to_string(),
                })?
            }
            "weight.seed_reset_hours" => {
                config.weight.seed_reset_hours = value.parse().map_err(|_| ApiError::InvalidConfigValue {
                    key: key.to_string(),
                    value: value.to_string(),
                    reason: "必须是非负整数".to_string(),
                })?
            }
            "weight.normalization_threshold" => {
                config.weight.normalization_threshold = value.parse().map_err(|_| ApiError::InvalidConfigValue {
                    key: key.to_string(),
                    value: value.to_string(),
                    reason: "必须是浮点数".to_string(),
                })?
            }
            "weight.normalization_target" => {
                config.weight.normalization_target = value.parse().map_err(|_| ApiError::InvalidConfigValue {
                    key: key.to_string(),
                    value: value.to_string(),
                    reason: "必须是浮点数".to_string(),
                })?
            }
            "weight.shuffle_period" => {
                config.weight.shuffle_period = value.parse().map_err(|_| ApiError::InvalidConfigValue {
                    key: key.to_string(),
                    value: value.to_string(),
                    reason: "必须是非负整数".to_string(),
                })?
            }
            "weight.shuffle_intensity" => {
                config.weight.shuffle_intensity = value.parse().map_err(|_| ApiError::InvalidConfigValue {
                    key: key.to_string(),
                    value: value.to_string(),
                    reason: "必须是 0.0-1.0 的浮点数".to_string(),
                })?
            }
            "vram.enabled" => {
                config.vram.enabled = value.parse().map_err(|_| ApiError::InvalidConfigValue {
                    key: key.to_string(),
                    value: value.to_string(),
                    reason: "必须是 true 或 false".to_string(),
                })?
            }
            "vram.threshold_percent" => {
                config.vram.threshold_percent = value.parse().map_err(|_| ApiError::InvalidConfigValue {
                    key: key.to_string(),
                    value: value.to_string(),
                    reason: "必须是浮点数".to_string(),
                })?
            }
            "vram.recovery_percent" => {
                config.vram.recovery_percent = value.parse().map_err(|_| ApiError::InvalidConfigValue {
                    key: key.to_string(),
                    value: value.to_string(),
                    reason: "必须是浮点数".to_string(),
                })?
            }
            _ => return Err(ApiError::InvalidConfigKey(key.to_string())),
        }

        // 保存配置
        update(ConfigUpdateInput {
            path: None,
            config,
        })
        .map_err(|e| ApiError::track(e.into(), "config_set"))?;

        Ok(ApiConfigSetOutput {
            key: key.to_string(),
            old_value,
            new_value: value.to_string(),
        })
    })();

    match result {
        Ok(output) => {
            guard.success(json!({"new_value": &output.new_value}));
            let debug_info = if debug {
                Some(ApiDebugInfo {
                    total_duration_ms: debug::get_trace().first().map(|t| t.duration_ms).unwrap_or(0),
                    trace: debug::get_trace(),
                })
            } else {
                None
            };
            debug::disable_debug();
            Ok(ApiResponse::success(output, debug_info))
        }
        Err(e) => {
            guard.error(&e.to_string());
            debug::disable_debug();
            Err(e)
        }
    }
}

/// 显示完整配置
pub fn config_show(debug: bool) -> Result<ApiResponse<ApiConfigShowOutput>, ApiError> {
    if debug {
        debug::enable_debug();
    }

    let guard = DebugGuard::new("api::config_show", json!({}));

    let result: Result<ApiConfigShowOutput, ApiError> = (|| {
        let config_path = dirs::config_dir()
            .unwrap_or_default()
            .join("lianwall/config.toml");

        let config_toml = fs::read_to_string(&config_path).map_err(|e| ApiError::Io(e))?;

        Ok(ApiConfigShowOutput { config_toml })
    })();

    match result {
        Ok(output) => {
            guard.success(json!({"length": output.config_toml.len()}));
            let debug_info = if debug {
                Some(ApiDebugInfo {
                    total_duration_ms: debug::get_trace().first().map(|t| t.duration_ms).unwrap_or(0),
                    trace: debug::get_trace(),
                })
            } else {
                None
            };
            debug::disable_debug();
            Ok(ApiResponse::success(output, debug_info))
        }
        Err(e) => {
            guard.error(&e.to_string());
            debug::disable_debug();
            Err(e)
        }
    }
}

/// 重置配置为默认值
pub fn config_reset(debug: bool) -> Result<ApiResponse<ApiConfigResetOutput>, ApiError> {
    if debug {
        debug::enable_debug();
    }

    let guard = DebugGuard::new("api::config_reset", json!({}));

    let result: Result<ApiConfigResetOutput, ApiError> = (|| {
        // 删除旧配置
        delete(ConfigDeleteInput { path: None }).map_err(|e| ApiError::track(e.into(), "config_reset"))?;

        // 创建新配置
        create(ConfigCreateInput { path: None }).map_err(|e| ApiError::track(e.into(), "config_reset"))?;

        Ok(ApiConfigResetOutput {
            message: "配置已重置为默认值".to_string(),
        })
    })();

    match result {
        Ok(output) => {
            guard.success(json!({"reset": true}));
            let debug_info = if debug {
                Some(ApiDebugInfo {
                    total_duration_ms: debug::get_trace().first().map(|t| t.duration_ms).unwrap_or(0),
                    trace: debug::get_trace(),
                })
            } else {
                None
            };
            debug::disable_debug();
            Ok(ApiResponse::success(output, debug_info))
        }
        Err(e) => {
            guard.error(&e.to_string());
            debug::disable_debug();
            Err(e)
        }
    }
}
