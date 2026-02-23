use crate::components::{self, SOCKET_PATH};
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

/// Launch the full stack:
///   1. Silently initialise amem / abeat (idempotent)
///   2. exec(2) into the TypeScript TUI (acomm-tui), replacing this process
pub async fn start_stack(tool: &str) -> Result<(), Box<dyn std::error::Error>> {
    let s = components::detect().await;

    if !s.acomm_available {
        return Err(
            "acomm not found in PATH. \
             See https://github.com/yuiseki/acomm for installation instructions."
                .into(),
        );
    }

    // Silently initialise amem / abeat (idempotent — safe to run even if already initialised)
    if s.amem_available {
        let _ = Command::new("amem")
            .arg("init")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
    }
    if s.abeat_available {
        let _ = Command::new("abeat")
            .arg("init")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
    }

    let amem_root = std::env::var("AMEM_ROOT")
        .ok()
        .filter(|v| !v.is_empty())
        .map(|v| format!("{} (AMEM_ROOT)", v))
        .unwrap_or_else(|| "~/.amem (default)".to_string());
    eprintln!("Starting YuiClaw with tool: {}", tool);
    eprintln!("Memory: {}", amem_root);
    eprintln!("Launching acomm-tui... (press q to quit)");

    // exec(2) into acomm-tui — this process is replaced by the TypeScript TUI.
    // acomm-tui is the bin entry from repos/acomm/tui/package.json installed via `make install`.
    // Falls back to `acomm` (Rust TUI) if acomm-tui is not in PATH.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;

        // Prefer the TypeScript TUI; fall back to Rust TUI for backwards compatibility.
        let tui_cmd = if is_command_in_path("acomm-tui") {
            "acomm-tui"
        } else {
            "acomm"
        };

        let err = std::process::Command::new(tui_cmd)
            .arg("--tool")
            .arg(tool)
            .exec();
        return Err(format!("Failed to exec {}: {}", tui_cmd, err).into());
    }

    #[cfg(not(unix))]
    {
        let tui_cmd = if is_command_in_path("acomm-tui") {
            "acomm-tui"
        } else {
            "acomm"
        };
        let status = Command::new(tui_cmd)
            .arg("--tool")
            .arg(tool)
            .status()
            .await?;
        if !status.success() {
            return Err(format!("{} exited with non-zero status", tui_cmd).into());
        }
        Ok(())
    }
}

/// Stop the acomm bridge process and remove the socket file.
pub async fn stop_bridge() -> Result<(), Box<dyn std::error::Error>> {
    if !is_bridge_running() {
        println!("Bridge is not running.");
        return Ok(());
    }

    // Send SIGTERM to any process matching "acomm.*--bridge"
    let killed = Command::new("pkill")
        .arg("-f")
        .arg("acomm.*--bridge")
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false);

    if !killed {
        println!("Bridge process not found; cleaning up socket...");
    }

    // Remove the Unix socket file
    if Path::new(SOCKET_PATH).exists() {
        std::fs::remove_file(SOCKET_PATH)?;
        println!("Removed socket: {}", SOCKET_PATH);
    }

    println!("Bridge stopped.");
    Ok(())
}

/// Run abeat's due jobs (heartbeat tick).
pub async fn run_tick() -> Result<(), Box<dyn std::error::Error>> {
    if !components::is_command_available("abeat").await {
        return Err("abeat not found in PATH.".into());
    }

    let status = Command::new("abeat")
        .arg("tick")
        .arg("--due")
        .status()
        .await?;

    if !status.success() {
        return Err("abeat tick failed.".into());
    }

    Ok(())
}

/// Publish a message to the running bridge.
pub async fn publish(
    message: &str,
    channel: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    if !is_bridge_running() {
        return Err(
            "Bridge is not running. Start yuiclaw with `yuiclaw start`.".into(),
        );
    }

    let mut cmd = Command::new("acomm");
    cmd.arg("--publish").arg(message);
    if let Some(ch) = channel {
        cmd.arg("--channel").arg(ch);
    }

    let status = cmd.status().await?;
    if !status.success() {
        return Err("Failed to publish message to bridge.".into());
    }

    Ok(())
}

/// Reset the active session by sending /clear to the running bridge.
/// Clears the in-memory event backlog and the agent session manager.
/// The TUI (if connected) will display the bridge's "Cleared." acknowledgement.
pub async fn reset_session() -> Result<(), Box<dyn std::error::Error>> {
    if !is_bridge_running() {
        println!("No active session (bridge is not running).");
        return Ok(());
    }

    let mut cmd = Command::new("acomm");
    cmd.arg("--publish").arg("/clear");

    let status = cmd.status().await?;
    if !status.success() {
        return Err("Failed to send reset command to bridge.".into());
    }

    println!("Session reset.");
    Ok(())
}

fn is_bridge_running() -> bool {
    components::is_bridge_running()
}

/// Check whether a command name resolves to an executable in PATH.
fn is_command_in_path(cmd: &str) -> bool {
    std::process::Command::new("which")
        .arg(cmd)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
