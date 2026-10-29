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
        let mut closing = Closing::new();
        while server.is_running() {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => break,
                _ = closing.wait() => {
                    tracing::info!("the window is closing: saving");
                    break;
                }
                _ = tokio::time::sleep(std::time::Duration::from_millis(500)) => {}
            }
        }
    });
    if server.stop() {
        // A non-zero exit, so a restart script can tell a crash from a stop.
        tracing::error!("stopped after an error (the world was saved)");
        std::process::exit(1);
    }
    tracing::info!("stopped");
    Ok(())
}

/// Windows closes a console program's window (the X button, logging off,
/// shutting down) with its own events, not Ctrl+C: catch them too, so the
/// world is saved (Windows waits a few seconds before ending the process).
struct Closing {
    #[cfg(windows)]
    signals: Option<(
        tokio::signal::windows::CtrlClose,
        tokio::signal::windows::CtrlBreak,
        tokio::signal::windows::CtrlShutdown,
        tokio::signal::windows::CtrlLogoff,
    )>,
}

impl Closing {
    fn new() -> Self {
        #[cfg(windows)]
        {
            use tokio::signal::windows::*;
            let signals = (|| {
                Some((
                    ctrl_close().ok()?,
                    ctrl_break().ok()?,
                    ctrl_shutdown().ok()?,
                    ctrl_logoff().ok()?,
                ))
            })();
            Closing { signals }
        }
        #[cfg(not(windows))]
        Closing {}
    }

    async fn wait(&mut self) {
        #[cfg(windows)]
        if let Some((close, brk, shut, logoff)) = &mut self.signals {
            tokio::select! {
                _ = close.recv() => {}
                _ = brk.recv() => {}
                _ = shut.recv() => {}
                _ = logoff.recv() => {}
            }
            return;
        }
        std::future::pending::<()>().await
    }
}
