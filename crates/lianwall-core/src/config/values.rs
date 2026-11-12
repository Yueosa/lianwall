//! Shared typed configuration access for daemon and offline CLI.
use serde_json::Value;

use super::{Config, ops::normalize_paths};

pub fn get_value(config: &Config, key: &str) -> Option<Value> {
    let (section, field) = key.split_once('.')?;
    serde_json::to_value(config)
        .ok()?
        .get(section)?
        .get(field)
        .cloned()
}

/// Parse CLI text according to the existing field's type, preserving numeric strings.
pub fn parse_value(config: &Config, key: &str, text: &str) -> Result<Value, String> {
    let current = get_value(config, key).ok_or_else(|| format!("Unknown config key: {key}"))?;
    let value = match current {
        Value::String(_) => {
            let text = serde_json::from_str::<String>(text).unwrap_or_else(|_| text.to_string());
            let text = match key {
                "paths.mode" if text.eq_ignore_ascii_case("video") => "Video".into(),
                "paths.mode" if text.eq_ignore_ascii_case("image") => "Image".into(),
                _ => text,
            };
            Value::String(text)
        }
        Value::Array(_) => {
            if text.trim_start().starts_with('[') {
                serde_json::from_str(text).map_err(|e| format!("Invalid array: {e}"))?
            } else {
                serde_json::json!(
                    text.split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                )
            }
        }
        Value::Bool(_) => Value::Bool(match text.to_ascii_lowercase().as_str() {
            "true" => true,
            "false" => false,
            _ => return Err("Expected true or false".into()),
        }),
        _ => serde_json::from_str(text).map_err(|e| format!("Invalid value: {e}"))?,
    };
    // Use exactly the same type and range validation as a socket SetConfig request.
    let mut candidate = config.clone();
    set_value(&mut candidate, key, value.clone())?;
    Ok(value)
}

/// Validate an edit on a candidate; failed edits never mutate the caller's config.
pub fn set_value(config: &mut Config, key: &str, value: Value) -> Result<Value, String> {
    let (section, field) = key
        .split_once('.')
        .ok_or_else(|| format!("Unknown config key: {key}"))?;
    let mut document = serde_json::to_value(&*config).map_err(|e| e.to_string())?;
    let slot = document
        .get_mut(section)
        .and_then(|s| s.get_mut(field))
        .ok_or_else(|| format!("Unknown config key: {key}"))?;
    *slot = value;
    let mut candidate: Config =
        serde_json::from_value(document).map_err(|e| format!("Invalid {key}: {e}"))?;
    match key {
        "video_engine.interval" | "image_engine.interval" => {
            let n = if section == "video_engine" {
                candidate.video_engine.interval
            } else {
                candidate.image_engine.interval
            };
            if n != 0 && !(10..=86400).contains(&n) {
                return Err("interval must be 0 (paused) or between 10 and 86400".into());
            }
        }
        "algorithm.bias_lambda" if !(0.0..=4.0).contains(&candidate.algorithm.bias_lambda) => {
            return Err("bias_lambda must be between 0.0 and 4.0".into());
        }
        "algorithm.temperature" if !(0.01..=5.0).contains(&candidate.algorithm.temperature) => {
            return Err("temperature must be between 0.01 and 5.0".into());
        }
        "vram.threshold_percent" | "vram.recovery_percent" => {
            if !(5.0..=50.0).contains(&candidate.vram.threshold_percent)
                || !(20.0..=80.0).contains(&candidate.vram.recovery_percent)
                || candidate.vram.recovery_percent <= candidate.vram.threshold_percent
            {
                return Err(
                    "VRAM threshold must be 5..50, recovery 20..80 and greater than threshold"
                        .into(),
                );
            }
        }
        "vram.check_interval" if !(1..=60).contains(&candidate.vram.check_interval) => {
            return Err("check_interval must be between 1 and 60".into());
        }
        "vram.cooldown_seconds" if !(10..=600).contains(&candidate.vram.cooldown_seconds) => {
            return Err("cooldown_seconds must be between 10 and 600".into());
        }
        "daemon.log_level"
            if !["error", "warn", "info", "debug", "trace"]
                .contains(&candidate.daemon.log_level.as_str()) =>
        {
            return Err("Invalid log_level, expected: error/warn/info/debug/trace".into());
        }
        _ => {}
    }
    normalize_paths(&mut candidate);
    let normalized = get_value(&candidate, key).expect("validated configuration field exists");
    *config = candidate;
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config() -> Config {
        let mut config = toml::from_str(super::super::DEFAULT_CONFIG_TOML).unwrap();
        normalize_paths(&mut config);
        config
    }

    #[test]
    fn typed_cli_values_round_trip_all_fields() {
        let config = config();
        let document = serde_json::to_value(&config).unwrap();
        for (section, fields) in document.as_object().unwrap() {
            for (field, expected) in fields.as_object().unwrap() {
                let key = format!("{section}.{field}");
                let parsed = parse_value(&config, &key, &expected.to_string()).unwrap();
                let mut updated = config.clone();
                let actual = set_value(&mut updated, &key, parsed).unwrap();
                assert_eq!(actual, get_value(&updated, &key).unwrap());
                assert_eq!(actual, *expected, "round trip changed {key}");
            }
        }
    }

    #[test]
    fn invalid_edits_leave_config_unchanged() {
        for (key, value) in [
            ("video_engine.interval", serde_json::json!(1)),
            ("image_engine.interval", serde_json::json!(-1)),
            ("video_engine.mpv_args", serde_json::json!([true])),
            ("vram.check_interval", serde_json::json!(0)),
            ("algorithm.temperature", serde_json::json!(0)),
            ("daemon.log_level", serde_json::json!("invalid")),
            ("unknown.field", serde_json::json!(true)),
        ] {
            let mut config = config();
            let before = serde_json::to_value(&config).unwrap();
            assert!(set_value(&mut config, key, value).is_err());
            assert_eq!(serde_json::to_value(&config).unwrap(), before);
        }
    }

    #[test]
    fn strings_arrays_and_zero_interval_keep_their_types() {
        let config = config();
        assert_eq!(
            parse_value(&config, "video_engine.display", "123").unwrap(),
            serde_json::json!("123")
        );
        assert_eq!(
            parse_value(&config, "video_engine.mpv_args", "a,b").unwrap(),
            serde_json::json!(["a", "b"])
        );
        assert_eq!(
            parse_value(&config, "video_engine.interval", "0").unwrap(),
            serde_json::json!(0)
        );
        assert!(parse_value(&config, "video_engine.mpv_args", "[1]").is_err());
    }
}
