use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    cangjie_mcp_cli::run().await
}
