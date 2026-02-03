//! Manager 模块：核心业务逻辑与状态管理的统一入口。
//!
//! ## 公共接口（方法签名）
//!
//! ### 生命周期管理
//! - `CoreManager::new() -> Result<Self, ManagerError>`
//! - `start(&mut self) -> Result<(), ManagerError>`
//! - `next(&mut self, mode: RunMode) -> Result<ManagerNextOutput, ManagerError>`
//! - `switch(&mut self, mode: RunMode) -> Result<(), ManagerError>`
//! - `stop(&mut self) -> Result<(), ManagerError>`
//! - `reload(&mut self, mode: RunMode) -> Result<ManagerReloadOutput, ManagerError>`
//! - `get_status(&self) -> ManagerStatusOutput`
//!
//! ### 自检接口
//! - `check_gpu(&self) -> DiagnoseGpuOutput`
//! - `check_engines(&self) -> DiagnoseEnginesOutput`
//! - `check_dirs(&self) -> DiagnoseDirsOutput`
//! - `check_all(&self) -> DiagnoseAllOutput`
//!
//! ### 配置管理
//! - `config_reset(&mut self) -> Result<PathBuf, ManagerError>`
//! - `config_get(&self) -> &Config`
//!
//! ### 壁纸管理
//! - `list(&mut self, mode: RunMode) -> Result<WallpaperListOutput, ManagerError>`
//! - `lock(&mut self, mode: RunMode, path: PathBuf) -> Result<LockOutput, ManagerError>`
//! - `unlock(&mut self, mode: RunMode, path: PathBuf) -> Result<LockOutput, ManagerError>`
//! - `stats(&mut self, mode: RunMode) -> Result<ModeStats, ManagerError>`
//!
//! ## 输入/输出结构体
//! - ManagerNextOutput / ManagerReloadOutput / ManagerStatusOutput
//! - DiagnoseGpuOutput / DiagnoseEnginesOutput / DiagnoseDirsOutput / DiagnoseAllOutput
//! - WallpaperListOutput / WallpaperInfo / LockOutput
//! - ModeStats
//!
//! ## 职责
//! - 集成所有 core 子模块（config/wallpaper/algorithm/engine/gpu/runtime）
//! - 提供统一的业务逻辑接口
//! - 管理权重缓存的持久化
//! - 实现壁纸锁定/解锁机制
//! - 运行时自检与诊断
//!
//! ## 核心特性
//! - **懒加载**：ModeManager 仅在需要时初始化
//! - **权重保护**：锁定的壁纸权重不被修改，不参与轮换
//! - **自动 reload**：启动时自动全量扫描验证文件存在性
//! - **缓存持久化**：每次切换壁纸后立即保存（含锁定状态）
//! - **完整自检**：GPU/引擎/目录一键检测
//!
//! ## 使用示例
//! ```rust,ignore
//! use crate::core::manager::CoreManager;
//! use crate::core::runtime::RunMode;
//!
//! // 创建管理器
//! let mut manager = CoreManager::new()?;
//!
//! // 完整自检
//! let diagnose = manager.check_all();
//! if !diagnose.all_passed {
//!     for err in diagnose.errors {
//!         eprintln!("自检失败: {}", err);
//!     }
//! }
//!
//! // 启动守护进程（阻塞式）
//! manager.start()?;
//!
//! // --- 以下为非阻塞式调用示例 ---
//!
//! // 手动切换壁纸
//! let result = manager.next(RunMode::Video)?;
//! println!("已切换: {:?}", result.selected_path);
//!
//! // 列出壁纸
//! let list = manager.list(RunMode::Video)?;
//! println!("活跃: {}, 锁定: {}", list.active.len(), list.locked.len());
//!
//! // 锁定壁纸
//! manager.lock(RunMode::Video, some_path)?;
//!
//! // 热重载
//! let reload_result = manager.reload(RunMode::Video)?;
//! println!("新增 {}, 删除 {}", reload_result.new_count, reload_result.removed_count);
//!
//! // 查询状态
//! let status = manager.get_status();
//! println!("当前模式: {:?}", status.current_mode);
//! ```

mod core_manager;
mod error;
mod mode_manager;
mod r#struct;

// 导出核心类型
pub use core_manager::CoreManager;

// 导出错误类型
pub use error::ManagerError;

// 导出结构体
pub use r#struct::{
    DiagnoseAllOutput, DiagnoseDirsOutput, DiagnoseEnginesOutput, DiagnoseGpuOutput, LockOutput,
    ManagerNextOutput, ManagerReloadOutput, ManagerStatusOutput, ModeStats, WallpaperInfo,
    WallpaperListOutput,
};
