//! factory-edge-agent — runs at a machine, drives it, talks to the gateway.
//!
//! ```text
//! factory-edge-agent --config agent.toml
//! ```

use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env().add_directive("factory_edge_agent=info".parse()?),
        )
        .init();

    let config_path = std::env::args()
        .skip_while(|a| a != "--config")
        .nth(1)
        .unwrap_or_else(|| "agent.toml".to_owned());

    let config = factory_edge_agent::config::AgentConfig::load(std::path::Path::new(&config_path))?;
    factory_edge_agent::run(config).await
}
