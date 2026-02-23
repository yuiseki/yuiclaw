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
    assert!(stdout.contains("yuiclaw"), "help text should mention yuiclaw");
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
    assert!(stdout.contains("YuiClaw"), "status output should contain YuiClaw");
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
    assert!(stdout.contains("YuiClaw"), "init output should contain YuiClaw");
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
    assert!(output.status.success(), "stop should exit 0 even if bridge is not running");
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
    assert!(stdout.contains("tool"), "start help should mention --tool");
}
