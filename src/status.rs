use crate::components::{self, SOCKET_PATH};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy)]
struct ChannelSpec {
    label: &'static str,
    env_keys: &'static [&'static str],
    adapter_flag: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ChannelStatus {
    label: &'static str,
    connected: bool,
}

const CHANNEL_SPECS: [ChannelSpec; 3] = [
    ChannelSpec {
        label: "ntfy",
        env_keys: &["NTFY_TOPIC"],
        adapter_flag: "--ntfy",
    },
    ChannelSpec {
        label: "Discord",
        env_keys: &["DISCORD_BOT_TOKEN"],
        adapter_flag: "--discord",
    },
    ChannelSpec {
        label: "Slack",
        env_keys: &["SLACK_APP_TOKEN", "SLACK_BOT_TOKEN"],
        adapter_flag: "--slack",
    },
];

/// 全コンポーネントのステータスをターミナルに表示する
pub async fn show_status() -> Result<(), Box<dyn std::error::Error>> {
    let s = components::detect().await;
    let channels = detect_channel_statuses(s.bridge_running).await;

    println!("=== YuiClaw Status ===");
    println!();

    println!("[Components]");
    println!(
        "  amem  : {}",
        if s.amem_available {
            "✓ available"
        } else {
            "✗ not found in PATH"
        }
    );
    println!(
        "  abeat : {}",
        if s.abeat_available {
            "✓ available"
        } else {
            "✗ not found in PATH"
        }
    );
    println!(
        "  acomm : {}",
        if s.acomm_available {
            "✓ available"
        } else {
            "✗ not found in PATH"
        }
    );
    println!();

    println!("[Bridge]");
    if s.bridge_running {
        println!("  Socket: ✓ running ({})", SOCKET_PATH);
    } else {
        println!("  Socket: ✗ not running  (run `yuiclaw start` to launch)");
    }
    println!();

    if !channels.is_empty() {
        println!("[Channels]");
        for ch in channels {
            println!(
                "  {:<7}: {}",
                ch.label,
                if ch.connected {
                    "✓ connected"
                } else {
                    "✗ not connected"
                }
            );
        }
        println!();
    }

    if s.abeat_available {
        println!("[Scheduled Jobs]");
        match tokio::process::Command::new("abeat")
            .arg("list")
            .output()
            .await
        {
            Ok(out) if out.status.success() => {
                let text = String::from_utf8_lossy(&out.stdout);
                for line in text.lines() {
                    println!("  {}", line);
                }
            }
            _ => println!("  (no jobs configured — run `yuiclaw init` to set up defaults)"),
        }
        println!();
    }

    if s.amem_available {
        println!("[Memory]");
        match tokio::process::Command::new("amem")
            .arg("which")
            .output()
            .await
        {
            Ok(out) if out.status.success() => {
                let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
                println!("  Root: {}", path);
            }
            _ => println!("  (amem which failed)"),
        }
        println!();
    }

    Ok(())
}

async fn detect_channel_statuses(bridge_running: bool) -> Vec<ChannelStatus> {
    let present_env_keys = present_nonempty_env_keys();
    let process_list = read_process_list().await.unwrap_or_default();

    channel_statuses_from_inputs(&present_env_keys, &process_list, bridge_running)
}

fn present_nonempty_env_keys() -> HashSet<String> {
    std::env::vars()
        .filter_map(|(k, v)| if v.trim().is_empty() { None } else { Some(k) })
        .collect()
}

async fn read_process_list() -> Option<String> {
    let out = tokio::process::Command::new("ps")
        .args(["-eo", "comm=,args="])
        .output()
        .await
        .ok()?;

    if !out.status.success() {
        return None;
    }

    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn channel_statuses_from_inputs(
    present_env_keys: &HashSet<String>,
    process_list: &str,
    bridge_running: bool,
) -> Vec<ChannelStatus> {
    CHANNEL_SPECS
        .iter()
        .filter(|spec| is_channel_configured(spec, present_env_keys))
        .map(|spec| ChannelStatus {
            label: spec.label,
            connected: bridge_running && process_list_has_adapter(process_list, spec.adapter_flag),
        })
        .collect()
}

fn is_channel_configured(spec: &ChannelSpec, present_env_keys: &HashSet<String>) -> bool {
    spec.env_keys.iter().all(|k| present_env_keys.contains(*k))
}

fn process_list_has_adapter(process_list: &str, adapter_flag: &str) -> bool {
    process_list
        .lines()
        .any(|line| process_line_matches_adapter(line, adapter_flag))
}

fn process_line_matches_adapter(line: &str, adapter_flag: &str) -> bool {
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

    args.split_whitespace().any(|token| token == adapter_flag)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_keys(keys: &[&str]) -> HashSet<String> {
        keys.iter().map(|k| (*k).to_string()).collect()
    }

    #[test]
    fn hides_unconfigured_channels() {
        let rows = channel_statuses_from_inputs(&env_keys(&[]), "", true);
        assert!(rows.is_empty());
    }

    #[test]
    fn shows_configured_channel_as_not_connected_when_adapter_process_missing() {
        let rows = channel_statuses_from_inputs(&env_keys(&["DISCORD_BOT_TOKEN"]), "", true);
        assert_eq!(
            rows,
            vec![ChannelStatus {
                label: "Discord",
                connected: false
            }]
        );
    }

    #[test]
    fn marks_channel_connected_only_when_bridge_and_adapter_process_are_running() {
        let ps_output = "acomm           acomm --discord\n";
        let rows = channel_statuses_from_inputs(&env_keys(&["DISCORD_BOT_TOKEN"]), ps_output, true);
        assert_eq!(
            rows,
            vec![ChannelStatus {
                label: "Discord",
                connected: true
            }]
        );

        let rows_without_bridge =
            channel_statuses_from_inputs(&env_keys(&["DISCORD_BOT_TOKEN"]), ps_output, false);
        assert_eq!(
            rows_without_bridge,
            vec![ChannelStatus {
                label: "Discord",
                connected: false
            }]
        );
    }

    #[test]
    fn slack_requires_both_tokens_to_be_configured() {
        let ps_output = "acomm           acomm --slack\n";
        let missing_bot =
            channel_statuses_from_inputs(&env_keys(&["SLACK_APP_TOKEN"]), ps_output, true);
        assert!(missing_bot.is_empty());

        let configured = channel_statuses_from_inputs(
            &env_keys(&["SLACK_APP_TOKEN", "SLACK_BOT_TOKEN"]),
            ps_output,
            true,
        );
        assert_eq!(
            configured,
            vec![ChannelStatus {
                label: "Slack",
                connected: true
            }]
        );
    }

    #[test]
    fn process_match_requires_acomm_executable_and_exact_flag() {
        assert!(process_line_matches_adapter(
            "acomm           /home/user/.cargo/bin/acomm --ntfy",
            "--ntfy"
        ));
        assert!(!process_line_matches_adapter(
            "cargo           cargo run -p acomm -- --ntfy",
            "--ntfy"
        ));
        assert!(!process_line_matches_adapter(
            "acomm           acomm --notify",
            "--ntfy"
        ));
    }
}
