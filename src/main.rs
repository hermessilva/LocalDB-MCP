mod config;
mod discovery;
mod localdb;
mod mcp;
mod security;
mod sql;

use anyhow::Result;
use rmcp::{ServiceExt, transport::stdio};
use tracing_subscriber::EnvFilter;

use config::Config;
use mcp::McpServer;

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::load()?;

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_new(&config.log_level).unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    tracing::info!("iniciando mssql-localdb-mcp");

    let server = McpServer::new(config)
        .serve(stdio())
        .await
        .inspect_err(|e| tracing::error!("erro ao subir servidor MCP: {e:?}"))?;

    server.waiting().await?;
    Ok(())
}
