use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "yuiclaw",
    version,
    about = "YuiClaw — AI執事システム (amem + abeat + acomm + acore)"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum DaemonCommands {
    /// デーモン (bridge + adapters) をバックグラウンドで起動する
    Start,
    /// デーモンのステータスを表示する
    Status {
        /// ステータスを JSON 形式で出力する
        #[arg(long)]
        json: bool,
    },
    /// デーモンを停止する
    Stop,
    /// デーモンを再起動する
    Restart,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// デーモン (bridge + adapters) をバックグラウンドで管理する
    Daemon {
        #[command(subcommand)]
        action: DaemonCommands,
    },
    /// フルスタックを起動する (bridge + TUI)
    Start {
        /// 使用するAIプロバイダー [Gemini|Claude|Codex|OpenCode|Dummy]
        #[arg(short, long, default_value = "Gemini")]
        provider: String,
    },
    /// Gemini プロバイダーで起動する (前回セッションがある場合は復元)
    Gemini {
        /// 既存セッションを破棄して新しいセッションで起動する
        #[arg(long)]
        new: bool,
    },
    /// Claude プロバイダーで起動する (前回セッションがある場合は復元)
    Claude {
        /// 既存セッションを破棄して新しいセッションで起動する
        #[arg(long)]
        new: bool,
    },
    /// Codex プロバイダーで起動する (前回セッションがある場合は復元)
    Codex {
        /// 既存セッションを破棄して新しいセッションで起動する
        #[arg(long)]
        new: bool,
    },
    /// OpenCode プロバイダーで起動する (前回セッションがある場合は復元)
    Opencode {
        /// 既存セッションを破棄して新しいセッションで起動する
        #[arg(long)]
        new: bool,
    },
    /// Dummy (echo bot) プロバイダーで起動する（TUI動作確認用）
    Dummy {
        /// 既存セッションを破棄して新しいセッションで起動する
        #[arg(long)]
        new: bool,
    },
    /// acomm bridge を停止する
    Stop,
    /// acomm bridge を再起動する (TUIは起動しない)
    Restart,
    /// 全コンポーネントのステータスを表示する
    Status,
    /// yuiclaw システムを初期化する (amem init + abeat init + デフォルトジョブ)
    Init,
    /// abeat の期限切れジョブを実行する
    Tick,
    /// 実行中の bridge にメッセージを送信する
    Pub {
        /// 送信するメッセージ
        message: String,
        /// チャンネル名 (省略可)
        #[arg(short, long)]
        channel: Option<String>,
    },
    /// 実行中の対話セッションをリセットする (会話履歴・エージェントセッションをクリア)
    Reset,
}
