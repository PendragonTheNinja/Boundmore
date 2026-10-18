//! Boundmore client.
//!
//! `boundmore --server HOST:PORT --name NAME --cert PATH`

mod app;
mod net;
mod predict;
mod render;
mod world;

use anyhow::{Context, bail};
use std::net::ToSocketAddrs;
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,wgpu_core=warn,wgpu_hal=warn,naga=warn,quinn=warn".into()),
        )
        .init();

    let mut server = "127.0.0.1:7777".to_string();
    let mut name = "Player".to_string();
    let mut cert = PathBuf::from("data").join(bm_net::CERT_FILE);
    let mut vsync = true;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut val = || args.next().with_context(|| format!("{a} needs a value"));
        match a.as_str() {
            "--server" => server = val()?,
            "--name" => name = val()?,
            "--cert" => cert = PathBuf::from(val()?),
            "--no-vsync" => vsync = false,
            "--help" | "-h" => {
                println!("boundmore [--server HOST:PORT] [--name NAME] [--cert server.cert.der] [--no-vsync]");
                return Ok(());
            }
            other => bail!("unknown argument {other}"),
        }
    }
    let addr = server
        .to_socket_addrs()
        .with_context(|| format!("cannot resolve {server}"))?
        .next()
        .with_context(|| format!("{server} has no address"))?;

    let event_loop = winit::event_loop::EventLoop::new()?;
    let mut app = app::App::new(app::Options {
        connect: net::ConnectOptions {
            server: addr,
            name,
            cert,
        },
        vsync,
    });
    event_loop.run_app(&mut app)?;
    Ok(())
}
