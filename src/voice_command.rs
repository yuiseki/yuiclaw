use clap::Subcommand;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceCommandLaunchSpec {
    pub program: OsString,
    pub script_path: PathBuf,
    pub args: Vec<OsString>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceCommandOperatorRuntimeConfig {
    pub workspaces_root: PathBuf,
    pub watch_script_path: PathBuf,
    pub server_session: String,
    pub listener_session: String,
    pub agent_session: String,
    pub overlay_session: String,
    pub lock_screen_session: String,
    pub lock_screen_port: String,
    pub watch_session: String,
    pub legacy_overlay_sessions: Vec<String>,
}

#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
pub enum VoiceCommandSubcommand {
    /// Manage the voice command operator compatibility runtime
    Operator {
        #[command(subcommand)]
        action: VoiceCommandOperatorAction,
    },
}

#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
pub enum VoiceCommandOperatorAction {
    /// Start whisper-server (if needed) and the listener
    Start,
    /// Restart the listener while keeping the server
    Restart,
    /// Restart the listener and whisper server
    RestartAll,
    /// Show the current voice command runtime status
    Status,
    /// Start the voice command agent
    StartAgent,
    /// Restart the voice command agent
    RestartAgent,
    /// Restart the voice command agent and whisper server
    RestartAgentAll,
    /// Stop the voice command agent
    StopAgent,
    /// Stop listener, agent, overlay, and server
    StopAll,
    /// Stop the listener
    Stop,
    /// Start the overlay stack
    StartOverlay,
    /// Restart the overlay stack
    RestartOverlay,
    /// Stop the overlay stack
    StopOverlay,
    /// Show recent whisper-server logs
    LogsServer,
    /// Show recent listener logs
    LogsListener,
    /// Show recent agent logs
    LogsAgent,
    /// Tail recent and live agent logs
    LogsAgentTail,
    /// Show recent overlay logs
    LogsOverlay,
    /// Show recent lock-screen logs
    LogsLockScreen,
    /// Attach to the whisper-server tmux session
    AttachServer,
    /// Attach to the listener tmux session
    AttachListener,
    /// Attach to the agent tmux session
    AttachAgent,
    /// Attach to the overlay tmux session
    AttachOverlay,
    /// Attach to the lock-screen tmux session
    AttachLockScreen,
    /// Start the DJI MIC MINI watcher
    WatchMic,
    /// Stop the DJI MIC MINI watcher
    StopWatchMic,
}

impl VoiceCommandOperatorAction {
    pub fn as_tmux_command(&self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Restart => "restart",
            Self::RestartAll => "restart-all",
            Self::Status => "status",
            Self::StartAgent => "start-agent",
            Self::RestartAgent => "restart-agent",
            Self::RestartAgentAll => "restart-agent-all",
            Self::StopAgent => "stop-agent",
            Self::StopAll => "stop-all",
            Self::Stop => "stop",
            Self::StartOverlay => "start-overlay",
            Self::RestartOverlay => "restart-overlay",
            Self::StopOverlay => "stop-overlay",
            Self::LogsServer => "logs-server",
            Self::LogsListener => "logs-listener",
            Self::LogsAgent => "logs-agent",
            Self::LogsAgentTail => "logs-agent-tail",
            Self::LogsOverlay => "logs-overlay",
            Self::LogsLockScreen => "logs-lock-screen",
            Self::AttachServer => "attach-server",
            Self::AttachListener => "attach-listener",
            Self::AttachAgent => "attach-agent",
            Self::AttachOverlay => "attach-overlay",
            Self::AttachLockScreen => "attach-lock-screen",
            Self::WatchMic => "watch-mic",
            Self::StopWatchMic => "stop-watch-mic",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceCommandOperatorLaunchSpec {
    pub program: OsString,
    pub script_path: PathBuf,
    pub args: Vec<OsString>,
    pub env: Vec<(OsString, OsString)>,
}

pub fn resolve_workspaces_root() -> PathBuf {
    if let Some(root) = std::env::var_os("YUICLAW_WORKSPACES_ROOT") {
        return PathBuf::from(root);
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or(manifest_dir)
}

pub fn resolve_voice_command_script_path(workspaces_root: &Path) -> PathBuf {
    if let Some(script_path) = std::env::var_os("YUICLAW_VOICE_COMMAND_SCRIPT") {
        return PathBuf::from(script_path);
    }
    workspaces_root.join("tmp/whispercpp-listen/voice_command_loop.py")
}

pub fn resolve_voice_command_operator_script_path(workspaces_root: &Path) -> PathBuf {
    if let Some(script_path) = std::env::var_os("YUICLAW_VOICE_COMMAND_OPERATOR_SCRIPT") {
        return PathBuf::from(script_path);
    }
    workspaces_root.join("tmp/whispercpp-listen/tmux_listen_only.sh")
}

pub fn resolve_voice_command_operator_runtime_config() -> VoiceCommandOperatorRuntimeConfig {
    let workspaces_root = resolve_workspaces_root();
    VoiceCommandOperatorRuntimeConfig {
        watch_script_path: workspaces_root.join("tmp/whispercpp-listen/watch_dji_mic.sh"),
        server_session: std::env::var("WHISPER_SERVER_SESSION")
            .unwrap_or_else(|_| "whisper-server-ja".to_string()),
        listener_session: std::env::var("WHISPER_LISTENER_SESSION")
            .unwrap_or_else(|_| "whisper-listen-ja".to_string()),
        agent_session: std::env::var("WHISPER_AGENT_SESSION")
            .unwrap_or_else(|_| "whisper-agent-ja".to_string()),
        overlay_session: std::env::var("CAPTION_OVERLAY_SESSION")
            .unwrap_or_else(|_| "acaption-overlay".to_string()),
        lock_screen_session: std::env::var("LOCK_SCREEN_SESSION")
            .unwrap_or_else(|_| "asec-lock-screen".to_string()),
        lock_screen_port: std::env::var("WHISPER_AGENT_LOCK_SCREEN_IPC_PORT")
            .unwrap_or_else(|_| "47833".to_string()),
        watch_session: std::env::var("WHISPER_WATCH_SESSION")
            .unwrap_or_else(|_| "whisper-watch-mic".to_string()),
        legacy_overlay_sessions: vec![
            "tauri-overlay".to_string(),
            "lock-screen-bridge".to_string(),
        ],
        workspaces_root,
    }
}

pub fn build_voice_command_launch_spec(
    run_command: Option<&str>,
    extra_args: &[String],
) -> VoiceCommandLaunchSpec {
    let workspaces_root = resolve_workspaces_root();
    let script_path = resolve_voice_command_script_path(&workspaces_root);
    let program = std::env::var_os("YUICLAW_VOICE_COMMAND_PYTHON")
        .unwrap_or_else(|| OsString::from("python3"));

    let mut args = vec![script_path.clone().into_os_string()];
    if let Some(text) = run_command {
        args.push(OsString::from("--run-command"));
        args.push(OsString::from(text));
    }
    args.extend(extra_args.iter().map(OsString::from));

    VoiceCommandLaunchSpec {
        program,
        script_path,
        args,
    }
}

pub fn build_voice_command_operator_launch_spec(
    action: &VoiceCommandOperatorAction,
) -> VoiceCommandOperatorLaunchSpec {
    let workspaces_root = resolve_workspaces_root();
    let script_path = resolve_voice_command_operator_script_path(&workspaces_root);
    let program = std::env::var_os("YUICLAW_VOICE_COMMAND_OPERATOR_SHELL")
        .unwrap_or_else(|| OsString::from("bash"));
    let args = vec![
        script_path.clone().into_os_string(),
        OsString::from(action.as_tmux_command()),
    ];
    let env = vec![(
        OsString::from("YUICLAW_VOICE_COMMAND_OPERATOR_BACKEND"),
        OsString::from("1"),
    )];

    VoiceCommandOperatorLaunchSpec {
        program,
        script_path,
        args,
        env,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        VoiceCommandOperatorAction, build_voice_command_launch_spec,
        build_voice_command_operator_launch_spec, resolve_voice_command_operator_runtime_config,
        resolve_voice_command_operator_script_path, resolve_voice_command_script_path,
        resolve_workspaces_root,
    };
    use std::ffi::OsString;
    use std::path::Path;
    use std::sync::{Mutex, MutexGuard};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn env_lock() -> MutexGuard<'static, ()> {
        ENV_LOCK.lock().expect("voice_command env lock poisoned")
    }

    #[test]
    fn resolve_workspaces_root_defaults_to_repo_parent() {
        let _guard = env_lock();
        unsafe { std::env::remove_var("YUICLAW_WORKSPACES_ROOT") };
        let root = resolve_workspaces_root();
        assert!(root.ends_with("Workspaces"));
    }

    #[test]
    fn resolve_voice_command_script_path_prefers_env_override() {
        let _guard = env_lock();
        unsafe {
            std::env::set_var(
                "YUICLAW_VOICE_COMMAND_SCRIPT",
                "/tmp/custom-voice-command.py",
            )
        };
        let script_path = resolve_voice_command_script_path(Path::new("/unused"));
        assert_eq!(script_path, Path::new("/tmp/custom-voice-command.py"));
        unsafe { std::env::remove_var("YUICLAW_VOICE_COMMAND_SCRIPT") };
    }

    #[test]
    fn build_voice_command_launch_spec_includes_run_command_and_extra_args() {
        let _guard = env_lock();
        unsafe {
            std::env::set_var("YUICLAW_WORKSPACES_ROOT", "/workspaces");
            std::env::set_var("YUICLAW_VOICE_COMMAND_PYTHON", "python-test");
            std::env::remove_var("YUICLAW_VOICE_COMMAND_SCRIPT");
        }

        let spec = build_voice_command_launch_spec(
            Some("システム 街頭カメラを表示"),
            &[
                "--debug".to_string(),
                "--source".to_string(),
                "mic".to_string(),
            ],
        );

        assert_eq!(spec.program, OsString::from("python-test"));
        assert_eq!(
            spec.script_path,
            Path::new("/workspaces/tmp/whispercpp-listen/voice_command_loop.py")
        );
        assert_eq!(
            spec.args,
            vec![
                OsString::from("/workspaces/tmp/whispercpp-listen/voice_command_loop.py"),
                OsString::from("--run-command"),
                OsString::from("システム 街頭カメラを表示"),
                OsString::from("--debug"),
                OsString::from("--source"),
                OsString::from("mic"),
            ]
        );

        unsafe {
            std::env::remove_var("YUICLAW_WORKSPACES_ROOT");
            std::env::remove_var("YUICLAW_VOICE_COMMAND_PYTHON");
        }
    }

    #[test]
    fn resolve_voice_command_operator_script_path_prefers_env_override() {
        let _guard = env_lock();
        unsafe {
            std::env::set_var(
                "YUICLAW_VOICE_COMMAND_OPERATOR_SCRIPT",
                "/tmp/custom-tmux-listen-only.sh",
            )
        };
        let script_path = resolve_voice_command_operator_script_path(Path::new("/unused"));
        assert_eq!(script_path, Path::new("/tmp/custom-tmux-listen-only.sh"));
        unsafe { std::env::remove_var("YUICLAW_VOICE_COMMAND_OPERATOR_SCRIPT") };
    }

    #[test]
    fn build_voice_command_operator_launch_spec_uses_bash_and_tmux_command() {
        let _guard = env_lock();
        unsafe {
            std::env::set_var("YUICLAW_WORKSPACES_ROOT", "/workspaces");
            std::env::set_var("YUICLAW_VOICE_COMMAND_OPERATOR_SHELL", "bash-test");
            std::env::remove_var("YUICLAW_VOICE_COMMAND_OPERATOR_SCRIPT");
        }

        let spec =
            build_voice_command_operator_launch_spec(&VoiceCommandOperatorAction::RestartAgentAll);

        assert_eq!(spec.program, OsString::from("bash-test"));
        assert_eq!(
            spec.script_path,
            Path::new("/workspaces/tmp/whispercpp-listen/tmux_listen_only.sh")
        );
        assert_eq!(
            spec.args,
            vec![
                OsString::from("/workspaces/tmp/whispercpp-listen/tmux_listen_only.sh"),
                OsString::from("restart-agent-all"),
            ]
        );
        assert_eq!(
            spec.env,
            vec![(
                OsString::from("YUICLAW_VOICE_COMMAND_OPERATOR_BACKEND"),
                OsString::from("1")
            )]
        );

        unsafe {
            std::env::remove_var("YUICLAW_WORKSPACES_ROOT");
            std::env::remove_var("YUICLAW_VOICE_COMMAND_OPERATOR_SHELL");
        }
    }

    #[test]
    fn resolve_voice_command_operator_runtime_config_uses_defaults() {
        let _guard = env_lock();
        unsafe {
            std::env::set_var("YUICLAW_WORKSPACES_ROOT", "/workspaces");
            std::env::remove_var("WHISPER_SERVER_SESSION");
            std::env::remove_var("WHISPER_LISTENER_SESSION");
            std::env::remove_var("WHISPER_AGENT_SESSION");
            std::env::remove_var("CAPTION_OVERLAY_SESSION");
            std::env::remove_var("LOCK_SCREEN_SESSION");
            std::env::remove_var("WHISPER_AGENT_LOCK_SCREEN_IPC_PORT");
            std::env::remove_var("WHISPER_WATCH_SESSION");
        }

        let config = resolve_voice_command_operator_runtime_config();

        assert_eq!(config.server_session, "whisper-server-ja");
        assert_eq!(config.listener_session, "whisper-listen-ja");
        assert_eq!(config.agent_session, "whisper-agent-ja");
        assert_eq!(config.overlay_session, "acaption-overlay");
        assert_eq!(config.lock_screen_session, "asec-lock-screen");
        assert_eq!(config.lock_screen_port, "47833");
        assert_eq!(config.watch_session, "whisper-watch-mic");
        assert_eq!(
            config.watch_script_path,
            Path::new("/workspaces/tmp/whispercpp-listen/watch_dji_mic.sh")
        );
        assert_eq!(
            config.legacy_overlay_sessions,
            vec![
                "tauri-overlay".to_string(),
                "lock-screen-bridge".to_string(),
            ]
        );

        unsafe {
            std::env::remove_var("YUICLAW_WORKSPACES_ROOT");
        }
    }

    #[test]
    fn resolve_voice_command_operator_runtime_config_prefers_env_overrides() {
        let _guard = env_lock();
        unsafe {
            std::env::set_var("YUICLAW_WORKSPACES_ROOT", "/workspaces");
            std::env::set_var("WHISPER_SERVER_SESSION", "server-x");
            std::env::set_var("WHISPER_LISTENER_SESSION", "listener-x");
            std::env::set_var("WHISPER_AGENT_SESSION", "agent-x");
            std::env::set_var("CAPTION_OVERLAY_SESSION", "overlay-x");
            std::env::set_var("LOCK_SCREEN_SESSION", "lock-x");
            std::env::set_var("WHISPER_AGENT_LOCK_SCREEN_IPC_PORT", "0");
            std::env::set_var("WHISPER_WATCH_SESSION", "watch-x");
        }

        let config = resolve_voice_command_operator_runtime_config();

        assert_eq!(config.server_session, "server-x");
        assert_eq!(config.listener_session, "listener-x");
        assert_eq!(config.agent_session, "agent-x");
        assert_eq!(config.overlay_session, "overlay-x");
        assert_eq!(config.lock_screen_session, "lock-x");
        assert_eq!(config.lock_screen_port, "0");
        assert_eq!(config.watch_session, "watch-x");

        unsafe {
            std::env::remove_var("YUICLAW_WORKSPACES_ROOT");
            std::env::remove_var("WHISPER_SERVER_SESSION");
            std::env::remove_var("WHISPER_LISTENER_SESSION");
            std::env::remove_var("WHISPER_AGENT_SESSION");
            std::env::remove_var("CAPTION_OVERLAY_SESSION");
            std::env::remove_var("LOCK_SCREEN_SESSION");
            std::env::remove_var("WHISPER_AGENT_LOCK_SCREEN_IPC_PORT");
            std::env::remove_var("WHISPER_WATCH_SESSION");
        }
    }
}
