use clap::Subcommand;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceCommandLaunchSpec {
    pub program: OsString,
    pub entrypoint_path: PathBuf,
    pub args: Vec<OsString>,
    pub env: Vec<(OsString, OsString)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceCommandOperatorRuntimeConfig {
    pub workspaces_root: PathBuf,
    pub watch_script_path: PathBuf,
    pub listener_script_path: PathBuf,
    pub agent_script_path: PathBuf,
    pub overlay_root: PathBuf,
    pub lock_screen_root: PathBuf,
    pub stt_backend: String,
    pub moonshine_model_size: String,
    pub server_bin: PathBuf,
    pub server_model: PathBuf,
    pub server_host: String,
    pub server_port: String,
    pub server_url: String,
    pub whisper_language: String,
    pub whisper_mic_source: Option<String>,
    pub whisper_listen_tmp_dir: PathBuf,
    pub server_session: String,
    pub listener_session: String,
    pub agent_session: String,
    pub overlay_session: String,
    pub overlay_host: String,
    pub overlay_port: String,
    pub overlay_display: Option<String>,
    pub overlay_xauthority: Option<String>,
    pub lock_screen_session: String,
    pub lock_screen_port: String,
    pub lock_screen_display: Option<String>,
    pub lock_screen_xauthority: Option<String>,
    pub whisper_agent_no_overlay: bool,
    pub biometric_password_file: Option<PathBuf>,
    pub biometric_password_private_key: Option<PathBuf>,
    pub biometric_unlock_signal_file: Option<PathBuf>,
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

pub fn resolve_voice_command_entrypoint_path(workspaces_root: &Path) -> PathBuf {
    if let Some(entrypoint_path) = std::env::var_os("YUICLAW_VOICE_COMMAND_ENTRYPOINT") {
        return PathBuf::from(entrypoint_path);
    }
    workspaces_root.join("repos/arouter/scripts/voice_command_runtime.py")
}

pub fn resolve_voice_command_operator_runtime_config() -> VoiceCommandOperatorRuntimeConfig {
    let workspaces_root = resolve_workspaces_root();
    let stt_backend = std::env::var("STT_BACKEND").unwrap_or_else(|_| "moonshine".to_string());
    let moonshine_model_size =
        std::env::var("MOONSHINE_MODEL_SIZE").unwrap_or_else(|_| "base".to_string());
    let whisper_root = std::env::var_os("WHISPER_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspaces_root.join("repos/whisper.cpp"));
    let server_bin = std::env::var_os("WHISPER_SERVER_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| whisper_root.join("build/bin/whisper-server"));
    let default_model = whisper_root.join("models/ggml-small.bin");
    let server_model = std::env::var_os("WHISPER_SERVER_MODEL")
        .map(PathBuf::from)
        .unwrap_or(default_model);
    let server_host =
        std::env::var("WHISPER_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let server_port = std::env::var("WHISPER_SERVER_PORT").unwrap_or_else(|_| "18080".to_string());
    let server_url = format!("http://{}:{}", server_host, server_port);
    let whisper_language = std::env::var("WHISPER_LANGUAGE").unwrap_or_else(|_| "ja".to_string());
    let whisper_mic_source = std::env::var("WHISPER_MIC_SOURCE")
        .ok()
        .filter(|value| !value.is_empty());
    let whisper_listen_tmp_dir = std::env::var_os("WHISPER_LISTEN_TMP_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/home/yuiseki/Workspaces/private/datasets/voices/_raw"));
    let overlay_root = std::env::var_os("CAPTION_OVERLAY_ROOT")
        .or_else(|| std::env::var_os("CAPTION_OVERLAY_POC_ROOT"))
        .map(PathBuf::from)
        .unwrap_or_else(|| workspaces_root.join("repos/acaption"));
    let overlay_host = std::env::var("WHISPER_AGENT_OVERLAY_IPC_HOST")
        .or_else(|_| std::env::var("CAPTION_OVERLAY_IPC_HOST"))
        .unwrap_or_else(|_| "127.0.0.1".to_string());
    let overlay_port = std::env::var("WHISPER_AGENT_OVERLAY_IPC_PORT")
        .or_else(|_| std::env::var("CAPTION_OVERLAY_IPC_PORT"))
        .unwrap_or_else(|_| "47832".to_string());
    let overlay_display = std::env::var("CAPTION_OVERLAY_DISPLAY")
        .ok()
        .filter(|value| !value.is_empty());
    let overlay_xauthority = std::env::var("CAPTION_OVERLAY_XAUTHORITY")
        .ok()
        .filter(|value| !value.is_empty());
    let lock_screen_root = std::env::var_os("LOCK_SCREEN_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspaces_root.join("repos/asec"));
    let lock_screen_display = std::env::var("ASEC_DISPLAY")
        .ok()
        .filter(|value| !value.is_empty());
    let lock_screen_xauthority = std::env::var("ASEC_XAUTHORITY")
        .ok()
        .filter(|value| !value.is_empty());
    let whisper_agent_no_overlay = std::env::var("WHISPER_AGENT_NO_OVERLAY")
        .map(|value| value == "1")
        .unwrap_or(false);
    let biometric_password_file = std::env::var_os("WHISPER_AGENT_BIOMETRIC_PASSWORD_FILE")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    let biometric_password_private_key =
        std::env::var_os("WHISPER_AGENT_BIOMETRIC_PASSWORD_PRIVATE_KEY")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from);
    let biometric_unlock_signal_file =
        std::env::var_os("WHISPER_AGENT_BIOMETRIC_UNLOCK_SIGNAL_FILE")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from);
    let listener_script_path = std::env::var_os("WHISPER_LISTENER_SCRIPT")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            if stt_backend == "moonshine" {
                workspaces_root.join("repos/ahear/python/src/ahear/moonshine_listener.py")
            } else {
                workspaces_root.join("repos/ahear/python/src/ahear/whisper_listener.py")
            }
        });
    let agent_script_path = std::env::var_os("WHISPER_AGENT_SCRIPT")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspaces_root.join("repos/arouter/scripts/voice_command_runtime.py"));
    VoiceCommandOperatorRuntimeConfig {
        stt_backend,
        moonshine_model_size,
        server_bin,
        server_model,
        server_host,
        server_port,
        server_url,
        whisper_language,
        whisper_mic_source,
        whisper_listen_tmp_dir,
        watch_script_path: workspaces_root.join("tmp/whispercpp-listen/watch_dji_mic.sh"),
        listener_script_path,
        agent_script_path,
        overlay_root,
        lock_screen_root,
        server_session: std::env::var("WHISPER_SERVER_SESSION")
            .unwrap_or_else(|_| "whisper-server-ja".to_string()),
        listener_session: std::env::var("WHISPER_LISTENER_SESSION")
            .unwrap_or_else(|_| "whisper-listen-ja".to_string()),
        agent_session: std::env::var("WHISPER_AGENT_SESSION")
            .unwrap_or_else(|_| "whisper-agent-ja".to_string()),
        overlay_session: std::env::var("CAPTION_OVERLAY_SESSION")
            .unwrap_or_else(|_| "acaption-overlay".to_string()),
        overlay_host,
        overlay_port,
        overlay_display,
        overlay_xauthority,
        lock_screen_session: std::env::var("LOCK_SCREEN_SESSION")
            .unwrap_or_else(|_| "asec-lock-screen".to_string()),
        lock_screen_port: std::env::var("WHISPER_AGENT_LOCK_SCREEN_IPC_PORT")
            .unwrap_or_else(|_| "47833".to_string()),
        lock_screen_display,
        lock_screen_xauthority,
        whisper_agent_no_overlay,
        biometric_password_file,
        biometric_password_private_key,
        biometric_unlock_signal_file,
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
    let entrypoint_path = resolve_voice_command_entrypoint_path(&workspaces_root);
    let program = std::env::var_os("YUICLAW_VOICE_COMMAND_PYTHON")
        .unwrap_or_else(|| OsString::from("python3"));
    let mut args = vec![entrypoint_path.clone().into_os_string()];
    if let Some(text) = run_command {
        args.push(OsString::from("--run-command"));
        args.push(OsString::from(text));
    }
    args.extend(extra_args.iter().map(OsString::from));

    VoiceCommandLaunchSpec {
        program,
        entrypoint_path,
        args,
        env: vec![
            (
                OsString::from("YUICLAW_WORKSPACES_ROOT"),
                workspaces_root.into_os_string(),
            ),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_voice_command_launch_spec, resolve_voice_command_operator_runtime_config,
        resolve_voice_command_entrypoint_path, resolve_workspaces_root,
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
    fn resolve_voice_command_entrypoint_path_prefers_env_override() {
        let _guard = env_lock();
        unsafe {
            std::env::set_var(
                "YUICLAW_VOICE_COMMAND_ENTRYPOINT",
                "/tmp/custom-voice-command-entrypoint.py",
            )
        };
        let entrypoint_path = resolve_voice_command_entrypoint_path(Path::new("/unused"));
        assert_eq!(
            entrypoint_path,
            Path::new("/tmp/custom-voice-command-entrypoint.py")
        );
        unsafe { std::env::remove_var("YUICLAW_VOICE_COMMAND_ENTRYPOINT") };
    }

    #[test]
    fn build_voice_command_launch_spec_includes_run_command_and_extra_args() {
        let _guard = env_lock();
        unsafe {
            std::env::set_var("YUICLAW_WORKSPACES_ROOT", "/workspaces");
            std::env::set_var("YUICLAW_VOICE_COMMAND_PYTHON", "python-test");
            std::env::remove_var("YUICLAW_VOICE_COMMAND_ENTRYPOINT");
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
            spec.entrypoint_path,
            Path::new("/workspaces/repos/arouter/scripts/voice_command_runtime.py")
        );
        assert_eq!(
            spec.args,
            vec![
                OsString::from("/workspaces/repos/arouter/scripts/voice_command_runtime.py"),
                OsString::from("--run-command"),
                OsString::from("システム 街頭カメラを表示"),
                OsString::from("--debug"),
                OsString::from("--source"),
                OsString::from("mic"),
            ]
        );
        assert_eq!(
            spec.env,
            vec![
                (
                    OsString::from("YUICLAW_WORKSPACES_ROOT"),
                    OsString::from("/workspaces"),
                ),
            ]
        );

        unsafe {
            std::env::remove_var("YUICLAW_WORKSPACES_ROOT");
            std::env::remove_var("YUICLAW_VOICE_COMMAND_PYTHON");
        }
    }

    #[test]
    fn build_voice_command_launch_spec_sets_workspaces_root_env() {
        let _guard = env_lock();
        unsafe {
            std::env::set_var("YUICLAW_WORKSPACES_ROOT", "/workspaces");
            std::env::remove_var("YUICLAW_VOICE_COMMAND_ENTRYPOINT");
        }

        let spec = build_voice_command_launch_spec(None, &[]);

        assert_eq!(
            spec.env,
            vec![(
                OsString::from("YUICLAW_WORKSPACES_ROOT"),
                OsString::from("/workspaces"),
            )]
        );

        unsafe {
            std::env::remove_var("YUICLAW_WORKSPACES_ROOT");
        }
    }

    #[test]
    fn build_voice_command_launch_spec_ignores_existing_pythonpath() {
        let _guard = env_lock();
        unsafe {
            std::env::set_var("YUICLAW_WORKSPACES_ROOT", "/workspaces");
            std::env::set_var("PYTHONPATH", "/already/there");
        }

        let spec = build_voice_command_launch_spec(None, &[]);

        assert_eq!(
            spec.env,
            vec![(
                OsString::from("YUICLAW_WORKSPACES_ROOT"),
                OsString::from("/workspaces"),
            )]
        );

        unsafe {
            std::env::remove_var("YUICLAW_WORKSPACES_ROOT");
            std::env::remove_var("PYTHONPATH");
        }
    }

    #[test]
    fn resolve_voice_command_operator_runtime_config_uses_defaults() {
        let _guard = env_lock();
        unsafe {
            std::env::set_var("YUICLAW_WORKSPACES_ROOT", "/workspaces");
            std::env::remove_var("STT_BACKEND");
            std::env::remove_var("MOONSHINE_MODEL_SIZE");
            std::env::remove_var("WHISPER_SERVER_BIN");
            std::env::remove_var("WHISPER_SERVER_MODEL");
            std::env::remove_var("WHISPER_SERVER_HOST");
            std::env::remove_var("WHISPER_SERVER_PORT");
            std::env::remove_var("WHISPER_LANGUAGE");
            std::env::remove_var("WHISPER_MIC_SOURCE");
            std::env::remove_var("WHISPER_LISTEN_TMP_DIR");
            std::env::remove_var("WHISPER_LISTENER_SCRIPT");
            std::env::remove_var("WHISPER_AGENT_SCRIPT");
            std::env::remove_var("CAPTION_OVERLAY_ROOT");
            std::env::remove_var("CAPTION_OVERLAY_POC_ROOT");
            std::env::remove_var("WHISPER_AGENT_OVERLAY_IPC_HOST");
            std::env::remove_var("CAPTION_OVERLAY_IPC_HOST");
            std::env::remove_var("WHISPER_AGENT_OVERLAY_IPC_PORT");
            std::env::remove_var("CAPTION_OVERLAY_IPC_PORT");
            std::env::remove_var("CAPTION_OVERLAY_DISPLAY");
            std::env::remove_var("CAPTION_OVERLAY_XAUTHORITY");
            std::env::remove_var("LOCK_SCREEN_ROOT");
            std::env::remove_var("ASEC_DISPLAY");
            std::env::remove_var("ASEC_XAUTHORITY");
            std::env::remove_var("WHISPER_AGENT_NO_OVERLAY");
            std::env::remove_var("WHISPER_AGENT_BIOMETRIC_PASSWORD_FILE");
            std::env::remove_var("WHISPER_AGENT_BIOMETRIC_PASSWORD_PRIVATE_KEY");
            std::env::remove_var("WHISPER_AGENT_BIOMETRIC_UNLOCK_SIGNAL_FILE");
            std::env::remove_var("WHISPER_SERVER_SESSION");
            std::env::remove_var("WHISPER_LISTENER_SESSION");
            std::env::remove_var("WHISPER_AGENT_SESSION");
            std::env::remove_var("CAPTION_OVERLAY_SESSION");
            std::env::remove_var("LOCK_SCREEN_SESSION");
            std::env::remove_var("WHISPER_AGENT_LOCK_SCREEN_IPC_PORT");
            std::env::remove_var("WHISPER_WATCH_SESSION");
        }

        let config = resolve_voice_command_operator_runtime_config();

        assert_eq!(config.stt_backend, "moonshine");
        assert_eq!(config.moonshine_model_size, "base");
        assert_eq!(
            config.listener_script_path,
            Path::new("/workspaces/repos/ahear/python/src/ahear/moonshine_listener.py")
        );
        assert_eq!(
            config.agent_script_path,
            Path::new("/workspaces/repos/arouter/scripts/voice_command_runtime.py")
        );
        assert_eq!(config.server_url, "http://127.0.0.1:18080");
        assert_eq!(config.whisper_language, "ja");
        assert_eq!(config.whisper_mic_source, None);
        assert_eq!(config.overlay_root, Path::new("/workspaces/repos/acaption"));
        assert_eq!(config.overlay_host, "127.0.0.1");
        assert_eq!(config.overlay_port, "47832");
        assert_eq!(config.overlay_display, None);
        assert_eq!(config.overlay_xauthority, None);
        assert_eq!(config.lock_screen_root, Path::new("/workspaces/repos/asec"));
        assert_eq!(config.lock_screen_display, None);
        assert_eq!(config.lock_screen_xauthority, None);
        assert!(!config.whisper_agent_no_overlay);
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
            std::env::set_var("STT_BACKEND", "whisper");
            std::env::set_var("MOONSHINE_MODEL_SIZE", "tiny");
            std::env::set_var("WHISPER_SERVER_HOST", "0.0.0.0");
            std::env::set_var("WHISPER_SERVER_PORT", "19090");
            std::env::set_var("WHISPER_LANGUAGE", "en");
            std::env::set_var("WHISPER_MIC_SOURCE", "alsa_input.usb");
            std::env::set_var("WHISPER_LISTEN_TMP_DIR", "/tmp/voices");
            std::env::set_var("WHISPER_LISTENER_SCRIPT", "/tmp/listener.py");
            std::env::set_var("WHISPER_AGENT_SCRIPT", "/tmp/agent.py");
            std::env::set_var("CAPTION_OVERLAY_ROOT", "/tmp/acaption");
            std::env::set_var("WHISPER_AGENT_OVERLAY_IPC_HOST", "10.0.0.5");
            std::env::set_var("WHISPER_AGENT_OVERLAY_IPC_PORT", "49000");
            std::env::set_var("CAPTION_OVERLAY_DISPLAY", ":1");
            std::env::set_var("CAPTION_OVERLAY_XAUTHORITY", "/tmp/xauth-overlay");
            std::env::set_var("LOCK_SCREEN_ROOT", "/tmp/asec");
            std::env::set_var("ASEC_DISPLAY", ":2");
            std::env::set_var("ASEC_XAUTHORITY", "/tmp/xauth-lock");
            std::env::set_var("WHISPER_AGENT_NO_OVERLAY", "1");
            std::env::set_var("WHISPER_AGENT_BIOMETRIC_PASSWORD_FILE", "/tmp/password.enc");
            std::env::set_var(
                "WHISPER_AGENT_BIOMETRIC_PASSWORD_PRIVATE_KEY",
                "/tmp/key.pem",
            );
            std::env::set_var(
                "WHISPER_AGENT_BIOMETRIC_UNLOCK_SIGNAL_FILE",
                "/tmp/unlock.signal",
            );
            std::env::set_var("WHISPER_SERVER_SESSION", "server-x");
            std::env::set_var("WHISPER_LISTENER_SESSION", "listener-x");
            std::env::set_var("WHISPER_AGENT_SESSION", "agent-x");
            std::env::set_var("CAPTION_OVERLAY_SESSION", "overlay-x");
            std::env::set_var("LOCK_SCREEN_SESSION", "lock-x");
            std::env::set_var("WHISPER_AGENT_LOCK_SCREEN_IPC_PORT", "0");
            std::env::set_var("WHISPER_WATCH_SESSION", "watch-x");
        }

        let config = resolve_voice_command_operator_runtime_config();

        assert_eq!(config.stt_backend, "whisper");
        assert_eq!(config.moonshine_model_size, "tiny");
        assert_eq!(config.server_url, "http://0.0.0.0:19090");
        assert_eq!(config.whisper_language, "en");
        assert_eq!(config.whisper_mic_source.as_deref(), Some("alsa_input.usb"));
        assert_eq!(config.whisper_listen_tmp_dir, Path::new("/tmp/voices"));
        assert_eq!(config.listener_script_path, Path::new("/tmp/listener.py"));
        assert_eq!(config.agent_script_path, Path::new("/tmp/agent.py"));
        assert_eq!(config.overlay_root, Path::new("/tmp/acaption"));
        assert_eq!(config.overlay_host, "10.0.0.5");
        assert_eq!(config.overlay_port, "49000");
        assert_eq!(config.overlay_display.as_deref(), Some(":1"));
        assert_eq!(
            config.overlay_xauthority.as_deref(),
            Some("/tmp/xauth-overlay")
        );
        assert_eq!(config.lock_screen_root, Path::new("/tmp/asec"));
        assert_eq!(config.lock_screen_display.as_deref(), Some(":2"));
        assert_eq!(
            config.lock_screen_xauthority.as_deref(),
            Some("/tmp/xauth-lock")
        );
        assert!(config.whisper_agent_no_overlay);
        assert_eq!(
            config.biometric_password_file.as_deref(),
            Some(Path::new("/tmp/password.enc"))
        );
        assert_eq!(
            config.biometric_password_private_key.as_deref(),
            Some(Path::new("/tmp/key.pem"))
        );
        assert_eq!(
            config.biometric_unlock_signal_file.as_deref(),
            Some(Path::new("/tmp/unlock.signal"))
        );
        assert_eq!(config.server_session, "server-x");
        assert_eq!(config.listener_session, "listener-x");
        assert_eq!(config.agent_session, "agent-x");
        assert_eq!(config.overlay_session, "overlay-x");
        assert_eq!(config.lock_screen_session, "lock-x");
        assert_eq!(config.lock_screen_port, "0");
        assert_eq!(config.watch_session, "watch-x");

        unsafe {
            std::env::remove_var("YUICLAW_WORKSPACES_ROOT");
            std::env::remove_var("STT_BACKEND");
            std::env::remove_var("MOONSHINE_MODEL_SIZE");
            std::env::remove_var("WHISPER_SERVER_HOST");
            std::env::remove_var("WHISPER_SERVER_PORT");
            std::env::remove_var("WHISPER_LANGUAGE");
            std::env::remove_var("WHISPER_MIC_SOURCE");
            std::env::remove_var("WHISPER_LISTEN_TMP_DIR");
            std::env::remove_var("WHISPER_LISTENER_SCRIPT");
            std::env::remove_var("WHISPER_AGENT_SCRIPT");
            std::env::remove_var("CAPTION_OVERLAY_ROOT");
            std::env::remove_var("WHISPER_AGENT_OVERLAY_IPC_HOST");
            std::env::remove_var("WHISPER_AGENT_OVERLAY_IPC_PORT");
            std::env::remove_var("CAPTION_OVERLAY_DISPLAY");
            std::env::remove_var("CAPTION_OVERLAY_XAUTHORITY");
            std::env::remove_var("LOCK_SCREEN_ROOT");
            std::env::remove_var("ASEC_DISPLAY");
            std::env::remove_var("ASEC_XAUTHORITY");
            std::env::remove_var("WHISPER_AGENT_NO_OVERLAY");
            std::env::remove_var("WHISPER_AGENT_BIOMETRIC_PASSWORD_FILE");
            std::env::remove_var("WHISPER_AGENT_BIOMETRIC_PASSWORD_PRIVATE_KEY");
            std::env::remove_var("WHISPER_AGENT_BIOMETRIC_UNLOCK_SIGNAL_FILE");
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
