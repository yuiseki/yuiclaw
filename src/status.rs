use crate::components::{self, SOCKET_PATH};

/// 全コンポーネントのステータスをターミナルに表示する
pub async fn show_status() -> Result<(), Box<dyn std::error::Error>> {
    let s = components::detect().await;

    println!("=== YuiClaw Status ===");
    println!();

    println!("[Components]");
    println!(
        "  amem  : {}",
        if s.amem_available { "✓ available" } else { "✗ not found in PATH" }
    );
    println!(
        "  abeat : {}",
        if s.abeat_available { "✓ available" } else { "✗ not found in PATH" }
    );
    println!(
        "  acomm : {}",
        if s.acomm_available { "✓ available" } else { "✗ not found in PATH" }
    );
    println!();

    println!("[Bridge]");
    if s.bridge_running {
        println!("  Socket: ✓ running ({})", SOCKET_PATH);
    } else {
        println!("  Socket: ✗ not running  (run `yuiclaw start` to launch)");
    }
    println!();

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
