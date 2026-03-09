use std::process::Command;

fn repo_root() -> &'static str {
    env!("CARGO_MANIFEST_DIR")
}

fn make_dry_run(target: &str) -> String {
    let output = Command::new("make")
        .arg("-n")
        .arg(target)
        .current_dir(repo_root())
        .output()
        .unwrap_or_else(|err| panic!("failed to run make -n {target}: {err}"));
    assert!(output.status.success(), "make -n {target} should exit 0");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn assert_submodule_init_precedes_command(target: &str, expected_command: &str) {
    let stdout = make_dry_run(target);
    let submodule_cmd = "git submodule update --init --recursive";
    let submodule_pos = stdout
        .find(submodule_cmd)
        .unwrap_or_else(|| panic!("{target} should include `{submodule_cmd}`; got:\n{stdout}"));
    let install_pos = stdout.find(expected_command).unwrap_or_else(|| {
        panic!("{target} should include `{expected_command}`; got:\n{stdout}")
    });
    assert!(
        submodule_pos < install_pos,
        "{target} should run `{submodule_cmd}` before `{expected_command}`; got:\n{stdout}"
    );
}

#[test]
fn install_targets_refresh_submodules_before_installing() {
    let cases = [
        ("install-acore", "cargo install --path deps/acore"),
        ("install-amem", "cargo install --path deps/amem"),
        ("install-abeat", "cargo install --path deps/abeat"),
        ("install-acomm", "cargo install --path deps/acomm"),
        (
            "install-acomm-tui",
            "cd deps/acomm/tui && npm install --legacy-peer-deps",
        ),
    ];

    for (target, expected_command) in cases {
        assert_submodule_init_precedes_command(target, expected_command);
    }
}
