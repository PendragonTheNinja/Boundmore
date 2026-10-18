use bm_server::config::ServerConfig;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,quinn=warn".into()),
        )
        .init();
    let cfg = ServerConfig::load(std::env::args().skip(1))?;
    let server = bm_server::start(cfg)?;
    tracing::info!("press Ctrl+C to save and stop");
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    rt.block_on(async {
        while server.is_running() {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => break,
                _ = tokio::time::sleep(std::time::Duration::from_millis(500)) => {}
            }
        }
    });
    server.stop();
    tracing::info!("stopped");
    Ok(())
}
