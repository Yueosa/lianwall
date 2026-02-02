//! 核心管理模块
//!
//! ## 职责
//! - 集成所有 core 子模块（config/wallpaper/algorithm/engine/gpu/runtime）
//! - 提供统一的业务逻辑接口
//! - 管理权重缓存的持久化
//! - 实现权重保护机制（封锁非活跃记录）
//!
//! ## 核心特性
//! - **懒加载**：Image Manager 仅在降级时初始化
//! - **权重保护**：非活跃记录（时间段未匹配）权重不被修改
//! - **自动 reload**：启动时自动全量扫描验证文件存在性
//! - **缓存持久化**：每次切换壁纸后立即保存
//!
//! ## 使用示例
//! ```rust
//! use crate::core::manager::CoreManager;
//!
//! // 启动守护进程
//! let mut manager = CoreManager::new()?;
//! manager.start()?;  // 阻塞式运行
//!
//! // 手动切换壁纸
//! let result = manager.next(RunMode::Video)?;
//! println!("已切换: {:?}", result.selected_path);
//!
//! // 热重载
//! let reload_result = manager.reload(RunMode::Video)?;
//! println!("新增 {} 个, 删除 {} 个", reload_result.new_count, reload_result.removed_count);
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
pub use r#struct::{ManagerNextOutput, ManagerReloadOutput, ManagerStatusOutput, ModeStats};
