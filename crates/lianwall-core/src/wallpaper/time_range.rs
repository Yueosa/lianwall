//! 时间段解析与判断

use std::collections::BTreeSet;
use chrono::Timelike;

/// 时间点（用于关键时间调度）
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TimePoint {
    pub hour: u8,
    pub minute: u8,
}

impl TimePoint {
    /// 创建时间点
    pub fn new(hour: u8, minute: u8) -> Self {
        // 24:xx 当作 00:xx 处理
        let hour = if hour >= 24 { hour - 24 } else { hour };
        Self { hour, minute }
    }

    /// 从当前时间创建
    pub fn now() -> Self {
        let now = chrono::Local::now();
        Self {
            hour: now.hour() as u8,
            minute: now.minute() as u8,
        }
    }

    /// 转换为分钟数（用于比较）
    pub fn to_minutes(&self) -> u16 {
        self.hour as u16 * 60 + self.minute as u16
    }

    /// 计算到下一个时间点的秒数
    pub fn seconds_until(&self, target: &TimePoint) -> u64 {
        let now_mins = self.to_minutes() as i32;
        let target_mins = target.to_minutes() as i32;

        let diff = if target_mins > now_mins {
            target_mins - now_mins
        } else {
            // 跨天
            (24 * 60 - now_mins) + target_mins
        };

        diff as u64 * 60
    }
}

/// 时间范围
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TimeRange {
    pub start: TimePoint,
    pub end: TimePoint,
}

impl TimeRange {
    /// 判断给定时间是否在范围内
    pub fn is_active(&self, time: &TimePoint) -> bool {
        let t = time.to_minutes();
        let s = self.start.to_minutes();
        let e = self.end.to_minutes();

        if s <= e {
            // 非跨天：08:00-18:00
            t >= s && t < e
        } else {
            // 跨天：23:00-06:00
            t >= s || t < e
        }
    }

    /// 是否跨天（如 23:00-06:00）
    pub fn crosses_midnight(&self) -> bool {
        self.start.to_minutes() > self.end.to_minutes()
    }

    /// 获取关键时间点（开始和结束）
    pub fn key_points(&self) -> [TimePoint; 2] {
        [self.start, self.end]
    }
}

/// 解析时间目录名
///
/// 支持格式：
/// - `HH-HH`: 08-18
/// - `HHMM-HHMM`: 0830-1745
/// - 混合: 08-1730, 0830-18
///
/// # Returns
/// `Some(TimeRange)` 如果是有效的时间目录名，否则 `None`
pub fn parse_time_dir(name: &str) -> Option<TimeRange> {
    let parts: Vec<&str> = name.split('-').collect();
    if parts.len() != 2 {
        return None;
    }

    let start = parse_time_part(parts[0])?;
    let end = parse_time_part(parts[1])?;

    Some(TimeRange { start, end })
}

/// 解析单个时间部分（HH 或 HHMM）
fn parse_time_part(s: &str) -> Option<TimePoint> {
    match s.len() {
        2 => {
            // HH 格式
            let hour: u8 = s.parse().ok()?;
            if hour > 24 {
                return None;
            }
            Some(TimePoint::new(hour, 0))
        }
        4 => {
            // HHMM 格式
            let hour: u8 = s[0..2].parse().ok()?;
            let minute: u8 = s[2..4].parse().ok()?;
            if hour > 24 || minute > 59 {
                return None;
            }
            Some(TimePoint::new(hour, minute))
        }
        _ => None,
    }
}

/// 找到下一个关键时间点
pub fn next_key_point(current: &TimePoint, points: &BTreeSet<TimePoint>) -> Option<TimePoint> {
    if points.is_empty() {
        return None;
    }

    // 找到第一个大于当前时间的点
    for point in points.iter() {
        if point > current {
            return Some(*point);
        }
    }

    // 没有找到，返回第一个点（跨天）
    points.iter().next().copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_time_dir() {
        // HH-HH
        let r = parse_time_dir("08-18").unwrap();
        assert_eq!(r.start, TimePoint::new(8, 0));
        assert_eq!(r.end, TimePoint::new(18, 0));

        // HHMM-HHMM
        let r = parse_time_dir("0830-1745").unwrap();
        assert_eq!(r.start, TimePoint::new(8, 30));
        assert_eq!(r.end, TimePoint::new(17, 45));

        // 混合
        let r = parse_time_dir("08-1730").unwrap();
        assert_eq!(r.start, TimePoint::new(8, 0));
        assert_eq!(r.end, TimePoint::new(17, 30));

        // 跨天
        let r = parse_time_dir("2300-0600").unwrap();
        assert_eq!(r.start, TimePoint::new(23, 0));
        assert_eq!(r.end, TimePoint::new(6, 0));

        // 24:00 当作 00:00
        let r = parse_time_dir("2200-2400").unwrap();
        assert_eq!(r.end, TimePoint::new(0, 0));

        // 无效格式
        assert!(parse_time_dir("invalid").is_none());
        assert!(parse_time_dir("08").is_none());
        assert!(parse_time_dir("08-18-20").is_none());
    }

    #[test]
    fn test_is_active() {
        // 非跨天
        let r = TimeRange {
            start: TimePoint::new(8, 0),
            end: TimePoint::new(18, 0),
        };
        assert!(r.is_active(&TimePoint::new(8, 0)));
        assert!(r.is_active(&TimePoint::new(12, 0)));
        assert!(!r.is_active(&TimePoint::new(18, 0))); // 结束时间不包含
        assert!(!r.is_active(&TimePoint::new(7, 59)));
        assert!(!r.is_active(&TimePoint::new(20, 0)));

        // 跨天
        let r = TimeRange {
            start: TimePoint::new(23, 0),
            end: TimePoint::new(6, 0),
        };
        assert!(r.is_active(&TimePoint::new(23, 0)));
        assert!(r.is_active(&TimePoint::new(0, 0)));
        assert!(r.is_active(&TimePoint::new(3, 0)));
        assert!(!r.is_active(&TimePoint::new(6, 0))); // 结束时间不包含
        assert!(!r.is_active(&TimePoint::new(12, 0)));
    }

    #[test]
    fn test_next_key_point() {
        let mut points = BTreeSet::new();
        points.insert(TimePoint::new(8, 0));
        points.insert(TimePoint::new(12, 0));
        points.insert(TimePoint::new(18, 0));

        assert_eq!(
            next_key_point(&TimePoint::new(7, 0), &points),
            Some(TimePoint::new(8, 0))
        );
        assert_eq!(
            next_key_point(&TimePoint::new(8, 0), &points),
            Some(TimePoint::new(12, 0))
        );
        assert_eq!(
            next_key_point(&TimePoint::new(20, 0), &points),
            Some(TimePoint::new(8, 0))
        ); // 跨天
    }
}
