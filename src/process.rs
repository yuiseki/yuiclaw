use crate::components::{self, SOCKET_PATH};
use std::collections::HashSet;
use std::path::Path;
use std::process::Stdio;
use tokio::net::UnixStream;
use tokio::process::Command;

#[derive(Debug, Clone, Copy)]
struct ChannelAdapterSpec {
    label: &'static str,
    env_keys: &'static [&'static str],
    adapter_flag: &'static str,
}

const CHANNEL_ADAPTER_SPECS: [ChannelAdapterSpec; 3] = [
    ChannelAdapterSpec {
        label: "ntfy",
        env_keys: &["NTFY_TOPIC"],
        adapter_flag: "--ntfy",
    },
    ChannelAdapterSpec {
        label: "Discord",
        env_keys: &["DISCORD_BOT_TOKEN"],
        adapter_flag: "--discord",
    },
    ChannelAdapterSpec {
        label: "Slack",
        env_keys: &["SLACK_APP_TOKEN", "SLACK_BOT_TOKEN"],
        adapter_flag: "--slack",
    },
];

/// Launch the full stack:
///   1. Silently initialise amem / abeat (idempotent)
///   2. exec(2) into the TypeScript TUI (acomm-tui), replacing this process
pub async fn start_stack(provider: &str) -> Result<(), Box<dyn std::error::Error>> {
    let s = components::detect().await;

    if !s.acomm_available {
        return Err("acomm not found in PATH. \
             See https://github.com/yuiseki/acomm for installation instructions."
            .into());
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

    auto_start_configured_adapters().await;

    let amem_root = std::env::var("AMEM_ROOT")
        .ok()
        .filter(|v| !v.is_empty())
        .map(|v| format!("{} (AMEM_ROOT)", v))
        .unwrap_or_else(|| "~/.amem (default)".to_string());
    eprintln!("Starting YuiClaw...");
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
            .arg("--provider")
            .arg(provider)
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
            .arg("--provider")
            .arg(provider)
            .status()
            .await?;
        if !status.success() {
            return Err(format!("{} exited with non-zero status", tui_cmd).into());
        }
        Ok(())
    }
}

/// Start the stack with optional new-session semantics.
///
/// If `new_session` is true and the bridge is already running, a `/clear`
/// command is sent to the bridge before exec'ing into the TUI, discarding any
/// existing session state for the selected provider.
pub async fn start_stack_with_opts(
    provider: &str,
    new_session: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if new_session && is_bridge_running() {
        // Ask the bridge to discard the current session before we attach
        let _ = Command::new("acomm")
            .arg("--publish")
            .arg("/clear")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
        // Brief pause so the bridge has time to process /clear before we exec
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    start_stack(provider).await
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
        return Err("Bridge is not running. Start yuiclaw with `yuiclaw start`.".into());
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

async fn auto_start_configured_adapters() {
    let present_env_keys = present_nonempty_env_keys();
    let mut process_list = read_process_list().await.unwrap_or_default();

    let adapters_to_start = adapters_to_autostart_from_inputs(&present_env_keys, &process_list);
    if adapters_to_start.is_empty() {
        return;
    }

    let bridge_ready = ensure_bridge_running_for_adapters().await;
    if !bridge_ready {
        eprintln!(
            "Warning: bridge was not ready; skipping auto-start for configured channel adapters."
        );
        return;
    }
    process_list = read_process_list().await.unwrap_or_default();

    for spec in adapters_to_start {
        // Re-check against the latest process list so we don't duplicate after a bridge refresh.
        if process_list_has_acomm_flag(&process_list, spec.adapter_flag) {
            continue;
        }

        match std::process::Command::new("acomm")
            .arg(spec.adapter_flag)
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
        {
            Ok(_) => {
                eprintln!("Auto-started acomm adapter: {}", spec.label);
            }
            Err(err) => {
                eprintln!(
                    "Warning: failed to auto-start acomm adapter {} ({}): {}",
                    spec.label, spec.adapter_flag, err
                );
            }
        }
    }
}

async fn ensure_bridge_running_for_adapters() -> bool {
    for _ in 0..20 {
        let process_list = read_process_list().await.unwrap_or_default();
        let bridge_process_running = process_list_has_acomm_flag(&process_list, "--bridge");

        if bridge_process_running && bridge_socket_accepts_connection().await {
            return true;
        }

        if bridge_process_running {
            // Bridge process exists but socket is not accepting yet (startup race). Wait.
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            continue;
        }

        if components::is_bridge_running() {
            // Stale socket file from a previous crash can block bridge/adapters.
            let _ = std::fs::remove_file(SOCKET_PATH);
        }

        if std::process::Command::new("acomm")
            .arg("--bridge")
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .is_err()
        {
            return false;
        }

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    bridge_socket_accepts_connection().await
}

fn present_nonempty_env_keys() -> HashSet<String> {
    std::env::vars()
        .filter_map(|(k, v)| if v.trim().is_empty() { None } else { Some(k) })
        .collect()
}

async fn read_process_list() -> Option<String> {
    let out = Command::new("ps")
        .args(["-eo", "comm=,args="])
        .output()
        .await
        .ok()?;

    if !out.status.success() {
        return None;
    }

    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn adapters_to_autostart_from_inputs(
    present_env_keys: &HashSet<String>,
    process_list: &str,
) -> Vec<&'static ChannelAdapterSpec> {
    CHANNEL_ADAPTER_SPECS
        .iter()
        .filter(|spec| is_adapter_configured(spec, present_env_keys))
        .filter(|spec| !process_list_has_acomm_flag(process_list, spec.adapter_flag))
        .collect()
}

fn is_adapter_configured(spec: &ChannelAdapterSpec, present_env_keys: &HashSet<String>) -> bool {
    spec.env_keys.iter().all(|k| present_env_keys.contains(*k))
}

async fn bridge_socket_accepts_connection() -> bool {
    UnixStream::connect(SOCKET_PATH).await.is_ok()
}

fn process_list_has_acomm_flag(process_list: &str, flag: &str) -> bool {
    process_list
        .lines()
        .any(|line| process_line_matches_acomm_flag(line, flag))
}

fn process_line_matches_acomm_flag(line: &str, flag: &str) -> bool {
    let trimmed = line.trim_start();
    if trimmed.is_empty() {
        return false;
    }

    let Some((comm, args)) = trimmed.split_once(char::is_whitespace) else {
        return false;
    };

    if comm != "acomm" {
        return false;
    }

    args.split_whitespace().any(|token| token == flag)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_keys(keys: &[&str]) -> HashSet<String> {
        keys.iter().map(|k| (*k).to_string()).collect()
    }

    #[test]
    fn adapters_to_autostart_uses_only_configured_services() {
        let rows = adapters_to_autostart_from_inputs(&env_keys(&[]), "");
        assert!(rows.is_empty());

        let discord = adapters_to_autostart_from_inputs(&env_keys(&["DISCORD_BOT_TOKEN"]), "");
        assert_eq!(discord.len(), 1);
        assert_eq!(discord[0].label, "Discord");

        let slack_missing_one =
            adapters_to_autostart_from_inputs(&env_keys(&["SLACK_APP_TOKEN"]), "");
        assert!(slack_missing_one.is_empty());
    }

    #[test]
    fn adapters_to_autostart_skips_already_running_adapter_processes() {
        let ps_output = "acomm           acomm --discord\nacomm           acomm --bridge\n";
        let rows = adapters_to_autostart_from_inputs(&env_keys(&["DISCORD_BOT_TOKEN"]), ps_output);
        assert!(rows.is_empty());
    }

    #[test]
    fn process_match_requires_acomm_binary_and_exact_flag() {
        assert!(process_line_matches_acomm_flag(
            "acomm           /home/user/.cargo/bin/acomm --discord",
            "--discord"
        ));
        assert!(!process_line_matches_acomm_flag(
            "cargo           cargo run -p acomm -- --discord",
            "--discord"
        ));
        assert!(!process_line_matches_acomm_flag(
            "acomm           acomm --discordx",
            "--discord"
        ));
    }
}
