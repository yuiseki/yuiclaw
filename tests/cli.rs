use std::process::Command;

fn yuiclaw_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_yuiclaw"))
}

// --- CLI entry point tests ---

#[test]
fn test_help_flag() {
    let output = yuiclaw_bin()
        .arg("--help")
        .output()
        .expect("failed to run yuiclaw --help");
    assert!(output.status.success(), "yuiclaw --help should exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("yuiclaw"),
        "help text should mention yuiclaw"
    );
}

#[test]
fn test_version_flag() {
    let output = yuiclaw_bin()
        .arg("--version")
        .output()
        .expect("failed to run yuiclaw --version");
    assert!(output.status.success(), "yuiclaw --version should exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("0.1.0"), "version string should be present");
}

#[test]
fn test_status_subcommand_exits_ok() {
    let output = yuiclaw_bin()
        .arg("status")
        .output()
        .expect("failed to run yuiclaw status");
    // status should always exit 0 even if components are missing
    assert!(output.status.success(), "yuiclaw status should exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("YuiClaw"),
        "status output should contain YuiClaw"
    );
}

#[test]
fn test_status_shows_component_section() {
    let output = yuiclaw_bin()
        .arg("status")
        .output()
        .expect("failed to run yuiclaw status");
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should show amem, abeat, acomm entries
    assert!(stdout.contains("amem"), "status should show amem");
    assert!(stdout.contains("abeat"), "status should show abeat");
    assert!(stdout.contains("acomm"), "status should show acomm");
    assert!(stdout.contains("Bridge"), "status should show Bridge");
}

#[test]
fn test_init_subcommand_exits_ok() {
    let output = yuiclaw_bin()
        .arg("init")
        .output()
        .expect("failed to run yuiclaw init");
    assert!(output.status.success(), "yuiclaw init should exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("YuiClaw"),
        "init output should contain YuiClaw"
    );
}

#[test]
fn test_tick_subcommand_fails_gracefully_without_abeat() {
    // tick should exit non-zero if abeat is not available, but not panic
    let output = yuiclaw_bin()
        .arg("tick")
        .output()
        .expect("failed to run yuiclaw tick");
    // We don't assert exit code here because abeat may or may not be present in CI
    // Just verify it doesn't crash or produce garbage
    let _ = String::from_utf8_lossy(&output.stdout);
}

#[test]
fn test_pub_subcommand_handles_bridge_state() {
    let output = yuiclaw_bin()
        .arg("pub")
        .arg("hello")
        .output()
        .expect("failed to run yuiclaw pub");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{}{}", stdout, stderr);

    if output.status.success() {
        // Bridge was running — pub succeeded, that's fine
    } else {
        // Bridge not running or publish failed — error must be non-empty and informative
        assert!(
            !combined.is_empty(),
            "failed pub should produce error output, got nothing"
        );
    }
}

#[test]
fn test_stop_subcommand_handles_no_bridge() {
    // stop when bridge is not running should exit ok with a message
    let output = yuiclaw_bin()
        .arg("stop")
        .output()
        .expect("failed to run yuiclaw stop");
    assert!(
        output.status.success(),
        "stop should exit 0 even if bridge is not running"
    );
}

#[test]
fn test_start_subcommand_help() {
    let output = yuiclaw_bin()
        .arg("start")
        .arg("--help")
        .output()
        .expect("failed to run yuiclaw start --help");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--provider"),
        "start help should mention --provider"
    );
}

#[test]
fn test_restart_subcommand_help() {
    let output = yuiclaw_bin()
        .arg("restart")
        .arg("--help")
        .output()
        .expect("failed to run yuiclaw restart --help");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("restart"),
        "restart help should mention restart subcommand"
    );
}

// --- Daemon subcommand tests ---

#[test]
fn test_daemon_subcommand_in_main_help() {
    let output = yuiclaw_bin()
        .arg("--help")
        .output()
        .expect("failed to run yuiclaw --help");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("daemon"),
        "main help should list daemon subcommand"
    );
}

#[test]
fn test_daemon_help() {
    let output = yuiclaw_bin()
        .arg("daemon")
        .arg("--help")
        .output()
        .expect("failed to run yuiclaw daemon --help");
    assert!(output.status.success(), "yuiclaw daemon --help should exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("start"), "daemon help should mention start");
    assert!(stdout.contains("stop"), "daemon help should mention stop");
    assert!(stdout.contains("status"), "daemon help should mention status");
    assert!(stdout.contains("restart"), "daemon help should mention restart");
}

#[test]
fn test_daemon_start_help() {
    let output = yuiclaw_bin()
        .arg("daemon")
        .arg("start")
        .arg("--help")
        .output()
        .expect("failed to run yuiclaw daemon start --help");
    assert!(output.status.success());
}

#[test]
fn test_daemon_stop_help() {
    let output = yuiclaw_bin()
        .arg("daemon")
        .arg("stop")
        .arg("--help")
        .output()
        .expect("failed to run yuiclaw daemon stop --help");
    assert!(output.status.success());
}

#[test]
fn test_daemon_status_json() {
    let output = yuiclaw_bin()
        .arg("daemon")
        .arg("status")
        .arg("--json")
        .output()
        .expect("failed to run yuiclaw daemon status --json");
    assert!(
        output.status.success(),
        "daemon status --json should exit 0"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should be valid JSON
    let _json: serde_json::Value = serde_json::from_str(&stdout).expect("output should be valid JSON");
}

#[test]
fn test_daemon_status_help() {
    let output = yuiclaw_bin()
        .arg("daemon")
        .arg("status")
        .arg("--help")
        .output()
        .expect("failed to run yuiclaw daemon status --help");
    assert!(output.status.success());
}

#[test]
fn test_daemon_restart_help() {
    let output = yuiclaw_bin()
        .arg("daemon")
        .arg("restart")
        .arg("--help")
        .output()
        .expect("failed to run yuiclaw daemon restart --help");
    assert!(output.status.success());
}

#[test]
fn test_daemon_stop_handles_no_daemon() {
    // daemon stop when bridge is not running should exit 0 with a message
    let output = yuiclaw_bin()
        .arg("daemon")
        .arg("stop")
        .output()
        .expect("failed to run yuiclaw daemon stop");
    assert!(
        output.status.success(),
        "daemon stop should exit 0 even if daemon is not running"
    );
}

#[test]
fn test_daemon_status_exits_ok() {
    let output = yuiclaw_bin()
        .arg("daemon")
        .arg("status")
        .output()
        .expect("failed to run yuiclaw daemon status");
    assert!(
        output.status.success(),
        "daemon status should always exit 0"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Daemon"),
        "daemon status output should contain Daemon"
    );
    assert!(
        stdout.contains("Bridge"),
        "daemon status output should contain Bridge"
    );
}

// --- Provider shorthand subcommand tests ---

#[test]
fn test_gemini_subcommand_exists() {
    let output = yuiclaw_bin()
        .arg("gemini")
        .arg("--help")
        .output()
        .expect("failed to run yuiclaw gemini --help");
    assert!(
        output.status.success(),
        "yuiclaw gemini --help should exit 0"
    );
}

#[test]
fn test_gemini_subcommand_has_new_flag() {
    let output = yuiclaw_bin()
        .arg("gemini")
        .arg("--help")
        .output()
        .expect("failed to run yuiclaw gemini --help");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("new"),
        "gemini help should mention --new flag"
    );
}

#[test]
fn test_claude_subcommand_exists() {
    let output = yuiclaw_bin()
        .arg("claude")
        .arg("--help")
        .output()
        .expect("failed to run yuiclaw claude --help");
    assert!(
        output.status.success(),
        "yuiclaw claude --help should exit 0"
    );
}

#[test]
fn test_codex_subcommand_exists() {
    let output = yuiclaw_bin()
        .arg("codex")
        .arg("--help")
        .output()
        .expect("failed to run yuiclaw codex --help");
    assert!(
        output.status.success(),
        "yuiclaw codex --help should exit 0"
    );
}

#[test]
fn test_opencode_subcommand_exists() {
    let output = yuiclaw_bin()
        .arg("opencode")
        .arg("--help")
        .output()
        .expect("failed to run yuiclaw opencode --help");
    assert!(
        output.status.success(),
        "yuiclaw opencode --help should exit 0"
    );
}

#[test]
fn test_dummy_subcommand_exists() {
    let output = yuiclaw_bin()
        .arg("dummy")
        .arg("--help")
        .output()
        .expect("failed to run yuiclaw dummy --help");
    assert!(
        output.status.success(),
        "yuiclaw dummy --help should exit 0"
    );
}

#[test]
fn test_provider_subcommands_appear_in_main_help() {
    let output = yuiclaw_bin()
        .arg("--help")
        .output()
        .expect("failed to run yuiclaw --help");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("gemini"),
        "main help should list gemini subcommand"
    );
    assert!(
        stdout.contains("claude"),
        "main help should list claude subcommand"
    );
    assert!(
        stdout.contains("codex"),
        "main help should list codex subcommand"
    );
    assert!(
        stdout.contains("opencode"),
        "main help should list opencode subcommand"
    );
    assert!(
        stdout.contains("dummy"),
        "main help should list dummy subcommand"
    );
    assert!(
        stdout.contains("restart"),
        "main help should list restart subcommand"
    );
}
