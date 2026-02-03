use crate::core::wallpaper::error::WallpaperError;
use crate::core::wallpaper::r#struct::TimeRange;

/// 解析时间段目录名
///
/// 支持格式：
/// - HH-HH: "18-23", "06-12"
/// - HHMM-HHMM: "1830-2300", "2200-0130"
/// - HH-HHMM 或 HHMM-HH: "18-2330", "1830-23"
pub fn parse_time_range(dir_name: &str) -> Option<TimeRange> {
    let parts: Vec<&str> = dir_name.split('-').collect();
    if parts.len() != 2 {
        return None;
    }

    let start = parse_time_part(parts[0])?;
    let end = parse_time_part(parts[1])?;

    let start_minutes = start.0 * 60 + start.1;
    let end_minutes = end.0 * 60 + end.1;

    // 分钟级别跨天判断
    let is_overnight = end_minutes < start_minutes;

    Some(TimeRange {
        start_minutes,
        end_minutes,
        is_overnight,
    })
}

/// 解析时间部分（HH 或 HHMM）
/// 支持 24 或 2400 表示午夜（等价于 00:00）
fn parse_time_part(s: &str) -> Option<(u16, u16)> {
    match s.len() {
        2 => {
            // HH 格式
            let hour: u16 = s.parse().ok()?;
            // 24 表示午夜，等价于 00:00
            if hour == 24 {
                return Some((24, 0));
            }
            if hour > 23 {
                return None;
            }
            Some((hour, 0))
        }
        4 => {
            // HHMM 格式
            let hour: u16 = s[0..2].parse().ok()?;
            let minute: u16 = s[2..4].parse().ok()?;
            // 2400 表示午夜，等价于 00:00
            if hour == 24 && minute == 0 {
                return Some((24, 0));
            }
            if hour > 23 || minute > 59 {
                return None;
            }
            Some((hour, minute))
        }
        _ => None,
    }
}

/// 检查当前时间是否在时间段内
///
/// # 参数
/// - `range`: 时间段
/// - `now`: 当前时间 (hour, minute)
pub fn is_in_range(range: &TimeRange, now: (u8, u8)) -> bool {
    let now_minutes = (now.0 as u16) * 60 + (now.1 as u16);

    if range.is_overnight {
        // 跨天：当前时间 >= 开始时间 或 当前时间 <= 结束时间
        now_minutes >= range.start_minutes || now_minutes <= range.end_minutes
    } else {
        // 不跨天：当前时间在 [开始, 结束] 区间内
        now_minutes >= range.start_minutes && now_minutes <= range.end_minutes
    }
}

/// 验证时间段是否有效
pub fn validate_time_range(input: &str) -> Result<TimeRange, WallpaperError> {
    parse_time_range(input).ok_or_else(|| WallpaperError::InvalidTimeRange {
        input: input.to_string(),
        reason: "格式应为 HH-HH 或 HHMM-HHMM".to_string(),
    })
}

/// 将 TimeRange 格式化为可读字符串 (HH:MM)
pub fn format_time_range(range: &TimeRange) -> (String, String) {
    let start_h = range.start_minutes / 60;
    let start_m = range.start_minutes % 60;
    let end_h = range.end_minutes / 60;
    let end_m = range.end_minutes % 60;

    (
        format!("{:02}:{:02}", start_h, start_m),
        format!("{:02}:{:02}", end_h, end_m),
    )
}
