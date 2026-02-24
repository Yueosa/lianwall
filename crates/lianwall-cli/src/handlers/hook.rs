//! Hook 命令处理器
//!
//! - `hook list`   - 列出当前 hook 配置
//! - `hook reload` - 重新加载 hooks.toml

use crate::commands::HookAction;
use crate::output::Formatter;

use super::{connect, HandlerError, Result};

/// 处理 hook 子命令
pub fn handle_hook(fmt: &Formatter, action: HookAction) -> Result<()> {
    match action {
        HookAction::List => handle_hook_list(fmt),
        HookAction::Reload => handle_hook_reload(fmt),
    }
}

fn handle_hook_list(fmt: &Formatter) -> Result<()> {
    let mut client = connect()?;
    let hooks = client.list_hooks()?;

    if fmt.is_json() {
        println!("{}", serde_json::to_string_pretty(&hooks).unwrap());
        return Ok(());
    }

    if hooks.is_empty() {
        fmt.print_info("No hooks configured");
        fmt.print_info(&format!(
            "Edit {} to add hooks",
            lianwall_core::hook::hooks_path().display()
        ));
        return Ok(());
    }

    let enabled = hooks.iter().filter(|h| h.enabled).count();
    let disabled = hooks.len() - enabled;

    fmt.print_info(&format!(
        "{} hooks ({} enabled, {} disabled)",
        hooks.len(),
        enabled,
        disabled
    ));
    println!();

    for (i, hook) in hooks.iter().enumerate() {
        let status = if hook.enabled {
            fmt.success("●").to_string()
        } else {
            fmt.dim("○").to_string()
        };

        println!(
            "  {} {} {} on {}",
            status,
            fmt.bold(&format!("[{}]", i + 1)),
            fmt.bold(&hook.name),
            fmt.info(&hook.on),
        );

        // 命令（截断显示）
        let cmd_display = if hook.command.len() > 60 {
            format!("{}...", &hook.command[..57])
        } else {
            hook.command.clone()
        };
        println!("    cmd: {}", fmt.dim(&cmd_display));

        // 可选过滤条件
        if let Some(ref mode) = hook.mode {
            println!("    mode: {}", mode);
        }
        if let Some(ref triggers) = hook.trigger {
            println!("    trigger: {}", triggers.join(", "));
        }
        if hook.timeout != 10 {
            println!("    timeout: {}s", hook.timeout);
        }
    }

    Ok(())
}

fn handle_hook_reload(fmt: &Formatter) -> Result<()> {
    let mut client = connect()?;
    client.reload_hooks().map_err(|e| {
        HandlerError::Other(format!("Failed to reload hooks: {}", e))
    })?;
    if fmt.is_json() {
        println!("{}", serde_json::json!({"success": true}));
    } else {
        fmt.print_success("Hooks reloaded");
    }
    Ok(())
}
