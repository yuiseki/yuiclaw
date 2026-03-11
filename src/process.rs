use crate::components::{self, SOCKET_PATH};
use crate::voice_command::{
    VoiceCommandOperatorAction, build_voice_command_launch_spec,
    resolve_voice_command_operator_runtime_config,
};
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::net::{TcpStream, UnixStream};
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
///   1. If daemon (bridge) is not running: silently initialise amem / abeat and start adapters
///   2. exec(2) into the TypeScript TUI (acomm-tui), replacing this process
pub async fn start_stack(provider: &str) -> Result<(), Box<dyn std::error::Error>> {
    if !is_bridge_running() {
        initialize_runtime_components().await?;
        auto_start_configured_adapters().await;
    }

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

/// Restart the headless runtime services (bridge + configured adapters) without launching the TUI.
///
/// Equivalent to `yuiclaw stop` followed by the non-interactive startup portion of `yuiclaw start`.
pub async fn restart_stack() -> Result<(), Box<dyn std::error::Error>> {
    stop_bridge().await?;
    initialize_runtime_components().await?;

    if !ensure_bridge_running_for_adapters().await {
        return Err("Failed to start acomm bridge.".into());
    }
    auto_start_configured_adapters().await;

    println!("Bridge restarted. (TUI not started)");
    Ok(())
}

/// Start the daemon (bridge + configured adapters) in the background without launching the TUI.
pub async fn daemon_start() -> Result<(), Box<dyn std::error::Error>> {
    if is_bridge_running() {
        println!("Daemon is already running.");
        return Ok(());
    }
    initialize_runtime_components().await?;

    if !ensure_bridge_running_for_adapters().await {
        return Err("Failed to start acomm bridge.".into());
    }
    auto_start_configured_adapters().await;

    println!("Daemon started. (bridge + adapters running in background)");
    Ok(())
}

/// Stop all adapter processes and the acomm bridge.
pub async fn daemon_stop() -> Result<(), Box<dyn std::error::Error>> {
    stop_all_adapters().await;
    stop_bridge().await
}

/// Restart the daemon (stop all, then start again).
pub async fn daemon_restart() -> Result<(), Box<dyn std::error::Error>> {
    daemon_stop().await?;
    // Be extra defensive on restart: remove any leftover socket before startup.
    // This covers cases where the bridge process has exited but the stale socket
    // file remains and can confuse daemon-start checks.
    let _ = remove_socket_file_if_exists(SOCKET_PATH)?;
    daemon_start().await
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
    remove_socket_file_if_exists(SOCKET_PATH)?;

    println!("Bridge stopped.");
    Ok(())
}

/// Stop all configured channel adapter processes (ntfy, Discord, Slack).
async fn stop_all_adapters() {
    for spec in &CHANNEL_ADAPTER_SPECS {
        let pattern = format!("acomm.*{}", spec.adapter_flag);
        let _ = Command::new("pkill").arg("-f").arg(&pattern).status().await;
    }
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

/// Launch the current voice command operator compatibility entrypoint.
pub async fn run_voice_command(
    run_command: Option<&str>,
    extra_args: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    let spec = build_voice_command_launch_spec(run_command, extra_args);
    if !spec.entrypoint_path.is_file() {
        return Err(format!(
            "voice command entrypoint not found: {}",
            spec.entrypoint_path.display()
        )
        .into());
    }

    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    for (key, value) in &spec.env {
        command.env(key, value);
    }

    let status = command.status().await?;

    if !status.success() {
        return Err(format!("voice command exited with status {}", status).into());
    }

    Ok(())
}

async fn tmux_has_session(session: &str) -> bool {
    Command::new("tmux")
        .arg("has-session")
        .arg("-t")
        .arg(session)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map(|status| status.success())
        .unwrap_or(false)
}

async fn tmux_kill_session_if_exists(
    session: &str,
    stopped_message: &str,
    missing_message: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if !tmux_has_session(session).await {
        println!("{missing_message}");
        return Ok(());
    }

    let status = Command::new("tmux")
        .arg("kill-session")
        .arg("-t")
        .arg(session)
        .status()
        .await?;
    if !status.success() {
        return Err(format!("failed to kill tmux session: {session}").into());
    }
    println!("{stopped_message}");
    Ok(())
}

fn shell_single_quote(path: &Path) -> String {
    shell_single_quote_str(&path.display().to_string())
}

fn shell_single_quote_str(raw: &str) -> String {
    format!("'{}'", raw.replace('\'', r"'\''"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum VoiceCommandServiceState {
    Running(String),
    Stopped(String),
    Disabled(String),
    NotApplicable(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum VoiceCommandEndpointState {
    Ready(String, String),
    NotReady(String, String),
    Disabled(String),
    NotApplicable(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VoiceCommandStatusSnapshot {
    stt_backend: String,
    moonshine_model_size: String,
    server_session: String,
    server_url: String,
    server_model: String,
    language: String,
    listener_session: String,
    agent_session: String,
    overlay_session: String,
    lock_screen_session: String,
    overlay_ipc: String,
    lock_screen_ipc: String,
    listener_script: String,
    agent_script: String,
    mic_source: String,
    active_listener_source: Option<String>,
    active_agent_source: Option<String>,
    server_session_state: VoiceCommandServiceState,
    listener_session_state: VoiceCommandServiceState,
    agent_session_state: VoiceCommandServiceState,
    overlay_session_state: VoiceCommandServiceState,
    lock_screen_session_state: VoiceCommandServiceState,
    server_endpoint: VoiceCommandEndpointState,
    overlay_endpoint: VoiceCommandEndpointState,
    lock_screen_endpoint: VoiceCommandEndpointState,
    contention_warning: bool,
}

fn extract_parec_source_from_args(args: &str) -> Option<String> {
    let mut parts = args.split_whitespace();
    if parts.next()? != "parec" {
        return None;
    }

    while let Some(part) = parts.next() {
        if part == "-d" {
            return parts.next().map(ToOwned::to_owned);
        }
    }
    None
}

async fn tmux_pane_pid(session: &str) -> Option<String> {
    let output = Command::new("tmux")
        .arg("list-panes")
        .arg("-t")
        .arg(format!("{session}:0"))
        .arg("-F")
        .arg("#{pane_pid}")
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToOwned::to_owned)
}

async fn parec_source_for_pid(parent_pid: &str) -> Option<String> {
    let output = Command::new("pgrep")
        .arg("-P")
        .arg(parent_pid)
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let child_pids = String::from_utf8_lossy(&output.stdout);
    for child_pid in child_pids
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let ps_output = Command::new("ps")
            .arg("-p")
            .arg(child_pid)
            .arg("-o")
            .arg("args=")
            .output()
            .await
            .ok()?;
        if !ps_output.status.success() {
            continue;
        }
        let args = String::from_utf8_lossy(&ps_output.stdout);
        if let Some(source) = extract_parec_source_from_args(args.trim()) {
            return Some(source);
        }
    }
    None
}

async fn active_source_for_session(session: &str) -> Option<String> {
    let pane_pid = tmux_pane_pid(session).await?;
    parec_source_for_pid(&pane_pid).await
}

async fn http_endpoint_ready(url: &str) -> bool {
    Command::new("curl")
        .arg("-fsS")
        .arg("--max-time")
        .arg("2")
        .arg(format!("{url}/"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map(|status| status.success())
        .unwrap_or(false)
}

async fn tcp_endpoint_ready(host: &str, port: &str) -> bool {
    let port = match port.parse::<u16>() {
        Ok(value) => value,
        Err(_) => return false,
    };
    TcpStream::connect((host, port)).await.is_ok()
}

async fn wait_tcp_endpoint_ready(host: &str, port: &str, timeout_sec: u64) -> bool {
    for _ in 0..(timeout_sec * 2) {
        if tcp_endpoint_ready(host, port).await {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    false
}

fn render_service_state(label: &str, state: &VoiceCommandServiceState) -> String {
    match state {
        VoiceCommandServiceState::Running(session) => format!("  {label}: RUNNING ({session})"),
        VoiceCommandServiceState::Stopped(session) => format!("  {label}: STOPPED ({session})"),
        VoiceCommandServiceState::Disabled(session) => format!("  {label}: DISABLED ({session})"),
        VoiceCommandServiceState::NotApplicable(reason) => format!("  {label}: N/A ({reason})"),
    }
}

fn render_endpoint_state(state: &VoiceCommandEndpointState) -> String {
    match state {
        VoiceCommandEndpointState::Ready(label, target) => format!("  ready: {label} ({target})"),
        VoiceCommandEndpointState::NotReady(label, target) => {
            format!("  ready: {label} ({target})")
        }
        VoiceCommandEndpointState::Disabled(reason) => format!("  disabled: yes ({reason})"),
        VoiceCommandEndpointState::NotApplicable(reason) => format!("  N/A ({reason})"),
    }
}

fn render_voice_command_status(snapshot: &VoiceCommandStatusSnapshot) -> String {
    let mut lines = vec![format!("stt_backend={}", snapshot.stt_backend)];
    if snapshot.stt_backend == "moonshine" {
        lines.push(format!(
            "moonshine_model_size={}",
            snapshot.moonshine_model_size
        ));
    } else {
        lines.push(format!("server_session={}", snapshot.server_session));
        lines.push(format!("server_url={}", snapshot.server_url));
        lines.push(format!("model={}", snapshot.server_model));
        lines.push(format!("language={}", snapshot.language));
    }
    lines.push(format!("listener_session={}", snapshot.listener_session));
    lines.push(format!("agent_session={}", snapshot.agent_session));
    lines.push(format!("overlay_session={}", snapshot.overlay_session));
    lines.push(format!(
        "lock_screen_session={}",
        snapshot.lock_screen_session
    ));
    lines.push(format!("overlay_ipc={}", snapshot.overlay_ipc));
    lines.push(format!("lock_screen_ipc={}", snapshot.lock_screen_ipc));
    lines.push(format!("listener_script={}", snapshot.listener_script));
    lines.push(format!("agent_script={}", snapshot.agent_script));
    lines.push(format!("mic_source={}", snapshot.mic_source));
    if let Some(source) = &snapshot.active_listener_source {
        lines.push(format!("active_listener_source={source}"));
    }
    if let Some(source) = &snapshot.active_agent_source {
        lines.push(format!("active_agent_source={source}"));
    }

    lines.push(String::new());
    lines.push("[tmux sessions]".to_string());
    lines.push(render_service_state(
        "server",
        &snapshot.server_session_state,
    ));
    lines.push(render_service_state(
        "listener",
        &snapshot.listener_session_state,
    ));
    lines.push(render_service_state("agent", &snapshot.agent_session_state));
    if snapshot.contention_warning {
        lines.push("  warning: listener+agent microphone contention likely".to_string());
    }
    lines.push(render_service_state(
        "overlay",
        &snapshot.overlay_session_state,
    ));
    lines.push(render_service_state(
        "lock-screen",
        &snapshot.lock_screen_session_state,
    ));

    lines.push(String::new());
    lines.push("[server endpoint]".to_string());
    lines.push(render_endpoint_state(&snapshot.server_endpoint));
    lines.push(String::new());
    lines.push("[overlay endpoint]".to_string());
    lines.push(render_endpoint_state(&snapshot.overlay_endpoint));
    lines.push(String::new());
    lines.push("[lock screen endpoint]".to_string());
    lines.push(render_endpoint_state(&snapshot.lock_screen_endpoint));
    lines.push(String::new());
    lines.join("\n")
}

async fn collect_voice_command_status(
    runtime: &crate::voice_command::VoiceCommandOperatorRuntimeConfig,
) -> VoiceCommandStatusSnapshot {
    let listener_running = tmux_has_session(&runtime.listener_session).await;
    let agent_running = tmux_has_session(&runtime.agent_session).await;
    let overlay_running = tmux_has_session(&runtime.overlay_session).await;
    let lock_screen_running = if runtime.lock_screen_port == "0" {
        false
    } else {
        tmux_has_session(&runtime.lock_screen_session).await
    };
    let server_running = if runtime.stt_backend == "moonshine" {
        false
    } else {
        tmux_has_session(&runtime.server_session).await
    };
    let server_ready = if runtime.stt_backend == "moonshine" {
        false
    } else {
        http_endpoint_ready(&runtime.server_url).await
    };
    let overlay_ready = if runtime.whisper_agent_no_overlay {
        false
    } else {
        tcp_endpoint_ready(&runtime.overlay_host, &runtime.overlay_port).await
    };
    let lock_screen_ready = if runtime.whisper_agent_no_overlay || runtime.lock_screen_port == "0" {
        false
    } else {
        tcp_endpoint_ready(&runtime.overlay_host, &runtime.lock_screen_port).await
    };

    VoiceCommandStatusSnapshot {
        stt_backend: runtime.stt_backend.clone(),
        moonshine_model_size: runtime.moonshine_model_size.clone(),
        server_session: runtime.server_session.clone(),
        server_url: runtime.server_url.clone(),
        server_model: runtime.server_model.display().to_string(),
        language: runtime.whisper_language.clone(),
        listener_session: runtime.listener_session.clone(),
        agent_session: runtime.agent_session.clone(),
        overlay_session: runtime.overlay_session.clone(),
        lock_screen_session: runtime.lock_screen_session.clone(),
        overlay_ipc: format!("{}:{}", runtime.overlay_host, runtime.overlay_port),
        lock_screen_ipc: format!("{}:{}", runtime.overlay_host, runtime.lock_screen_port),
        listener_script: runtime.listener_script_path.display().to_string(),
        agent_script: runtime.agent_script_path.display().to_string(),
        mic_source: runtime
            .whisper_mic_source
            .clone()
            .unwrap_or_else(|| "<auto>".to_string()),
        active_listener_source: active_source_for_session(&runtime.listener_session).await,
        active_agent_source: active_source_for_session(&runtime.agent_session).await,
        server_session_state: if runtime.stt_backend == "moonshine" {
            VoiceCommandServiceState::NotApplicable(
                "moonshine backend — no whisper-server needed".to_string(),
            )
        } else if server_running {
            VoiceCommandServiceState::Running(runtime.server_session.clone())
        } else {
            VoiceCommandServiceState::Stopped(runtime.server_session.clone())
        },
        listener_session_state: if listener_running {
            VoiceCommandServiceState::Running(runtime.listener_session.clone())
        } else {
            VoiceCommandServiceState::Stopped(runtime.listener_session.clone())
        },
        agent_session_state: if agent_running {
            VoiceCommandServiceState::Running(runtime.agent_session.clone())
        } else {
            VoiceCommandServiceState::Stopped(runtime.agent_session.clone())
        },
        overlay_session_state: if overlay_running {
            VoiceCommandServiceState::Running(runtime.overlay_session.clone())
        } else {
            VoiceCommandServiceState::Stopped(runtime.overlay_session.clone())
        },
        lock_screen_session_state: if runtime.lock_screen_port == "0" {
            VoiceCommandServiceState::Disabled(runtime.lock_screen_session.clone())
        } else if lock_screen_running {
            VoiceCommandServiceState::Running(runtime.lock_screen_session.clone())
        } else {
            VoiceCommandServiceState::Stopped(runtime.lock_screen_session.clone())
        },
        server_endpoint: if runtime.stt_backend == "moonshine" {
            VoiceCommandEndpointState::NotApplicable("moonshine backend".to_string())
        } else if server_ready {
            VoiceCommandEndpointState::Ready("yes".to_string(), format!("{}/", runtime.server_url))
        } else {
            VoiceCommandEndpointState::NotReady(
                "no".to_string(),
                format!("{}/", runtime.server_url),
            )
        },
        overlay_endpoint: if runtime.whisper_agent_no_overlay {
            VoiceCommandEndpointState::Disabled("WHISPER_AGENT_NO_OVERLAY=1".to_string())
        } else if overlay_ready {
            VoiceCommandEndpointState::Ready(
                "yes".to_string(),
                format!("{}:{}", runtime.overlay_host, runtime.overlay_port),
            )
        } else {
            VoiceCommandEndpointState::NotReady(
                "no".to_string(),
                format!("{}:{}", runtime.overlay_host, runtime.overlay_port),
            )
        },
        lock_screen_endpoint: if runtime.whisper_agent_no_overlay {
            VoiceCommandEndpointState::Disabled("WHISPER_AGENT_NO_OVERLAY=1".to_string())
        } else if runtime.lock_screen_port == "0" {
            VoiceCommandEndpointState::Disabled("WHISPER_AGENT_LOCK_SCREEN_IPC_PORT=0".to_string())
        } else if lock_screen_ready {
            VoiceCommandEndpointState::Ready(
                "yes".to_string(),
                format!("{}:{}", runtime.overlay_host, runtime.lock_screen_port),
            )
        } else {
            VoiceCommandEndpointState::NotReady(
                "no".to_string(),
                format!("{}:{}", runtime.overlay_host, runtime.lock_screen_port),
            )
        },
        contention_warning: listener_running && agent_running,
    }
}

async fn print_voice_command_status(
    runtime: &crate::voice_command::VoiceCommandOperatorRuntimeConfig,
) {
    let snapshot = collect_voice_command_status(runtime).await;
    print!("{}", render_voice_command_status(&snapshot));
}

fn build_caption_overlay_start_command(
    runtime: &crate::voice_command::VoiceCommandOperatorRuntimeConfig,
) -> String {
    let mut env_prefix = String::new();
    if let Some(display) = &runtime.overlay_display {
        env_prefix.push_str(&format!(
            "export DISPLAY={}; export CAPTION_OVERLAY_DISPLAY={}; ",
            shell_single_quote_str(display),
            shell_single_quote_str(display),
        ));
    }
    env_prefix.push_str("export XDG_RUNTIME_DIR=${XDG_RUNTIME_DIR:-/run/user/$(id -u)}; ");
    if let Some(xauthority) = &runtime.overlay_xauthority {
        env_prefix.push_str(&format!(
            "export XAUTHORITY={}; export CAPTION_OVERLAY_XAUTHORITY={}; ",
            shell_single_quote_str(xauthority),
            shell_single_quote_str(xauthority),
        ));
    }
    env_prefix.push_str(&format!(
        "export CAPTION_OVERLAY_IPC_HOST={}; export CAPTION_OVERLAY_IPC_PORT={}; ",
        shell_single_quote_str(&runtime.overlay_host),
        shell_single_quote_str(&runtime.overlay_port),
    ));

    format!(
        "cd {} && {}exec npm run start",
        shell_single_quote(&runtime.overlay_root),
        env_prefix
    )
}

fn build_lock_screen_start_command(
    runtime: &crate::voice_command::VoiceCommandOperatorRuntimeConfig,
) -> String {
    let mut env_prefix = String::new();
    env_prefix.push_str("export XDG_RUNTIME_DIR=${XDG_RUNTIME_DIR:-/run/user/$(id -u)}; ");
    if let Some(display) = &runtime.lock_screen_display {
        env_prefix.push_str(&format!(
            "export DISPLAY={}; export ASEC_DISPLAY={}; ",
            shell_single_quote_str(display),
            shell_single_quote_str(display),
        ));
    }
    if let Some(xauthority) = &runtime.lock_screen_xauthority {
        env_prefix.push_str(&format!(
            "export XAUTHORITY={}; export ASEC_XAUTHORITY={}; ",
            shell_single_quote_str(xauthority),
            shell_single_quote_str(xauthority),
        ));
    }
    env_prefix.push_str(&format!(
        "export ASEC_IPC_PORT={}; ",
        shell_single_quote_str(&runtime.lock_screen_port),
    ));
    if let Some(path) = &runtime.biometric_password_file {
        env_prefix.push_str(&format!(
            "export ASEC_BIOMETRIC_PASSWORD_FILE={}; ",
            shell_single_quote(path),
        ));
    }
    if let Some(path) = &runtime.biometric_password_private_key {
        env_prefix.push_str(&format!(
            "export ASEC_BIOMETRIC_PASSWORD_PRIVATE_KEY={}; ",
            shell_single_quote(path),
        ));
    }
    if let Some(path) = &runtime.biometric_unlock_signal_file {
        env_prefix.push_str(&format!(
            "export ASEC_BIOMETRIC_UNLOCK_SIGNAL_FILE={}; ",
            shell_single_quote(path),
        ));
    }

    format!(
        "cd {} && {}exec npm run start:bridge -- --tcp-port {}",
        shell_single_quote(&runtime.lock_screen_root),
        env_prefix,
        runtime.lock_screen_port
    )
}

async fn start_tmux_session(
    session: &str,
    command: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let status = Command::new("tmux")
        .arg("new-session")
        .arg("-d")
        .arg("-s")
        .arg(session)
        .arg(format!("bash -lc {}", shell_single_quote_str(command)))
        .status()
        .await?;
    if !status.success() {
        return Err(format!("failed to start tmux session: {session}").into());
    }
    Ok(())
}

fn require_command(cmd: &str) -> Result<(), Box<dyn std::error::Error>> {
    if is_command_in_path(cmd) {
        return Ok(());
    }
    Err(format!("required command not found: {cmd}").into())
}

fn env_var_nonempty(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|value| !value.is_empty())
}

fn env_flag_enabled(key: &str, default: bool) -> bool {
    std::env::var(key)
        .ok()
        .map(|value| value == "1")
        .unwrap_or(default)
}

fn push_optional_arg(args: &mut Vec<String>, flag: &str, value: Option<String>) {
    if let Some(value) = value {
        args.push(flag.to_string());
        args.push(value);
    }
}

fn shell_join_args(args: &[String]) -> String {
    args.iter()
        .map(|arg| shell_single_quote_str(arg))
        .collect::<Vec<_>>()
        .join(" ")
}

fn build_server_command_args(
    runtime: &crate::voice_command::VoiceCommandOperatorRuntimeConfig,
) -> Vec<String> {
    vec![
        runtime.server_bin.display().to_string(),
        "--host".to_string(),
        runtime.server_host.clone(),
        "--port".to_string(),
        runtime.server_port.clone(),
        "-m".to_string(),
        runtime.server_model.display().to_string(),
        "-l".to_string(),
        runtime.whisper_language.clone(),
        "-nt".to_string(),
        "-ng".to_string(),
    ]
}

fn build_listener_command_args(
    runtime: &crate::voice_command::VoiceCommandOperatorRuntimeConfig,
) -> Vec<String> {
    let mut args = vec![
        "python3".to_string(),
        runtime.listener_script_path.display().to_string(),
        "--tmp-dir".to_string(),
        runtime.whisper_listen_tmp_dir.display().to_string(),
    ];
    if runtime.stt_backend == "moonshine" {
        args.push("--model-size".to_string());
        args.push(runtime.moonshine_model_size.clone());
    } else {
        args.push("--server".to_string());
        args.push(runtime.server_url.clone());
        args.push("--language".to_string());
        args.push(runtime.whisper_language.clone());
        push_optional_arg(
            &mut args,
            "--stt-prompt",
            env_var_nonempty("WHISPER_LISTEN_STT_PROMPT"),
        );
    }
    push_optional_arg(&mut args, "--source", runtime.whisper_mic_source.clone());
    if env_flag_enabled("WHISPER_LISTEN_DEBUG", false) {
        args.push("--debug".to_string());
    }
    push_optional_arg(
        &mut args,
        "--max-run-sec",
        env_var_nonempty("WHISPER_LISTEN_MAX_RUN_SEC"),
    );
    push_optional_arg(
        &mut args,
        "--max-segments",
        env_var_nonempty("WHISPER_LISTEN_MAX_SEGMENTS"),
    );
    args
}

fn build_agent_command_args(
    runtime: &crate::voice_command::VoiceCommandOperatorRuntimeConfig,
) -> Vec<String> {
    let mut args = vec![
        "python3".to_string(),
        runtime.agent_script_path.display().to_string(),
        "--tmp-dir".to_string(),
        runtime.whisper_listen_tmp_dir.display().to_string(),
    ];
    if runtime.stt_backend == "moonshine" {
        args.push("--stt-backend".to_string());
        args.push("moonshine".to_string());
        args.push("--model-size".to_string());
        args.push(runtime.moonshine_model_size.clone());
    } else {
        args.push("--stt-backend".to_string());
        args.push("whisper".to_string());
        args.push("--server".to_string());
        args.push(runtime.server_url.clone());
        args.push("--language".to_string());
        args.push(runtime.whisper_language.clone());
        push_optional_arg(
            &mut args,
            "--stt-prompt",
            env_var_nonempty("WHISPER_AGENT_STT_PROMPT"),
        );
    }
    push_optional_arg(&mut args, "--source", runtime.whisper_mic_source.clone());
    if env_flag_enabled("WHISPER_AGENT_DEBUG", false) {
        args.push("--debug".to_string());
    }
    push_optional_arg(
        &mut args,
        "--max-run-sec",
        env_var_nonempty("WHISPER_AGENT_MAX_RUN_SEC"),
    );
    push_optional_arg(
        &mut args,
        "--max-segments",
        env_var_nonempty("WHISPER_AGENT_MAX_SEGMENTS"),
    );
    push_optional_arg(
        &mut args,
        "--calibration-ms",
        env_var_nonempty("WHISPER_AGENT_CALIBRATION_MS"),
    );
    args.push("--pre-roll-ms".to_string());
    args.push(env_var_nonempty("WHISPER_AGENT_PRE_ROLL_MS").unwrap_or_else(|| "500".to_string()));
    push_optional_arg(
        &mut args,
        "--start-rms",
        env_var_nonempty("WHISPER_AGENT_START_RMS"),
    );
    push_optional_arg(
        &mut args,
        "--stop-rms",
        env_var_nonempty("WHISPER_AGENT_STOP_RMS"),
    );
    push_optional_arg(
        &mut args,
        "--start-rms-min",
        env_var_nonempty("WHISPER_AGENT_START_RMS_MIN"),
    );
    push_optional_arg(
        &mut args,
        "--start-rms-max",
        env_var_nonempty("WHISPER_AGENT_START_RMS_MAX"),
    );
    push_optional_arg(
        &mut args,
        "--stop-rms-min",
        env_var_nonempty("WHISPER_AGENT_STOP_RMS_MIN"),
    );
    push_optional_arg(
        &mut args,
        "--stop-rms-max",
        env_var_nonempty("WHISPER_AGENT_STOP_RMS_MAX"),
    );
    if env_flag_enabled("WHISPER_AGENT_NO_VOICE", false) {
        args.push("--no-voice".to_string());
    }
    if env_flag_enabled("WHISPER_AGENT_WAIT_ACK_AFTER_ACTION", false) {
        args.push("--wait-ack-after-action".to_string());
    }
    push_optional_arg(
        &mut args,
        "--audio-sink",
        env_var_nonempty("WHISPER_AGENT_AUDIO_SINK"),
    );
    if env_flag_enabled("WHISPER_AGENT_NOTIFY_PROGRESS", false) {
        args.push("--notify-progress".to_string());
    }
    if runtime.whisper_agent_no_overlay {
        args.push("--no-overlay".to_string());
    } else {
        args.push("--overlay-ipc-host".to_string());
        args.push(runtime.overlay_host.clone());
        args.push("--overlay-ipc-port".to_string());
        args.push(runtime.overlay_port.clone());
        if runtime.lock_screen_port != "0" {
            args.push("--lock-screen-ipc-port".to_string());
            args.push(runtime.lock_screen_port.clone());
        }
    }
    if env_flag_enabled("WHISPER_AGENT_SPEAKER_ID", true) {
        args.push("--speaker-id".to_string());
        args.push("--speaker-master".to_string());
        args.push(
            env_var_nonempty("WHISPER_AGENT_SPEAKER_MASTER").unwrap_or_else(|| {
                runtime
                    .workspaces_root
                    .join("repos/ahear/python/src/ahear/models/master_voiceprint.npy")
                    .display()
                    .to_string()
            }),
        );
        args.push("--speaker-threshold".to_string());
        args.push(
            env_var_nonempty("WHISPER_AGENT_SPEAKER_THRESHOLD")
                .unwrap_or_else(|| "0.60".to_string()),
        );
        args.push("--speaker-topk".to_string());
        args.push(
            env_var_nonempty("WHISPER_AGENT_SPEAKER_TOPK").unwrap_or_else(|| "5".to_string()),
        );
        args.push("--speaker-device".to_string());
        args.push(
            env_var_nonempty("WHISPER_AGENT_SPEAKER_DEVICE").unwrap_or_else(|| "cpu".to_string()),
        );
    }
    if env_flag_enabled("WHISPER_AGENT_BIOMETRIC_LOCK", true) {
        args.push("--biometric-lock".to_string());
    }
    if env_flag_enabled("WHISPER_AGENT_BIOMETRIC_START_LOCKED", false) {
        args.push("--biometric-start-locked".to_string());
    }
    push_optional_arg(
        &mut args,
        "--biometric-command-idle-lock-sec",
        env_var_nonempty("WHISPER_AGENT_BIOMETRIC_COMMAND_IDLE_LOCK_SEC"),
    );
    push_optional_arg(
        &mut args,
        "--biometric-face-absent-lock-sec",
        env_var_nonempty("WHISPER_AGENT_BIOMETRIC_FACE_ABSENT_LOCK_SEC"),
    );
    push_optional_arg(
        &mut args,
        "--biometric-unlock-face-fresh-ms",
        env_var_nonempty("WHISPER_AGENT_BIOMETRIC_UNLOCK_FACE_FRESH_MS"),
    );
    push_optional_arg(
        &mut args,
        "--biometric-poll-sec",
        env_var_nonempty("WHISPER_AGENT_BIOMETRIC_POLL_SEC"),
    );
    push_optional_arg(
        &mut args,
        "--god-mode-status-url",
        env_var_nonempty("WHISPER_AGENT_GOD_MODE_STATUS_URL"),
    );
    push_optional_arg(
        &mut args,
        "--biometric-password-file",
        runtime
            .biometric_password_file
            .as_ref()
            .map(|path| path.display().to_string()),
    );
    push_optional_arg(
        &mut args,
        "--biometric-password-public-key",
        std::env::var_os("WHISPER_AGENT_BIOMETRIC_PASSWORD_PUBLIC_KEY")
            .filter(|value| !value.is_empty())
            .map(|value| PathBuf::from(value).display().to_string()),
    );
    push_optional_arg(
        &mut args,
        "--biometric-password-private-key",
        runtime
            .biometric_password_private_key
            .as_ref()
            .map(|path| path.display().to_string()),
    );
    push_optional_arg(
        &mut args,
        "--biometric-lock-signal-file",
        std::env::var_os("WHISPER_AGENT_BIOMETRIC_LOCK_SIGNAL_FILE")
            .filter(|value| !value.is_empty())
            .map(|value| PathBuf::from(value).display().to_string()),
    );
    push_optional_arg(
        &mut args,
        "--biometric-unlock-signal-file",
        runtime
            .biometric_unlock_signal_file
            .as_ref()
            .map(|path| path.display().to_string()),
    );
    args
}

async fn wait_http_endpoint_ready(url: &str, timeout_sec: u64) -> bool {
    for _ in 0..(timeout_sec * 2) {
        if http_endpoint_ready(url).await {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    false
}

async fn stop_listener_runtime(
    runtime: &crate::voice_command::VoiceCommandOperatorRuntimeConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    tmux_kill_session_if_exists(
        &runtime.listener_session,
        &format!("listener stopped: {}", runtime.listener_session),
        &format!("listener session not running: {}", runtime.listener_session),
    )
    .await
}

async fn stop_agent_runtime(
    runtime: &crate::voice_command::VoiceCommandOperatorRuntimeConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    tmux_kill_session_if_exists(
        &runtime.agent_session,
        &format!("agent stopped: {}", runtime.agent_session),
        &format!("agent session not running: {}", runtime.agent_session),
    )
    .await
}

async fn stop_server_runtime(
    runtime: &crate::voice_command::VoiceCommandOperatorRuntimeConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    tmux_kill_session_if_exists(
        &runtime.server_session,
        &format!("server stopped: {}", runtime.server_session),
        &format!("server session not running: {}", runtime.server_session),
    )
    .await
}

async fn start_server_runtime(
    runtime: &crate::voice_command::VoiceCommandOperatorRuntimeConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    if runtime.stt_backend == "moonshine" {
        println!("STT_BACKEND=moonshine: skipping whisper-server (not needed)");
        return Ok(());
    }

    if !runtime.server_bin.is_file() {
        return Err(format!(
            "whisper-server binary not executable: {}",
            runtime.server_bin.display()
        )
        .into());
    }
    if !runtime.server_model.is_file() {
        return Err(format!("model file not found: {}", runtime.server_model.display()).into());
    }

    if tmux_has_session(&runtime.server_session).await {
        println!(
            "server tmux session already exists: {}",
            runtime.server_session
        );
        return Ok(());
    }

    if http_endpoint_ready(&runtime.server_url).await {
        println!(
            "server endpoint already ready at {} (outside managed tmux session?)",
            runtime.server_url
        );
        return Ok(());
    }

    let command = format!(
        "exec {}",
        shell_join_args(&build_server_command_args(runtime))
    );
    println!(
        "starting whisper-server in tmux session {}",
        runtime.server_session
    );
    start_tmux_session(&runtime.server_session, &command).await?;

    if !wait_http_endpoint_ready(&runtime.server_url, 30).await {
        return Err(format!(
            "whisper-server failed to become ready at {}",
            runtime.server_url
        )
        .into());
    }
    println!("whisper-server ready: {}", runtime.server_url);
    Ok(())
}

async fn start_listener_runtime(
    runtime: &crate::voice_command::VoiceCommandOperatorRuntimeConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    if !runtime.listener_script_path.is_file() {
        return Err(format!(
            "listener script not found: {}",
            runtime.listener_script_path.display()
        )
        .into());
    }
    require_command("python3")?;
    require_command("tmux")?;
    require_command("pactl")?;
    if runtime.stt_backend == "moonshine" {
        require_command("ffmpeg")?;
    } else {
        require_command("curl")?;
        require_command("parec")?;
    }

    if tmux_has_session(&runtime.listener_session).await {
        println!(
            "listener tmux session already exists: {}",
            runtime.listener_session
        );
        return Ok(());
    }

    if tmux_has_session(&runtime.agent_session).await {
        println!(
            "stopping agent session to avoid microphone contention: {}",
            runtime.agent_session
        );
        stop_agent_runtime(runtime).await?;
    }

    if runtime.stt_backend != "moonshine" && !http_endpoint_ready(&runtime.server_url).await {
        return Err(format!("whisper-server is not ready at {}", runtime.server_url).into());
    }

    std::fs::create_dir_all(&runtime.whisper_listen_tmp_dir)?;
    let listener_cmd = shell_join_args(&build_listener_command_args(runtime));
    let command = format!(
        "export XDG_RUNTIME_DIR=${{XDG_RUNTIME_DIR:-/run/user/$(id -u)}}; exec {listener_cmd}"
    );

    println!(
        "starting listener in tmux session {}",
        runtime.listener_session
    );
    start_tmux_session(&runtime.listener_session, &command).await?;
    tokio::time::sleep(std::time::Duration::from_millis(800)).await;

    if tmux_has_session(&runtime.listener_session).await {
        println!("listener started");
        Ok(())
    } else {
        Err("listener tmux session exited immediately".into())
    }
}

async fn start_agent_runtime(
    runtime: &crate::voice_command::VoiceCommandOperatorRuntimeConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    if !runtime.agent_script_path.is_file() {
        return Err(format!(
            "agent script not found: {}",
            runtime.agent_script_path.display()
        )
        .into());
    }
    require_command("python3")?;
    require_command("tmux")?;
    require_command("pactl")?;
    require_command("xdotool")?;
    require_command("wmctrl")?;
    if runtime.stt_backend == "moonshine" {
        require_command("ffmpeg")?;
    } else {
        require_command("curl")?;
        require_command("parec")?;
    }

    if tmux_has_session(&runtime.agent_session).await {
        println!(
            "agent tmux session already exists: {}",
            runtime.agent_session
        );
        return Ok(());
    }

    if tmux_has_session(&runtime.listener_session).await {
        println!(
            "stopping listener session to avoid microphone contention: {}",
            runtime.listener_session
        );
        stop_listener_runtime(runtime).await?;
    }

    if runtime.stt_backend != "moonshine" && !http_endpoint_ready(&runtime.server_url).await {
        return Err(format!("whisper-server is not ready at {}", runtime.server_url).into());
    }

    start_overlay_runtime(runtime).await?;
    std::fs::create_dir_all(&runtime.whisper_listen_tmp_dir)?;
    let agent_cmd = shell_join_args(&build_agent_command_args(runtime));
    let command = format!(
        "export XDG_RUNTIME_DIR=${{XDG_RUNTIME_DIR:-/run/user/$(id -u)}}; exec {agent_cmd}"
    );

    println!(
        "starting voice command agent in tmux session {}",
        runtime.agent_session
    );
    start_tmux_session(&runtime.agent_session, &command).await?;
    tokio::time::sleep(std::time::Duration::from_millis(800)).await;

    if tmux_has_session(&runtime.agent_session).await {
        println!("agent started");
        Ok(())
    } else {
        Err("agent tmux session exited immediately".into())
    }
}

async fn stop_legacy_overlay_sessions(
    runtime: &crate::voice_command::VoiceCommandOperatorRuntimeConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    for session in &runtime.legacy_overlay_sessions {
        if tmux_has_session(session).await {
            let status = Command::new("tmux")
                .arg("kill-session")
                .arg("-t")
                .arg(session)
                .status()
                .await?;
            if !status.success() {
                return Err(format!("failed to kill legacy overlay session: {session}").into());
            }
            println!("legacy overlay session stopped: {session}");
        }
    }
    Ok(())
}

async fn stop_overlay_runtime(
    runtime: &crate::voice_command::VoiceCommandOperatorRuntimeConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    tmux_kill_session_if_exists(
        &runtime.overlay_session,
        &format!("overlay stopped: {}", runtime.overlay_session),
        &format!("overlay session not running: {}", runtime.overlay_session),
    )
    .await?;
    if runtime.lock_screen_port != "0" {
        tmux_kill_session_if_exists(
            &runtime.lock_screen_session,
            &format!("lock screen stopped: {}", runtime.lock_screen_session),
            &format!(
                "lock screen session not running: {}",
                runtime.lock_screen_session
            ),
        )
        .await?;
    }
    stop_legacy_overlay_sessions(runtime).await
}

async fn start_caption_overlay_runtime(
    runtime: &crate::voice_command::VoiceCommandOperatorRuntimeConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    if tmux_has_session(&runtime.overlay_session).await {
        if tcp_endpoint_ready(&runtime.overlay_host, &runtime.overlay_port).await {
            println!(
                "overlay tmux session already exists and IPC is ready: {}",
                runtime.overlay_session
            );
            return Ok(());
        }
        println!(
            "overlay session exists but IPC is still not ready; restarting: {}",
            runtime.overlay_session
        );
        tmux_kill_session_if_exists(
            &runtime.overlay_session,
            &format!("overlay stopped: {}", runtime.overlay_session),
            &format!("overlay session not running: {}", runtime.overlay_session),
        )
        .await?;
    }

    if tcp_endpoint_ready(&runtime.overlay_host, &runtime.overlay_port).await {
        println!(
            "overlay IPC already ready at {}:{} (outside managed tmux session?)",
            runtime.overlay_host, runtime.overlay_port
        );
        return Ok(());
    }

    if !runtime.overlay_root.is_dir() {
        return Err(format!(
            "caption overlay root not found: {}",
            runtime.overlay_root.display()
        )
        .into());
    }

    let command = build_caption_overlay_start_command(runtime);
    println!(
        "starting acaption in tmux session {} (ipc={}:{})",
        runtime.overlay_session, runtime.overlay_host, runtime.overlay_port
    );
    start_tmux_session(&runtime.overlay_session, &command).await?;

    if !wait_tcp_endpoint_ready(&runtime.overlay_host, &runtime.overlay_port, 45).await {
        return Err(format!(
            "acaption failed to become ready at {}:{}",
            runtime.overlay_host, runtime.overlay_port
        )
        .into());
    }
    println!(
        "acaption ready: {}:{}",
        runtime.overlay_host, runtime.overlay_port
    );
    Ok(())
}

async fn start_lock_screen_overlay_runtime(
    runtime: &crate::voice_command::VoiceCommandOperatorRuntimeConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    if runtime.lock_screen_port == "0" {
        println!("lock screen start skipped (WHISPER_AGENT_LOCK_SCREEN_IPC_PORT=0)");
        return Ok(());
    }

    if tmux_has_session(&runtime.lock_screen_session).await {
        if tcp_endpoint_ready(&runtime.overlay_host, &runtime.lock_screen_port).await {
            println!(
                "lock screen tmux session already exists and IPC is ready: {}",
                runtime.lock_screen_session
            );
            return Ok(());
        }
        println!(
            "lock screen session exists but IPC is still not ready; restarting: {}",
            runtime.lock_screen_session
        );
        tmux_kill_session_if_exists(
            &runtime.lock_screen_session,
            &format!("lock screen stopped: {}", runtime.lock_screen_session),
            &format!(
                "lock screen session not running: {}",
                runtime.lock_screen_session
            ),
        )
        .await?;
    }

    if tcp_endpoint_ready(&runtime.overlay_host, &runtime.lock_screen_port).await {
        println!(
            "lock screen IPC already ready at {}:{} (outside managed tmux session?)",
            runtime.overlay_host, runtime.lock_screen_port
        );
        return Ok(());
    }

    if !runtime.lock_screen_root.is_dir() {
        return Err(format!(
            "lock screen root not found: {}",
            runtime.lock_screen_root.display()
        )
        .into());
    }

    let command = build_lock_screen_start_command(runtime);
    println!(
        "starting asec in tmux session {} (ipc={}:{})",
        runtime.lock_screen_session, runtime.overlay_host, runtime.lock_screen_port
    );
    start_tmux_session(&runtime.lock_screen_session, &command).await?;

    if !wait_tcp_endpoint_ready(&runtime.overlay_host, &runtime.lock_screen_port, 45).await {
        return Err(format!(
            "asec failed to become ready at {}:{}",
            runtime.overlay_host, runtime.lock_screen_port
        )
        .into());
    }
    println!(
        "asec ready: {}:{}",
        runtime.overlay_host, runtime.lock_screen_port
    );
    Ok(())
}

async fn start_overlay_runtime(
    runtime: &crate::voice_command::VoiceCommandOperatorRuntimeConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    if runtime.whisper_agent_no_overlay {
        println!("overlay start skipped (WHISPER_AGENT_NO_OVERLAY=1)");
        return Ok(());
    }
    require_command("npm")?;
    stop_legacy_overlay_sessions(runtime).await?;
    start_caption_overlay_runtime(runtime).await?;
    start_lock_screen_overlay_runtime(runtime).await
}

fn voice_command_operator_log_session<'a>(
    action: &VoiceCommandOperatorAction,
    runtime: &'a crate::voice_command::VoiceCommandOperatorRuntimeConfig,
) -> Option<&'a str> {
    match action {
        VoiceCommandOperatorAction::LogsServer => Some(runtime.server_session.as_str()),
        VoiceCommandOperatorAction::LogsListener => Some(runtime.listener_session.as_str()),
        VoiceCommandOperatorAction::LogsAgent => Some(runtime.agent_session.as_str()),
        VoiceCommandOperatorAction::LogsAgentTail => Some(runtime.agent_session.as_str()),
        VoiceCommandOperatorAction::LogsOverlay => Some(runtime.overlay_session.as_str()),
        VoiceCommandOperatorAction::LogsLockScreen => Some(runtime.lock_screen_session.as_str()),
        _ => None,
    }
}

fn voice_command_operator_attach_session<'a>(
    action: &VoiceCommandOperatorAction,
    runtime: &'a crate::voice_command::VoiceCommandOperatorRuntimeConfig,
) -> Option<&'a str> {
    match action {
        VoiceCommandOperatorAction::AttachServer => Some(runtime.server_session.as_str()),
        VoiceCommandOperatorAction::AttachListener => Some(runtime.listener_session.as_str()),
        VoiceCommandOperatorAction::AttachAgent => Some(runtime.agent_session.as_str()),
        VoiceCommandOperatorAction::AttachOverlay => Some(runtime.overlay_session.as_str()),
        VoiceCommandOperatorAction::AttachLockScreen => Some(runtime.lock_screen_session.as_str()),
        _ => None,
    }
}

async fn show_tmux_logs(session: &str) -> Result<(), Box<dyn std::error::Error>> {
    if !tmux_has_session(session).await {
        return Err(format!("tmux session not found: {session}").into());
    }

    let status = Command::new("tmux")
        .arg("capture-pane")
        .arg("-pt")
        .arg(format!("{session}:0"))
        .arg("-S")
        .arg("-120")
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await?;
    if !status.success() {
        return Err(format!("failed to capture tmux logs for session: {session}").into());
    }
    Ok(())
}

async fn attach_tmux_session(session: &str) -> Result<(), Box<dyn std::error::Error>> {
    if !tmux_has_session(session).await {
        return Err(format!("tmux session not found: {session}").into());
    }

    let status = Command::new("tmux")
        .arg("attach-session")
        .arg("-t")
        .arg(session)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await?;
    if !status.success() {
        return Err(format!("failed to attach tmux session: {session}").into());
    }
    Ok(())
}

async fn run_direct_voice_command_operator_action(
    action: &VoiceCommandOperatorAction,
) -> Result<bool, Box<dyn std::error::Error>> {
    let runtime = resolve_voice_command_operator_runtime_config();
    match action {
        VoiceCommandOperatorAction::Start => {
            start_server_runtime(&runtime).await?;
            start_listener_runtime(&runtime).await?;
            print_voice_command_status(&runtime).await;
            Ok(true)
        }
        VoiceCommandOperatorAction::Restart => {
            stop_listener_runtime(&runtime).await?;
            start_listener_runtime(&runtime).await?;
            print_voice_command_status(&runtime).await;
            Ok(true)
        }
        VoiceCommandOperatorAction::RestartAll => {
            stop_listener_runtime(&runtime).await?;
            stop_server_runtime(&runtime).await?;
            start_server_runtime(&runtime).await?;
            start_listener_runtime(&runtime).await?;
            print_voice_command_status(&runtime).await;
            Ok(true)
        }
        VoiceCommandOperatorAction::Status => {
            print_voice_command_status(&runtime).await;
            Ok(true)
        }
        VoiceCommandOperatorAction::StartAgent => {
            start_server_runtime(&runtime).await?;
            start_agent_runtime(&runtime).await?;
            print_voice_command_status(&runtime).await;
            Ok(true)
        }
        VoiceCommandOperatorAction::RestartAgent => {
            stop_agent_runtime(&runtime).await?;
            start_agent_runtime(&runtime).await?;
            print_voice_command_status(&runtime).await;
            Ok(true)
        }
        VoiceCommandOperatorAction::RestartAgentAll => {
            stop_agent_runtime(&runtime).await?;
            stop_overlay_runtime(&runtime).await?;
            stop_server_runtime(&runtime).await?;
            start_server_runtime(&runtime).await?;
            start_agent_runtime(&runtime).await?;
            print_voice_command_status(&runtime).await;
            Ok(true)
        }
        VoiceCommandOperatorAction::StartOverlay => {
            start_overlay_runtime(&runtime).await?;
            print_voice_command_status(&runtime).await;
            Ok(true)
        }
        VoiceCommandOperatorAction::RestartOverlay => {
            stop_overlay_runtime(&runtime).await?;
            start_overlay_runtime(&runtime).await?;
            print_voice_command_status(&runtime).await;
            Ok(true)
        }
        VoiceCommandOperatorAction::Stop => {
            stop_listener_runtime(&runtime).await?;
            Ok(true)
        }
        VoiceCommandOperatorAction::StopAgent => {
            stop_agent_runtime(&runtime).await?;
            Ok(true)
        }
        VoiceCommandOperatorAction::StopOverlay => {
            stop_overlay_runtime(&runtime).await?;
            Ok(true)
        }
        VoiceCommandOperatorAction::StopAll => {
            stop_agent_runtime(&runtime).await?;
            stop_listener_runtime(&runtime).await?;
            stop_overlay_runtime(&runtime).await?;
            stop_server_runtime(&runtime).await?;
            Ok(true)
        }
        VoiceCommandOperatorAction::WatchMic => {
            if tmux_has_session(&runtime.watch_session).await {
                println!("mic watcher already running: {}", runtime.watch_session);
                return Ok(true);
            }
            if !runtime.watch_script_path.is_file() {
                return Err(format!(
                    "watch script not found: {}",
                    runtime.watch_script_path.display()
                )
                .into());
            }
            let command = format!("bash {}", shell_single_quote(&runtime.watch_script_path));
            let status = Command::new("tmux")
                .arg("new-session")
                .arg("-d")
                .arg("-s")
                .arg(&runtime.watch_session)
                .arg(command)
                .status()
                .await?;
            if !status.success() {
                return Err(format!(
                    "failed to start mic watcher session: {}",
                    runtime.watch_session
                )
                .into());
            }
            println!("mic watcher started: {}", runtime.watch_session);
            Ok(true)
        }
        VoiceCommandOperatorAction::StopWatchMic => {
            if tmux_has_session(&runtime.watch_session).await {
                let status = Command::new("tmux")
                    .arg("kill-session")
                    .arg("-t")
                    .arg(&runtime.watch_session)
                    .status()
                    .await?;
                if !status.success() {
                    return Err(format!(
                        "failed to kill mic watcher session: {}",
                        runtime.watch_session
                    )
                    .into());
                }
                println!("mic watcher stopped: {}", runtime.watch_session);
            } else {
                println!("mic watcher not running: {}", runtime.watch_session);
            }
            Ok(true)
        }
        _ if voice_command_operator_log_session(action, &runtime).is_some() => {
            let session =
                voice_command_operator_log_session(action, &runtime).expect("session checked");
            show_tmux_logs(session).await?;
            Ok(true)
        }
        _ if voice_command_operator_attach_session(action, &runtime).is_some() => {
            let session =
                voice_command_operator_attach_session(action, &runtime).expect("session checked");
            attach_tmux_session(session).await?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

/// Launch the current voice command tmux/operator compatibility entrypoint.
pub async fn run_voice_command_operator(
    action: &VoiceCommandOperatorAction,
) -> Result<(), Box<dyn std::error::Error>> {
    if !run_direct_voice_command_operator_action(action).await? {
        return Err(format!(
            "voice command operator action not implemented: {:?}",
            action
        )
        .into());
    }

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

async fn initialize_runtime_components() -> Result<(), Box<dyn std::error::Error>> {
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

    Ok(())
}

async fn auto_start_configured_adapters() {
    let present_env_keys = present_nonempty_env_keys();
    let daemon_workdir = daemon_session_workdir();
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

        // Wrap the adapter in a bash supervisor loop so that if the process exits
        // for any reason (unrecoverable error, signal, crash) it is automatically
        // restarted after a 5-second delay without requiring manual intervention.
        // The .env file is sourced inside the loop so env vars are always present
        // even when the daemon was started without them exported.
        let script = format!(
            "ENV_FILE=\"$HOME/.config/yuiclaw/.env\"; \
            while true; do \
                if [ -f \"$ENV_FILE\" ]; then set -a; . \"$ENV_FILE\"; set +a; fi; \
                acomm {flag}; \
                echo '[yuiclaw] {label} adapter exited, restarting in 5s...' >&2; \
                sleep 5; \
            done",
            flag = spec.adapter_flag,
            label = spec.label,
        );
        let mut cmd = std::process::Command::new("bash");
        cmd.arg("-c")
            .arg(&script)
            .stdout(Stdio::null())
            .stderr(Stdio::inherit());
        // Background adapters inherit the daemon session workdir so all bridge-mediated
        // sessions run under YUICLAW_HOME when configured.
        apply_spawn_workdir_if_configured(&mut cmd, daemon_workdir.as_deref());

        match cmd.spawn() {
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
    let daemon_workdir = daemon_session_workdir();
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

        // Wrap the bridge in a supervisor loop so it auto-restarts on exit.
        let bridge_script = "while true; do acomm --bridge; \
             echo '[yuiclaw] bridge exited, restarting in 3s...' >&2; sleep 3; done";
        let mut cmd = std::process::Command::new("bash");
        cmd.arg("-c")
            .arg(bridge_script)
            .stdout(Stdio::null())
            .stderr(Stdio::inherit());
        apply_spawn_workdir_if_configured(&mut cmd, daemon_workdir.as_deref());

        if cmd.spawn().is_err() {
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

fn remove_socket_file_if_exists(path: &str) -> Result<bool, std::io::Error> {
    if Path::new(path).exists() {
        std::fs::remove_file(path)?;
        println!("Removed socket: {}", path);
        return Ok(true);
    }
    Ok(false)
}

fn resolve_daemon_session_workdir_from_env_value(raw: Option<String>) -> Option<PathBuf> {
    let raw = raw?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(PathBuf::from(trimmed))
}

fn daemon_session_workdir() -> Option<PathBuf> {
    // Prefer YUICLAW_HOME, fall back to YUICLAW_WORKSPACES_ROOT, then auto-detect.
    resolve_daemon_session_workdir_from_env_value(std::env::var("YUICLAW_HOME").ok())
        .or_else(|| resolve_daemon_session_workdir_from_env_value(std::env::var("YUICLAW_WORKSPACES_ROOT").ok()))
        .or_else(|| Some(crate::voice_command::resolve_workspaces_root()))
}

fn apply_spawn_workdir_if_configured(cmd: &mut std::process::Command, workdir: Option<&Path>) {
    if let Some(dir) = workdir {
        cmd.current_dir(dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voice_command::{
        VoiceCommandOperatorAction, resolve_voice_command_operator_runtime_config,
    };
    use std::sync::{Mutex, MutexGuard};
    use tempfile::tempdir;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn env_lock() -> MutexGuard<'static, ()> {
        ENV_LOCK.lock().expect("process env lock poisoned")
    }

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

    #[test]
    fn remove_socket_file_if_exists_removes_existing_file() {
        let dir = tempdir().unwrap();
        let sock_path = dir.path().join("acomm.sock");
        std::fs::write(&sock_path, b"stale").unwrap();
        assert!(sock_path.exists());

        let removed = remove_socket_file_if_exists(sock_path.to_str().unwrap()).unwrap();

        assert!(removed);
        assert!(!sock_path.exists());
    }

    #[test]
    fn remove_socket_file_if_exists_returns_false_when_missing() {
        let dir = tempdir().unwrap();
        let sock_path = dir.path().join("missing.sock");

        let removed = remove_socket_file_if_exists(sock_path.to_str().unwrap()).unwrap();

        assert!(!removed);
        assert!(!sock_path.exists());
    }

    #[test]
    fn resolve_daemon_session_workdir_from_env_value_returns_none_for_missing_or_blank() {
        assert!(resolve_daemon_session_workdir_from_env_value(None).is_none());
        assert!(resolve_daemon_session_workdir_from_env_value(Some("".into())).is_none());
        assert!(resolve_daemon_session_workdir_from_env_value(Some("   ".into())).is_none());
    }

    #[test]
    fn resolve_daemon_session_workdir_from_env_value_returns_trimmed_path() {
        let path =
            resolve_daemon_session_workdir_from_env_value(Some(" /tmp/yuiclaw-home ".into()))
                .expect("path should be parsed");
        assert_eq!(path, PathBuf::from("/tmp/yuiclaw-home"));
    }

    #[test]
    fn daemon_session_workdir_falls_back_to_workspaces_root_when_env_unset() {
        let _guard = env_lock();
        unsafe {
            std::env::remove_var("YUICLAW_HOME");
            std::env::remove_var("YUICLAW_WORKSPACES_ROOT");
        }
        // Should never return None — always resolves to some workspaces root.
        assert!(daemon_session_workdir().is_some());
    }

    #[test]
    fn daemon_session_workdir_prefers_yuiclaw_home() {
        let _guard = env_lock();
        unsafe {
            std::env::set_var("YUICLAW_HOME", "/tmp/yuiclaw-home-test");
            std::env::remove_var("YUICLAW_WORKSPACES_ROOT");
        }
        let dir = daemon_session_workdir().unwrap();
        assert_eq!(dir, PathBuf::from("/tmp/yuiclaw-home-test"));
        unsafe { std::env::remove_var("YUICLAW_HOME"); }
    }

    #[test]
    fn apply_spawn_workdir_if_configured_sets_command_current_dir() {
        let dir = tempdir().unwrap();
        let mut cmd = std::process::Command::new("sh");
        apply_spawn_workdir_if_configured(&mut cmd, Some(dir.path()));
        assert_eq!(cmd.get_current_dir(), Some(dir.path()));
    }

    #[test]
    fn apply_spawn_workdir_if_configured_leaves_current_dir_unset_when_missing() {
        let mut cmd = std::process::Command::new("sh");
        apply_spawn_workdir_if_configured(&mut cmd, None);
        assert!(cmd.get_current_dir().is_none());
    }

    #[test]
    fn operator_log_session_resolves_known_sessions() {
        let runtime = resolve_voice_command_operator_runtime_config();
        assert_eq!(
            voice_command_operator_log_session(&VoiceCommandOperatorAction::LogsAgent, &runtime),
            Some(runtime.agent_session.as_str())
        );
        assert_eq!(
            voice_command_operator_log_session(&VoiceCommandOperatorAction::LogsOverlay, &runtime),
            Some(runtime.overlay_session.as_str())
        );
        assert_eq!(
            voice_command_operator_log_session(&VoiceCommandOperatorAction::StartAgent, &runtime),
            None
        );
    }

    #[test]
    fn operator_attach_session_resolves_known_sessions() {
        let runtime = resolve_voice_command_operator_runtime_config();
        assert_eq!(
            voice_command_operator_attach_session(
                &VoiceCommandOperatorAction::AttachListener,
                &runtime
            ),
            Some(runtime.listener_session.as_str())
        );
        assert_eq!(
            voice_command_operator_attach_session(
                &VoiceCommandOperatorAction::AttachLockScreen,
                &runtime
            ),
            Some(runtime.lock_screen_session.as_str())
        );
        assert_eq!(
            voice_command_operator_attach_session(&VoiceCommandOperatorAction::LogsAgent, &runtime),
            None
        );
    }

    #[test]
    fn extract_parec_source_from_args_reads_dash_d_value() {
        assert_eq!(
            extract_parec_source_from_args("parec -d alsa_input.usb --raw"),
            Some("alsa_input.usb".to_string())
        );
        assert_eq!(
            extract_parec_source_from_args("bash -lc parec -d not-matched"),
            None
        );
        assert_eq!(extract_parec_source_from_args("parec --help"), None);
    }

    #[test]
    fn render_voice_command_status_reports_running_services_and_warning() {
        let snapshot = VoiceCommandStatusSnapshot {
            stt_backend: "whisper".to_string(),
            moonshine_model_size: "base".to_string(),
            server_session: "server-x".to_string(),
            server_url: "http://127.0.0.1:18080".to_string(),
            server_model: "/models/ggml-small.bin".to_string(),
            language: "ja".to_string(),
            listener_session: "listener-x".to_string(),
            agent_session: "agent-x".to_string(),
            overlay_session: "overlay-x".to_string(),
            lock_screen_session: "lock-x".to_string(),
            overlay_ipc: "127.0.0.1:47832".to_string(),
            lock_screen_ipc: "127.0.0.1:47833".to_string(),
            listener_script: "/tmp/listener.py".to_string(),
            agent_script: "/tmp/agent.py".to_string(),
            mic_source: "alsa_input.usb".to_string(),
            active_listener_source: Some("alsa_input.usb".to_string()),
            active_agent_source: Some("alsa_input.usb".to_string()),
            server_session_state: VoiceCommandServiceState::Running("server-x".to_string()),
            listener_session_state: VoiceCommandServiceState::Running("listener-x".to_string()),
            agent_session_state: VoiceCommandServiceState::Running("agent-x".to_string()),
            overlay_session_state: VoiceCommandServiceState::Running("overlay-x".to_string()),
            lock_screen_session_state: VoiceCommandServiceState::Running("lock-x".to_string()),
            server_endpoint: VoiceCommandEndpointState::Ready(
                "yes".to_string(),
                "http://127.0.0.1:18080/".to_string(),
            ),
            overlay_endpoint: VoiceCommandEndpointState::Ready(
                "yes".to_string(),
                "127.0.0.1:47832".to_string(),
            ),
            lock_screen_endpoint: VoiceCommandEndpointState::Ready(
                "yes".to_string(),
                "127.0.0.1:47833".to_string(),
            ),
            contention_warning: true,
        };

        let rendered = render_voice_command_status(&snapshot);

        assert!(rendered.contains("stt_backend=whisper"));
        assert!(rendered.contains("active_listener_source=alsa_input.usb"));
        assert!(rendered.contains("warning: listener+agent microphone contention likely"));
        assert!(rendered.contains("server: RUNNING (server-x)"));
        assert!(rendered.contains("ready: yes (http://127.0.0.1:18080/)"));
    }

    #[test]
    fn render_voice_command_status_handles_disabled_overlay_and_moonshine() {
        let snapshot = VoiceCommandStatusSnapshot {
            stt_backend: "moonshine".to_string(),
            moonshine_model_size: "tiny".to_string(),
            server_session: "server-x".to_string(),
            server_url: "http://127.0.0.1:18080".to_string(),
            server_model: "/models/ggml-small.bin".to_string(),
            language: "ja".to_string(),
            listener_session: "listener-x".to_string(),
            agent_session: "agent-x".to_string(),
            overlay_session: "overlay-x".to_string(),
            lock_screen_session: "lock-x".to_string(),
            overlay_ipc: "127.0.0.1:47832".to_string(),
            lock_screen_ipc: "127.0.0.1:0".to_string(),
            listener_script: "/tmp/listener.py".to_string(),
            agent_script: "/tmp/agent.py".to_string(),
            mic_source: "<auto>".to_string(),
            active_listener_source: None,
            active_agent_source: None,
            server_session_state: VoiceCommandServiceState::NotApplicable(
                "moonshine backend — no whisper-server needed".to_string(),
            ),
            listener_session_state: VoiceCommandServiceState::Stopped("listener-x".to_string()),
            agent_session_state: VoiceCommandServiceState::Stopped("agent-x".to_string()),
            overlay_session_state: VoiceCommandServiceState::Stopped("overlay-x".to_string()),
            lock_screen_session_state: VoiceCommandServiceState::Disabled("lock-x".to_string()),
            server_endpoint: VoiceCommandEndpointState::NotApplicable(
                "moonshine backend".to_string(),
            ),
            overlay_endpoint: VoiceCommandEndpointState::Disabled(
                "WHISPER_AGENT_NO_OVERLAY=1".to_string(),
            ),
            lock_screen_endpoint: VoiceCommandEndpointState::Disabled(
                "WHISPER_AGENT_NO_OVERLAY=1".to_string(),
            ),
            contention_warning: false,
        };

        let rendered = render_voice_command_status(&snapshot);

        assert!(rendered.contains("moonshine_model_size=tiny"));
        assert!(rendered.contains("server: N/A (moonshine backend — no whisper-server needed)"));
        assert!(rendered.contains("disabled: yes (WHISPER_AGENT_NO_OVERLAY=1)"));
        assert!(rendered.contains("lock-screen: DISABLED (lock-x)"));
    }

    #[test]
    fn build_caption_overlay_start_command_exports_ipc_and_display() {
        let runtime = resolve_voice_command_operator_runtime_config();
        let runtime = crate::voice_command::VoiceCommandOperatorRuntimeConfig {
            overlay_root: PathBuf::from("/tmp/acaption"),
            overlay_host: "127.0.0.1".to_string(),
            overlay_port: "47832".to_string(),
            overlay_display: Some(":1".to_string()),
            overlay_xauthority: Some("/tmp/xauth-overlay".to_string()),
            ..runtime
        };

        let command = build_caption_overlay_start_command(&runtime);

        assert!(command.contains("cd '/tmp/acaption'"));
        assert!(command.contains("export DISPLAY=':1'"));
        assert!(command.contains("export CAPTION_OVERLAY_IPC_PORT='47832'"));
        assert!(command.contains("exec npm run start"));
    }

    #[test]
    fn build_lock_screen_start_command_exports_credentials_and_port() {
        let runtime = resolve_voice_command_operator_runtime_config();
        let runtime = crate::voice_command::VoiceCommandOperatorRuntimeConfig {
            lock_screen_root: PathBuf::from("/tmp/asec"),
            lock_screen_port: "47833".to_string(),
            lock_screen_display: Some(":1".to_string()),
            lock_screen_xauthority: Some("/tmp/xauth-lock".to_string()),
            biometric_password_file: Some(PathBuf::from("/tmp/password.enc")),
            biometric_password_private_key: Some(PathBuf::from("/tmp/key.pem")),
            biometric_unlock_signal_file: Some(PathBuf::from("/tmp/unlock.signal")),
            ..runtime
        };

        let command = build_lock_screen_start_command(&runtime);

        assert!(command.contains("cd '/tmp/asec'"));
        assert!(command.contains("export ASEC_IPC_PORT='47833'"));
        assert!(command.contains("export ASEC_DISPLAY=':1'"));
        assert!(command.contains("export ASEC_BIOMETRIC_PASSWORD_FILE='/tmp/password.enc'"));
        assert!(command.contains("exec npm run start:bridge -- --tcp-port 47833"));
    }

    #[test]
    fn build_server_command_args_use_runtime_values() {
        let runtime = resolve_voice_command_operator_runtime_config();
        let runtime = crate::voice_command::VoiceCommandOperatorRuntimeConfig {
            server_bin: PathBuf::from("/tmp/whisper-server"),
            server_host: "0.0.0.0".to_string(),
            server_port: "19090".to_string(),
            server_model: PathBuf::from("/models/ggml-medium.bin"),
            whisper_language: "en".to_string(),
            ..runtime
        };

        let args = build_server_command_args(&runtime);

        assert_eq!(
            args,
            vec![
                "/tmp/whisper-server",
                "--host",
                "0.0.0.0",
                "--port",
                "19090",
                "-m",
                "/models/ggml-medium.bin",
                "-l",
                "en",
                "-nt",
                "-ng",
            ]
        );
    }

    #[test]
    fn build_listener_command_args_include_whisper_env_overrides() {
        let _guard = env_lock();
        unsafe {
            std::env::set_var("WHISPER_LISTEN_STT_PROMPT", "システム 状況報告");
            std::env::set_var("WHISPER_LISTEN_DEBUG", "1");
            std::env::set_var("WHISPER_LISTEN_MAX_RUN_SEC", "30");
            std::env::set_var("WHISPER_LISTEN_MAX_SEGMENTS", "2");
        }

        let runtime = resolve_voice_command_operator_runtime_config();
        let runtime = crate::voice_command::VoiceCommandOperatorRuntimeConfig {
            stt_backend: "whisper".to_string(),
            listener_script_path: PathBuf::from("/tmp/listener.py"),
            whisper_listen_tmp_dir: PathBuf::from("/tmp/voices"),
            server_url: "http://127.0.0.1:18080".to_string(),
            whisper_language: "ja".to_string(),
            whisper_mic_source: Some("alsa_input.usb".to_string()),
            ..runtime
        };

        let args = build_listener_command_args(&runtime);

        assert_eq!(
            args,
            vec![
                "python3",
                "/tmp/listener.py",
                "--tmp-dir",
                "/tmp/voices",
                "--server",
                "http://127.0.0.1:18080",
                "--language",
                "ja",
                "--stt-prompt",
                "システム 状況報告",
                "--source",
                "alsa_input.usb",
                "--debug",
                "--max-run-sec",
                "30",
                "--max-segments",
                "2",
            ]
        );

        unsafe {
            std::env::remove_var("WHISPER_LISTEN_STT_PROMPT");
            std::env::remove_var("WHISPER_LISTEN_DEBUG");
            std::env::remove_var("WHISPER_LISTEN_MAX_RUN_SEC");
            std::env::remove_var("WHISPER_LISTEN_MAX_SEGMENTS");
        }
    }

    #[test]
    fn build_agent_command_args_include_overlay_speaker_and_biometric_options() {
        let _guard = env_lock();
        unsafe {
            std::env::set_var("WHISPER_AGENT_DEBUG", "1");
            std::env::set_var("WHISPER_AGENT_MAX_RUN_SEC", "40");
            std::env::set_var("WHISPER_AGENT_STT_PROMPT", "システム おはよう");
            std::env::set_var("WHISPER_AGENT_CALIBRATION_MS", "600");
            std::env::set_var("WHISPER_AGENT_START_RMS", "0.02");
            std::env::set_var("WHISPER_AGENT_STOP_RMS", "0.01");
            std::env::set_var("WHISPER_AGENT_NO_VOICE", "1");
            std::env::set_var("WHISPER_AGENT_WAIT_ACK_AFTER_ACTION", "1");
            std::env::set_var("WHISPER_AGENT_AUDIO_SINK", "sink-main");
            std::env::set_var("WHISPER_AGENT_NOTIFY_PROGRESS", "1");
            std::env::set_var("WHISPER_AGENT_SPEAKER_ID", "1");
            std::env::set_var("WHISPER_AGENT_SPEAKER_MASTER", "/tmp/master.npy");
            std::env::set_var("WHISPER_AGENT_SPEAKER_THRESHOLD", "0.75");
            std::env::set_var("WHISPER_AGENT_SPEAKER_TOPK", "7");
            std::env::set_var("WHISPER_AGENT_SPEAKER_DEVICE", "cuda:0");
            std::env::set_var("WHISPER_AGENT_BIOMETRIC_LOCK", "1");
            std::env::set_var("WHISPER_AGENT_BIOMETRIC_START_LOCKED", "1");
            std::env::set_var("WHISPER_AGENT_BIOMETRIC_COMMAND_IDLE_LOCK_SEC", "1800");
            std::env::set_var("WHISPER_AGENT_BIOMETRIC_FACE_ABSENT_LOCK_SEC", "900");
            std::env::set_var("WHISPER_AGENT_BIOMETRIC_UNLOCK_FACE_FRESH_MS", "1000");
            std::env::set_var("WHISPER_AGENT_BIOMETRIC_POLL_SEC", "5");
            std::env::set_var(
                "WHISPER_AGENT_GOD_MODE_STATUS_URL",
                "http://127.0.0.1:8765/status",
            );
            std::env::set_var(
                "WHISPER_AGENT_BIOMETRIC_PASSWORD_PUBLIC_KEY",
                "/tmp/pub.pem",
            );
            std::env::set_var(
                "WHISPER_AGENT_BIOMETRIC_LOCK_SIGNAL_FILE",
                "/tmp/lock.signal",
            );
        }

        let runtime = resolve_voice_command_operator_runtime_config();
        let runtime = crate::voice_command::VoiceCommandOperatorRuntimeConfig {
            stt_backend: "whisper".to_string(),
            agent_script_path: PathBuf::from("/tmp/agent.py"),
            whisper_listen_tmp_dir: PathBuf::from("/tmp/voices"),
            server_url: "http://127.0.0.1:18080".to_string(),
            whisper_language: "ja".to_string(),
            whisper_mic_source: Some("alsa_input.usb".to_string()),
            overlay_host: "127.0.0.1".to_string(),
            overlay_port: "47832".to_string(),
            lock_screen_port: "47833".to_string(),
            biometric_password_file: Some(PathBuf::from("/tmp/password.enc")),
            biometric_password_private_key: Some(PathBuf::from("/tmp/key.pem")),
            biometric_unlock_signal_file: Some(PathBuf::from("/tmp/unlock.signal")),
            ..runtime
        };

        let args = build_agent_command_args(&runtime);

        assert!(args.starts_with(&[
            "python3".to_string(),
            "/tmp/agent.py".to_string(),
            "--tmp-dir".to_string(),
            "/tmp/voices".to_string(),
            "--stt-backend".to_string(),
            "whisper".to_string(),
        ]));
        assert!(args.contains(&"--overlay-ipc-host".to_string()));
        assert!(args.contains(&"127.0.0.1".to_string()));
        assert!(args.contains(&"--speaker-id".to_string()));
        assert!(args.contains(&"/tmp/master.npy".to_string()));
        assert!(args.contains(&"cuda:0".to_string()));
        assert!(args.contains(&"--biometric-lock".to_string()));
        assert!(args.contains(&"--biometric-start-locked".to_string()));
        assert!(args.contains(&"/tmp/password.enc".to_string()));
        assert!(args.contains(&"/tmp/key.pem".to_string()));
        assert!(args.contains(&"/tmp/pub.pem".to_string()));
        assert!(args.contains(&"/tmp/lock.signal".to_string()));
        assert!(args.contains(&"/tmp/unlock.signal".to_string()));
        assert!(args.contains(&"http://127.0.0.1:8765/status".to_string()));

        unsafe {
            std::env::remove_var("WHISPER_AGENT_DEBUG");
            std::env::remove_var("WHISPER_AGENT_MAX_RUN_SEC");
            std::env::remove_var("WHISPER_AGENT_STT_PROMPT");
            std::env::remove_var("WHISPER_AGENT_CALIBRATION_MS");
            std::env::remove_var("WHISPER_AGENT_START_RMS");
            std::env::remove_var("WHISPER_AGENT_STOP_RMS");
            std::env::remove_var("WHISPER_AGENT_NO_VOICE");
            std::env::remove_var("WHISPER_AGENT_WAIT_ACK_AFTER_ACTION");
            std::env::remove_var("WHISPER_AGENT_AUDIO_SINK");
            std::env::remove_var("WHISPER_AGENT_NOTIFY_PROGRESS");
            std::env::remove_var("WHISPER_AGENT_SPEAKER_ID");
            std::env::remove_var("WHISPER_AGENT_SPEAKER_MASTER");
            std::env::remove_var("WHISPER_AGENT_SPEAKER_THRESHOLD");
            std::env::remove_var("WHISPER_AGENT_SPEAKER_TOPK");
            std::env::remove_var("WHISPER_AGENT_SPEAKER_DEVICE");
            std::env::remove_var("WHISPER_AGENT_BIOMETRIC_LOCK");
            std::env::remove_var("WHISPER_AGENT_BIOMETRIC_START_LOCKED");
            std::env::remove_var("WHISPER_AGENT_BIOMETRIC_COMMAND_IDLE_LOCK_SEC");
            std::env::remove_var("WHISPER_AGENT_BIOMETRIC_FACE_ABSENT_LOCK_SEC");
            std::env::remove_var("WHISPER_AGENT_BIOMETRIC_UNLOCK_FACE_FRESH_MS");
            std::env::remove_var("WHISPER_AGENT_BIOMETRIC_POLL_SEC");
            std::env::remove_var("WHISPER_AGENT_GOD_MODE_STATUS_URL");
            std::env::remove_var("WHISPER_AGENT_BIOMETRIC_PASSWORD_PUBLIC_KEY");
            std::env::remove_var("WHISPER_AGENT_BIOMETRIC_LOCK_SIGNAL_FILE");
        }
    }

    #[test]
    fn build_agent_command_args_defaults_speaker_master_to_ahear_model() {
        let _guard = env_lock();
        unsafe {
            std::env::set_var("YUICLAW_WORKSPACES_ROOT", "/workspaces");
            std::env::remove_var("WHISPER_AGENT_SPEAKER_MASTER");
            std::env::set_var("WHISPER_AGENT_SPEAKER_ID", "1");
        }

        let runtime = resolve_voice_command_operator_runtime_config();
        let args = build_agent_command_args(&runtime);

        assert!(args.contains(&"--speaker-master".to_string()));
        assert!(args.contains(
            &"/workspaces/repos/ahear/python/src/ahear/models/master_voiceprint.npy".to_string()
        ));
        assert!(!args.contains(
            &"/workspaces/tmp/whispercpp-listen/tests/fixtures/master_voiceprint.npy".to_string()
        ));

        unsafe {
            std::env::remove_var("YUICLAW_WORKSPACES_ROOT");
            std::env::remove_var("WHISPER_AGENT_SPEAKER_MASTER");
            std::env::remove_var("WHISPER_AGENT_SPEAKER_ID");
        }
    }
}
