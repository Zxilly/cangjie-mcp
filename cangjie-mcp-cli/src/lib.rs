pub mod cli;
pub mod client;
pub mod config;
pub mod daemon;

use std::io::IsTerminal;
use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;
use rmcp::ServiceExt;
use tracing::info;

use cangjie_core::config::Settings;
use cangjie_core::logging::setup_logging;
use cangjie_indexer::search::LocalSearchIndex;

use cli::{CangjieArgs, Commands, ConfigAction, DaemonAction};

pub async fn run() -> ExitCode {
    // Load config file to env BEFORE clap parsing (priority: CLI > env > config > defaults)
    config::load_config_to_env();
    let args = CangjieArgs::parse();

    // Daemon serve mode: log to daemon.log instead of stderr
    let is_daemon_serve = matches!(args.command, Some(Commands::Serve));
    if is_daemon_serve {
        let log_path = daemon::paths::log_file();
        setup_logging(Some(log_path.as_path()), args.debug);
    } else {
        setup_logging(args.log_file.as_deref(), args.debug);
    }

    let result = match args.command {
        Some(Commands::Serve) => {
            let settings = config::settings_from_env();
            daemon::server::run_daemon(settings, args.daemon_timeout).await
        }
        Some(Commands::Index) => run_index(args.server.to_settings()).await,
        Some(Commands::Daemon { action }) => run_daemon_action(action),
        Some(Commands::Config { action }) => run_config_action(action),
        Some(ref cmd) => run_tool_command(cmd, args.daemon_timeout).await,
        None => run_mcp_server(args.server.to_settings()).await,
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            cli::output::print_error(&format!("{e:#}"));
            ExitCode::FAILURE
        }
    }
}

const MCP_INIT_TIMEOUT_SECS: u64 = 5;

fn mcp_not_interactive(preamble: &str) -> anyhow::Error {
    eprintln!("cangjie-mcp: {preamble}");
    eprintln!("This command starts an MCP stdio server for AI coding assistants.");
    eprintln!(
        "It communicates via stdin/stdout and is not meant to be run directly in a terminal."
    );
    eprintln!();
    eprintln!("To use with an AI assistant, see: https://github.com/Zxilly/cangjie-mcp#快速配置");
    eprintln!("For CLI usage, try: cangjie-mcp query \"泛型\"");
    anyhow::anyhow!("not an MCP client")
}

async fn run_mcp_server(settings: Settings) -> Result<()> {
    if std::io::stdin().is_terminal() {
        return Err(mcp_not_interactive(
            "stdin is a terminal — no MCP client detected.",
        ));
    }

    if settings.server_url.is_some() {
        info!("Using remote server - local index options are ignored.");
    }

    let server = cangjie_server::CangjieServer::new(settings);

    let server_clone = server.clone();
    let init_handle = tokio::spawn(async move {
        if let Err(e) = server_clone.initialize().await {
            tracing::error!("Failed to initialize server: {e}");
        }
    });

    info!("Starting MCP server on stdio...");
    let service = match tokio::time::timeout(
        std::time::Duration::from_secs(MCP_INIT_TIMEOUT_SECS),
        server.serve(rmcp::transport::stdio()),
    )
    .await
    {
        Ok(result) => result.map_err(|e| anyhow::anyhow!("Failed to start MCP server: {e}"))?,
        Err(_) => {
            init_handle.abort();
            return Err(mcp_not_interactive(&format!(
                "No MCP initialize request received within {MCP_INIT_TIMEOUT_SECS} seconds."
            )));
        }
    };
    service.waiting().await?;

    Ok(())
}

async fn run_index(settings: Settings) -> Result<()> {
    info!(
        "Building index (version={}, lang={})...",
        settings.docs_version, settings.docs_lang
    );

    let mut search_index = LocalSearchIndex::new(settings.clone()).await;
    let index_info = search_index.init().await?;

    cangjie_core::config::log_startup_info(&settings, &index_info);
    info!("Index built successfully.");

    Ok(())
}

fn run_daemon_action(action: DaemonAction) -> Result<()> {
    match action {
        DaemonAction::Stop => daemon::stop_daemon(),
        DaemonAction::Status => daemon::daemon_status(),
        DaemonAction::Logs { tail, follow } => daemon::daemon_logs(tail, follow),
    }
}

fn run_config_action(action: ConfigAction) -> Result<()> {
    match action {
        ConfigAction::Path => {
            println!("{}", config::config_file().display());
            Ok(())
        }
        ConfigAction::Init => {
            let path = config::config_file();
            if path.exists() {
                anyhow::bail!("Config file already exists at {}", path.display());
            }
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&path, config::generate_default_config())?;
            println!("Created config file at {}", path.display());
            Ok(())
        }
    }
}

async fn run_tool_command(cmd: &Commands, daemon_timeout: u64) -> Result<()> {
    let params = cli::commands::command_to_tool_call(cmd)
        .ok_or_else(|| anyhow::anyhow!("internal error: not a tool command"))?;

    daemon::ensure_running(daemon_timeout).await?;

    let result = client::call_tool(params).await?;

    if result.is_error.unwrap_or(false) {
        cli::output::print_tool_result(&result, cmd);
        anyhow::bail!("tool returned an error");
    }

    cli::output::print_tool_result(&result, cmd);
    Ok(())
}
