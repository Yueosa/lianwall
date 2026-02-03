use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use crate::core::algorithm::{
    get_stats, initialize, select, update_weights, AlgorithmInitInput, AlgorithmSelectInput,
    AlgorithmStatsOutput, AlgorithmUpdateInput, WeightRecord, WeightUpdateConfig,
};
use crate::core::engine::EngineType;
use crate::core::manager::error::ManagerError;
use crate::core::manager::r#struct::ManagerReloadOutput;
use crate::core::wallpaper::{scan, WallpaperScanInput};

/// 模式管理器（内部结构）
pub struct ModeManager {
    /// 完整的权重记录（包括所有历史文件，含时间段未匹配的）
    pub all_records: Vec<WeightRecord>,

    /// 当前活跃的记录（当前时间段匹配的，参与选择）
    pub active_records: Vec<WeightRecord>,

    /// 引擎类型
    pub engine_type: EngineType,

    /// 缓存文件路径
    pub cache_path: PathBuf,

    /// 扫描配置
    pub scan_config: WallpaperScanInput,
}

impl ModeManager {
    /// 初始化 ModeManager
    ///
    /// 流程：
    /// 1. 从 cache_path 加载缓存 → all_records
    /// 2. 调用 wallpaper::scan() (use_time_ranges=true)
    /// 3. 提取活跃记录：all_records 中在扫描结果中的
    /// 4. 检测新文件：扫描结果中不在 all_records 中的
    /// 5. algorithm::initialize() 初始化新文件权重
    /// 6. 合并新记录到 active_records
    pub fn new(
        cache_path: PathBuf,
        scan_config: WallpaperScanInput,
        engine_type: EngineType,
        base_weight: f64,
    ) -> Result<Self, ManagerError> {
        let mut manager = Self {
            all_records: Self::load_cache(&cache_path),
            active_records: Vec::new(),
            engine_type,
            cache_path,
            scan_config,
        };

        // 初始化时执行一次 reload
        manager.reload(base_weight)?;

        Ok(manager)
    }

    /// 重新扫描（用于 reload）
    pub fn reload(&mut self, base_weight: f64) -> Result<ManagerReloadOutput, ManagerError> {
        // 1. 全量扫描（禁用时间段过滤）
        let full_scan = scan(WallpaperScanInput {
            base_dir: self.scan_config.base_dir.clone(),
            extensions: self.scan_config.extensions.clone(),
            use_time_ranges: false,
        })?;

        let full_paths: HashSet<PathBuf> = full_scan.wallpapers.into_iter().collect();

        // 2. 移除已删除的文件
        let before_count = self.all_records.len();
        self.all_records.retain(|r| full_paths.contains(&r.path));
        let removed_count = before_count - self.all_records.len();

        // 3. 时间段扫描（获取活跃列表）
        let time_scan = scan(WallpaperScanInput {
            base_dir: self.scan_config.base_dir.clone(),
            extensions: self.scan_config.extensions.clone(),
            use_time_ranges: self.scan_config.use_time_ranges,
        })?;

        // 4. 提取活跃记录
        let active_paths: HashSet<PathBuf> = time_scan.wallpapers.iter().cloned().collect();
        self.active_records = self
            .all_records
            .iter()
            .filter(|r| active_paths.contains(&r.path))
            .cloned()
            .collect();

        // 5. 检测新文件
        let existing_paths: HashSet<PathBuf> =
            self.all_records.iter().map(|r| r.path.clone()).collect();

        let new_files: Vec<PathBuf> = time_scan
            .wallpapers
            .into_iter()
            .filter(|p| !existing_paths.contains(p))
            .collect();

        // 6. 初始化新文件权重
        let new_count = new_files.len();
        if !new_files.is_empty() {
            let init_result = initialize(AlgorithmInitInput {
                wallpapers: new_files,
                cached_records: self.active_records.clone(),
                base_weight,
            });

            // 新记录同时加入活跃和完整列表
            self.active_records.extend(init_result.records.clone());
            self.all_records.extend(init_result.records);
        }

        // 7. 保存缓存
        self.save_cache()?;

        Ok(ManagerReloadOutput {
            total_count: self.all_records.len(),
            active_count: self.active_records.len(),
            new_count,
            removed_count,
        })
    }

    /// 选择壁纸（从活跃记录）
    pub fn select(
        &self,
        tolerance: f64,
        perturbation_ratio: f64,
    ) -> Result<(usize, PathBuf), ManagerError> {
        if self.active_records.is_empty() {
            return Err(ManagerError::NoWallpapersAvailable);
        }

        let select_result = select(AlgorithmSelectInput {
            records: self.active_records.clone(),
            tolerance,
            perturbation_ratio,
        })?;

        Ok((select_result.selected_index, select_result.selected_path))
    }

    /// 更新权重（只更新活跃记录）
    ///
    /// 流程：
    /// 1. algorithm::update_weights() 更新活跃记录
    /// 2. 合并到 all_records
    /// 3. 保存缓存
    pub fn update_and_save(
        &mut self,
        selected_index: usize,
        config: WeightUpdateConfig,
        selection_count: u32,
    ) -> Result<(bool, bool), ManagerError> {
        let update_result = update_weights(AlgorithmUpdateInput {
            records: self.active_records.clone(),
            selected_index,
            config,
            selection_count,
        })?;

        // 更新活跃记录
        self.active_records = update_result.updated_records.clone();

        // 合并到完整记录
        Self::merge_records(&mut self.all_records, update_result.updated_records);

        // 保存缓存
        self.save_cache()?;

        Ok((update_result.normalized, update_result.shuffled))
    }

    /// 获取统计信息
    pub fn get_stats(&self) -> AlgorithmStatsOutput {
        get_stats(&self.active_records)
    }

    /// 保存缓存
    fn save_cache(&self) -> Result<(), ManagerError> {
        // 确保缓存目录存在
        if let Some(parent) = self.cache_path.parent() {
            fs::create_dir_all(parent).map_err(|e| ManagerError::Cache {
                path: parent.to_path_buf(),
                source: e,
            })?;
        }

        let content =
            serde_json::to_string_pretty(&self.all_records).map_err(|e| {
                ManagerError::JsonSerialize {
                    path: self.cache_path.clone(),
                    source: e,
                }
            })?;

        fs::write(&self.cache_path, content).map_err(|e| ManagerError::Cache {
            path: self.cache_path.clone(),
            source: e,
        })?;

        Ok(())
    }

    /// 加载缓存
    fn load_cache(path: &PathBuf) -> Vec<WeightRecord> {
        if !path.exists() {
            return Vec::new();
        }

        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        serde_json::from_str(&content).unwrap_or_default()
    }

    /// 合并活跃记录到完整记录
    ///
    /// 规则：
    /// 1. 更新活跃记录的权重
    /// 2. 保留非活跃记录的权重（封锁保护）
    fn merge_records(all_records: &mut Vec<WeightRecord>, updated_active: Vec<WeightRecord>) {
        // 构建活跃记录的路径集合
        let active_paths: HashSet<PathBuf> =
            updated_active.iter().map(|r| r.path.clone()).collect();

        // 移除旧的活跃记录
        all_records.retain(|r| !active_paths.contains(&r.path));

        // 添加新的活跃记录
        all_records.extend(updated_active);
    }
}
