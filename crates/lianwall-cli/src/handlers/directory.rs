//! 目录操作命令处理器
//!
//! - `reload` - 重新加载配置文件并重新扫描壁纸目录
//! - `rescan` - 重新扫描壁纸目录

use crate::output::Formatter;

use super::{connect, Result};

/// 处理 reload 命令
///
/// 重新加载配置文件并重新扫描壁纸目录。
///
/// # 与 rescan 的区别
/// - `reload`: 重新读取 config.toml 文件，更新 daemon 的所有配置状态，
///   如果配置中的壁纸目录路径发生变化，也会自动触发重新扫描。
/// - `rescan`: 只重新扫描壁纸目录发现新增/删除的文件，不读取配置文件。
pub fn handle_reload(fmt: &Formatter) -> Result<()> {
    let mut client = connect()?;
    client.reload_config()?;
    fmt.print_success("Reloaded config and rescanned directories");
    Ok(())
}

/// 处理 rescan 命令
///
/// 重新扫描壁纸目录，发现新增/删除的壁纸文件。
///
/// # 使用场景
/// - 在壁纸目录中添加或删除了壁纸文件
/// - 修改了壁纸文件的时间约束目录结构（如 `00-06/`）
///
/// # 与 reload 的区别
/// - `rescan`: 只重新扫描目录，不重新读取配置文件，适合壁纸文件变动的情况
/// - `reload`: 重新读取 config.toml，适合配置文件变动的情况
pub fn handle_rescan(fmt: &Formatter) -> Result<()> {
    let mut client = connect()?;
    client.rescan()?;
    fmt.print_success("Rescanned wallpaper directories");
    Ok(())
}
