mod cli;
mod components;
mod env;
mod init;
mod process;
mod status;

use clap::Parser;
use cli::{Cli, Commands, DaemonCommands};

#[tokio::main]
async fn main() {
    // Load ~/.config/yuiclaw/.env before anything else so that adapter tokens
    // and other settings are available for all subcommands.
    env::load_config_dotenv();

    let cli = Cli::parse();

    // デフォルト（引数なし）は start と同等
    let command = cli.command.unwrap_or(Commands::Start {
        provider: "Gemini".to_string(),
    });

    let result = match command {
        Commands::Daemon { action } => match action {
            DaemonCommands::Start => process::daemon_start().await,
            DaemonCommands::Status { json } => status::show_daemon_status(json).await,
            DaemonCommands::Stop => process::daemon_stop().await,
            DaemonCommands::Restart => process::daemon_restart().await,
        },
        Commands::Start { provider } => process::start_stack(&provider).await,
        // Provider shorthand subcommands — map to start_stack_with_opts
        Commands::Gemini { new } => process::start_stack_with_opts("Gemini", new).await,
        Commands::Claude { new } => process::start_stack_with_opts("Claude", new).await,
        Commands::Codex { new } => process::start_stack_with_opts("Codex", new).await,
        Commands::Opencode { new } => process::start_stack_with_opts("OpenCode", new).await,
        Commands::Dummy { new } => process::start_stack_with_opts("Dummy", new).await,
        Commands::Stop => process::stop_bridge().await,
        Commands::Restart => process::restart_stack().await,
        Commands::Status => status::show_status().await,
        Commands::Init => init::initialize().await,
        Commands::Tick => process::run_tick().await,
        Commands::Pub { message, channel } => process::publish(&message, channel.as_deref()).await,
        Commands::Reset => process::reset_session().await,
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
