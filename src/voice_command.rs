use clap::Subcommand;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceCommandLaunchSpec {
    pub program: OsString,
    pub script_path: PathBuf,
    pub args: Vec<OsString>,
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
    /// Start the overlay stack
    StartOverlay,
    /// Restart the overlay stack
    RestartOverlay,
    /// Stop the overlay stack
    StopOverlay,
}

impl VoiceCommandOperatorAction {
    pub fn as_tmux_command(&self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::StartAgent => "start-agent",
            Self::RestartAgent => "restart-agent",
            Self::RestartAgentAll => "restart-agent-all",
            Self::StopAgent => "stop-agent",
            Self::StartOverlay => "start-overlay",
            Self::RestartOverlay => "restart-overlay",
            Self::StopOverlay => "stop-overlay",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceCommandOperatorLaunchSpec {
    pub program: OsString,
    pub script_path: PathBuf,
    pub args: Vec<OsString>,
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

    VoiceCommandOperatorLaunchSpec {
        program,
        script_path,
        args,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        VoiceCommandOperatorAction, build_voice_command_launch_spec,
        build_voice_command_operator_launch_spec, resolve_voice_command_operator_script_path,
        resolve_voice_command_script_path, resolve_workspaces_root,
    };
    use std::ffi::OsString;
    use std::path::Path;

    #[test]
    fn resolve_workspaces_root_defaults_to_repo_parent() {
        unsafe { std::env::remove_var("YUICLAW_WORKSPACES_ROOT") };
        let root = resolve_workspaces_root();
        assert!(root.ends_with("Workspaces"));
    }

    #[test]
    fn resolve_voice_command_script_path_prefers_env_override() {
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

        unsafe {
            std::env::remove_var("YUICLAW_WORKSPACES_ROOT");
            std::env::remove_var("YUICLAW_VOICE_COMMAND_OPERATOR_SHELL");
        }
    }
}
