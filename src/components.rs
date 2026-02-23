use std::path::Path;
use tokio::process::Command;

/// acomm bridge の Unix ソケットパス (acomm 内部と一致させる)
pub const SOCKET_PATH: &str = "/tmp/acomm.sock";

/// 各コンポーネントの稼働状況
pub struct ComponentStatus {
    pub amem_available: bool,
    pub abeat_available: bool,
    pub acomm_available: bool,
    pub bridge_running: bool,
}

/// 全コンポーネントの状態を確認する
pub async fn detect() -> ComponentStatus {
    // amem / abeat / acomm の存在確認は並行して実行
    let (amem, abeat, acomm) = tokio::join!(
        is_command_available("amem"),
        is_command_available("abeat"),
        is_command_available("acomm"),
    );
    ComponentStatus {
        amem_available: amem,
        abeat_available: abeat,
        acomm_available: acomm,
        bridge_running: is_bridge_running(),
    }
}

/// コマンドが PATH 上に存在するか確認する (`which` を使用)
pub async fn is_command_available(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}

/// acomm bridge ソケットが存在するか確認する
pub fn is_bridge_running() -> bool {
    Path::new(SOCKET_PATH).exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bridge_not_running_for_random_path() {
        // ランダムなパスは存在しない
        assert!(!Path::new("/tmp/yuiclaw_nonexistent_socket_test_xyz.sock").exists());
    }

    #[tokio::test]
    async fn test_detect_returns_struct_without_panic() {
        let status = detect().await;
        // パニックしないことを確認。値は環境依存なので assert しない
        let _ = status.amem_available;
        let _ = status.abeat_available;
        let _ = status.acomm_available;
        let _ = status.bridge_running;
    }

    #[tokio::test]
    async fn test_is_command_available_for_nonexistent() {
        // 存在しないコマンドは false を返す
        assert!(!is_command_available("__yuiclaw_nonexistent_cmd__").await);
    }

    #[tokio::test]
    async fn test_is_command_available_for_sh() {
        // sh は必ず存在する
        assert!(is_command_available("sh").await);
    }
}
