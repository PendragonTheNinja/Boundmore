//! Boundmore client.
//!
//! `boundmore [--server HOST:PORT] [--name NAME] [--password PW] [--register] [--cert PATH]`

mod app;
mod chat;
mod chisel;
mod claims;
mod colour;
mod demo;
mod icons;
mod items;
mod menus;
mod mining;
mod net;
mod portal;
mod predict;
mod render;
mod settings;
mod sky;
mod storage;
mod textures;
mod world;

use anyhow::{Context, bail};
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,wgpu_core=warn,wgpu_hal=warn,naga=warn,quinn=warn".into()),
        )
        .init();

    let mut server = "127.0.0.1:7777".to_string();
    let mut name = None;
    let mut password = None;
    let mut register = false;
    let mut cert = PathBuf::from("data").join(bm_net::CERT_FILE);
    let mut vsync = true;
    let mut assets = PathBuf::from("assets");
    let mut settings_path = settings::default_path();
    let mut demo = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut val = || args.next().with_context(|| format!("{a} needs a value"));
        match a.as_str() {
            "--server" => server = val()?,
            "--name" => name = Some(val()?),
            // For testing and scripts; the login screen is the normal way in.
            "--password" => password = Some(val()?),
            "--register" => register = true,
            "--cert" => cert = PathBuf::from(val()?),
            "--no-vsync" => vsync = false,
            "--assets" => assets = PathBuf::from(val()?),
            "--settings" => settings_path = PathBuf::from(val()?),
            // Hidden: menus offline with sample data (see demo.rs).
            "--demo-ui" => demo = Some(val()?),
            "--help" | "-h" => {
                println!(
                    "boundmore [--server HOST:PORT] [--name NAME] [--password PASSWORD] [--register] \
                     [--cert server.cert.der] [--assets DIR] [--settings FILE] [--no-vsync]"
                );
                return Ok(());
            }
            other => bail!("unknown argument {other}"),
        }
    }
    if password.is_some() && name.is_none() {
        bail!("--password needs --name");
    }

    let event_loop = winit::event_loop::EventLoop::new()?;
    let mut app = app::App::new(app::Options {
        server,
        name,
        password,
        register,
        cert,
        vsync,
        assets,
        settings_path,
        demo,
    });
    event_loop.run_app(&mut app)?;
    Ok(())
}
