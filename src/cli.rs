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
pub enum Commands {
    /// フルスタックを起動する (bridge + TUI)
    Start {
        /// 使用するAIツール [gemini|claude|codex|opencode]
        #[arg(short, long, default_value = "gemini")]
        tool: String,
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
    /// acomm bridge を停止する
    Stop,
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
