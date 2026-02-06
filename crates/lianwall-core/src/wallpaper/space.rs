//! 向量空间构建与重建

use rand::prelude::*;
use rand::distributions::Uniform;
use std::collections::VecDeque;
use std::f64::consts::TAU;

use super::r#struct::{ModeData, PersistedRecord, WallpaperRecord, WallpaperSpace};
use super::scanner::ScannedWallpaper;

/// 构建新的向量空间
///
/// 将壁纸随机打乱后均匀分布到单位圆上
///
/// # Arguments
/// * `wallpapers` - 扫描到的壁纸列表（带时间约束）
/// * `seed` - 随机种子（0 表示使用系统熵）
pub fn build_space(wallpapers: Vec<ScannedWallpaper>, seed: u64) -> WallpaperSpace {
    if wallpapers.is_empty() {
        return WallpaperSpace {
            items: vec![],
            pointer: 0.0,
            cooldown_queue: VecDeque::new(),
            history: Vec::new(),
            current_index: None,
        };
    }

    // 创建 RNG
    let mut rng = if seed == 0 {
        StdRng::from_entropy()
    } else {
        StdRng::seed_from_u64(seed)
    };

    // Fisher-Yates 洗牌
    let mut wallpapers = wallpapers;
    wallpapers.shuffle(&mut rng);

    // 均匀分布到圆周
    let n = wallpapers.len();
    let items: Vec<WallpaperRecord> = wallpapers
        .into_iter()
        .enumerate()
        .map(|(i, w)| WallpaperRecord {
            path: w.path,
            angle: TAU * (i as f64) / (n as f64),
            locked: false,
            last_played: None,
            time_constraints: w.time_constraints,
        })
        .collect();

    // 随机初始指针
    let dist = Uniform::new(0.0, TAU);
    let pointer = rng.sample(dist);

    WallpaperSpace {
        items,
        pointer,
        cooldown_queue: VecDeque::new(),
        history: Vec::new(),
        current_index: None,
    }
}

/// 重建向量空间（保留历史状态）
///
/// # Arguments
/// * `wallpapers` - 新的扫描壁纸列表（带时间约束）
/// * `old_space` - 旧的运行时空间（可选）
/// * `persisted` - 持久化数据（可选）
/// * `seed` - 随机种子
pub fn rebuild_space(
    wallpapers: Vec<ScannedWallpaper>,
    old_space: Option<&WallpaperSpace>,
    persisted: Option<&ModeData>,
    seed: u64,
) -> WallpaperSpace {
    let mut space = build_space(wallpapers, seed);

    // 继承指针位置（优先使用运行时状态）
    if let Some(old) = old_space {
        space.pointer = old.pointer;
    } else if let Some(p) = persisted {
        space.pointer = p.pointer;
    }

    // 继承锁定状态和播放历史
    if let Some(p) = persisted {
        for item in &mut space.items {
            if let Some(old_record) = p.items.iter().find(|r| r.path == item.path) {
                item.locked = old_record.locked;
                item.last_played = old_record.last_played;
            }
        }
        
        // 恢复当前壁纸索引
        if let Some(ref current_path) = p.current_path {
            space.current_index = space.items.iter().position(|item| &item.path == current_path);
        }
    }

    space
}

/// 从 WallpaperSpace 导出为持久化格式
pub fn export_to_persisted(space: &WallpaperSpace) -> ModeData {
    // 获取当前壁纸路径
    let current_path = space.current_index
        .and_then(|idx| space.items.get(idx))
        .map(|item| item.path.clone());
    
    ModeData {
        pointer: space.pointer,
        current_path,
        items: space
            .items
            .iter()
            .map(|w| PersistedRecord {
                path: w.path.clone(),
                locked: w.locked,
                last_played: w.last_played,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_wallpapers(count: usize) -> Vec<ScannedWallpaper> {
        (0..count)
            .map(|i| ScannedWallpaper {
                path: PathBuf::from(format!("/test/{}.jpg", i)),
                time_constraints: vec![],
            })
            .collect()
    }

    #[test]
    fn test_build_space_empty() {
        let space = build_space(vec![], 42);
        assert!(space.is_empty());
    }

    #[test]
    fn test_build_space_deterministic() {
        let wallpapers1 = make_wallpapers(5);
        let wallpapers2 = make_wallpapers(5);

        let space1 = build_space(wallpapers1, 12345);
        let space2 = build_space(wallpapers2, 12345);

        // 相同种子应产生相同结果
        assert_eq!(space1.items.len(), space2.items.len());
        for (a, b) in space1.items.iter().zip(space2.items.iter()) {
            assert_eq!(a.path, b.path);
            assert_eq!(a.angle, b.angle);
        }
        assert_eq!(space1.pointer, space2.pointer);
    }

    #[test]
    fn test_angles_distributed() {
        let wallpapers = make_wallpapers(10);
        let space = build_space(wallpapers, 42);

        // 检查角度均匀分布
        for (i, item) in space.items.iter().enumerate() {
            let expected = TAU * (i as f64) / 10.0;
            assert!((item.angle - expected).abs() < 1e-10);
        }
    }
}
