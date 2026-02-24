use crate::components;
use std::process::Stdio;
use tokio::process::Command;

/// yuiclaw システム全体を初期化する
pub async fn initialize() -> Result<(), Box<dyn std::error::Error>> {
    let s = components::detect().await;

    println!("=== YuiClaw Initialization ===");
    println!();

    // 1. amem init
    if s.amem_available {
        println!("[1/3] amem を初期化しています...");
        let ok = Command::new("amem")
            .arg("init")
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false);
        println!(
            "  {}",
            if ok {
                "✓ amem initialized"
            } else {
                "✗ amem init failed (continuing)"
            }
        );
    } else {
        println!("[1/3] amem が見つかりません — スキップ");
    }

    // 2. abeat init
    if s.abeat_available {
        println!("[2/3] abeat を初期化しています...");
        let ok = Command::new("abeat")
            .arg("init")
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false);
        println!(
            "  {}",
            if ok {
                "✓ abeat initialized"
            } else {
                "✗ abeat init failed (continuing)"
            }
        );

        // 3. デフォルトのハートビートジョブを設定
        println!("[3/3] ハートビートジョブを設定しています...");
        setup_heartbeat_job().await;
    } else {
        println!("[2/3] abeat が見つかりません — スキップ");
        println!("[3/3] abeat なしのためジョブ設定をスキップ");
    }

    println!();
    println!("初期化完了。`yuiclaw start` でシステムを起動してください。");
    Ok(())
}

/// yuiclaw-heartbeat ジョブを abeat に登録する
async fn setup_heartbeat_job() {
    // ジョブが既に存在するか確認
    let exists = Command::new("abeat")
        .args(["get", "job", "yuiclaw-heartbeat"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false);

    if exists {
        println!("  ✓ heartbeat job は既に存在します");
        return;
    }

    // ホームディレクトリを workspace として使用
    let home = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .to_string_lossy()
        .to_string();

    // abeat set jobs add で登録
    // acomm が利用可能であれば結果を bridge に publish する
    let exec_cmd = [
        "if command -v acomm >/dev/null 2>&1 && test -S /tmp/acomm.sock; then",
        "  acomm --publish 'Proactive heartbeat: review recent amem activities and provide a brief status update.' --channel heartbeat 2>/dev/null;",
        "else",
        "  echo HEARTBEAT_OK;",
        "fi",
    ]
    .join(" ");

    let ok = Command::new("abeat")
        .args([
            "set",
            "jobs",
            "add",
            "--id",
            "yuiclaw-heartbeat",
            "--description",
            "YuiClaw 30分ごとのプロアクティブチェック",
            "--kind",
            "heartbeat_check",
            "--every",
            "30m",
            "--agent",
            "shell",
            "--workspace",
            &home,
            "--exec",
            &exec_cmd,
            "--no-op-token",
            "HEARTBEAT_OK",
        ])
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false);

    println!(
        "  {}",
        if ok {
            "✓ yuiclaw-heartbeat job を登録しました"
        } else {
            "✗ heartbeat job の登録に失敗しました (abeat set jobs add が未実装の可能性あり)"
        }
    );
}
