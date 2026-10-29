//! Boundmore client.
//!
//! `boundmore [--server HOST:PORT] [--name NAME] [--password PW] [--register] [--cert PATH]`

mod app;
mod atlas;
mod chat;
mod chisel;
mod claims;
mod colour;
mod combat;
mod compass;
mod creatures;
mod demo;
mod effects;
mod far;
mod fight_hud;
mod forge_screen;
mod icons;
mod items;
mod menus;
mod meteors;
mod mining;
mod net;
mod portal;
mod predict;
mod render;
mod settings;
mod shop;
mod signs;
mod skill_screen;
mod skills;
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
    let mut cert_given = false;
    let mut assets_given = false;
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
            "--cert" => {
                cert = PathBuf::from(val()?);
                cert_given = true;
            }
            "--no-vsync" => vsync = false,
            "--assets" => {
                assets = PathBuf::from(val()?);
                assets_given = true;
            }
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
    // Started from anywhere (a double click on the exe): the defaults are
    // found beside the executable, or up from it (target\release).
    let exe_dir = std::env::current_exe().ok().and_then(|e| e.parent().map(PathBuf::from));
    if !assets_given {
        assets = find_near(&assets, exe_dir.as_deref());
    }
    if !cert_given {
        cert = find_near(&cert, exe_dir.as_deref());
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

/// `rel` as given if it exists from here; else the first of the executable's
/// folder and up to three above it that holds it; else `rel` unchanged (the
/// error then names the path the player expects).
fn find_near(rel: &std::path::Path, exe_dir: Option<&std::path::Path>) -> PathBuf {
    if rel.is_absolute() || rel.exists() {
        return rel.to_path_buf();
    }
    exe_dir
        .into_iter()
        .flat_map(|d| d.ancestors().take(4))
        .map(|d| d.join(rel))
        .find(|p| p.exists())
        .unwrap_or_else(|| rel.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_found_up_from_the_executable() {
        let root = std::env::temp_dir().join(format!("bm-near-{}", std::process::id()));
        let exe = root.join("target").join("release");
        std::fs::create_dir_all(&exe).unwrap();
        std::fs::create_dir_all(root.join("assets-for-test")).unwrap();
        let rel = PathBuf::from("assets-for-test");
        assert_eq!(find_near(&rel, Some(&exe)), root.join("assets-for-test"));
        let missing = PathBuf::from("nowhere-to-be-found");
        assert_eq!(find_near(&missing, Some(&exe)), missing, "unchanged when nowhere");
        std::fs::remove_dir_all(&root).ok();
    }
}
