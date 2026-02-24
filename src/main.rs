mod cli;
mod components;
mod init;
mod process;
mod status;

use clap::Parser;
use cli::{Cli, Commands};

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // デフォルト（引数なし）は start と同等
    let command = cli.command.unwrap_or(Commands::Start {
        provider: "Gemini".to_string(),
    });

    let result = match command {
        Commands::Start { provider } => process::start_stack(&provider).await,
        // Provider shorthand subcommands — map to start_stack_with_opts
        Commands::Gemini { new } => process::start_stack_with_opts("Gemini", new).await,
        Commands::Claude { new } => process::start_stack_with_opts("Claude", new).await,
        Commands::Codex { new } => process::start_stack_with_opts("Codex", new).await,
        Commands::Opencode { new } => process::start_stack_with_opts("OpenCode", new).await,
        Commands::Stop => process::stop_bridge().await,
        Commands::Status => status::show_status().await,
        Commands::Init => init::initialize().await,
        Commands::Tick => process::run_tick().await,
        Commands::Pub { message, channel } => {
            process::publish(&message, channel.as_deref()).await
        }
        Commands::Reset => process::reset_session().await,
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
