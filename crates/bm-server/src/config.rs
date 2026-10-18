//! Server settings: `server.toml` in the working directory if present, then
//! command-line overrides. Every field has a default so a bare `bm-server`
//! runs a playable local planet.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub bind: SocketAddr,
    /// Saves, the certificate, logs.
    pub data_dir: PathBuf,
    pub planet_name: String,
    /// Used only when the planet is first created; afterwards the saved
    /// planet's own settings win.
    pub seed: u64,
    pub width_chunks: u32,
    pub height_chunks: u32,
    /// Horizontal chunk radius streamed around each player.
    pub view_radius: u8,
    /// Vertical chunk radius streamed around each player.
    pub view_height: u8,
    pub max_players: usize,
    /// Furthest a player may edit from their eye, in blocks.
    pub reach: f64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: "0.0.0.0:7777".parse().expect("valid"),
            data_dir: PathBuf::from("data"),
            planet_name: "Verdance".into(),
            seed: 20261018,
            // 2 048 blocks around: a few minutes' sprint to lap the planet.
            width_chunks: 64,
            height_chunks: 12,
            view_radius: 10,
            view_height: 4,
            max_players: 200,
            reach: 7.0,
        }
    }
}

impl ServerConfig {
    pub fn load(args: impl Iterator<Item = String>) -> Result<Self> {
        let mut cfg = match std::fs::read_to_string("server.toml") {
            Ok(s) => toml::from_str(&s).context("server.toml")?,
            Err(_) => Self::default(),
        };
        let mut args = args.peekable();
        while let Some(a) = args.next() {
            let mut val = || args.next().with_context(|| format!("{a} needs a value"));
            match a.as_str() {
                "--bind" => cfg.bind = val()?.parse()?,
                "--data" => cfg.data_dir = PathBuf::from(val()?),
                "--seed" => cfg.seed = val()?.parse()?,
                "--view" => cfg.view_radius = val()?.parse()?,
                "--width" => cfg.width_chunks = val()?.parse()?,
                "--help" | "-h" => {
                    println!("bm-server [--bind ADDR] [--data DIR] [--seed N] [--view CHUNKS] [--width CHUNKS]");
                    std::process::exit(0);
                }
                other => bail!("unknown argument {other}"),
            }
        }
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn validate(&self) -> Result<()> {
        if self.width_chunks < 8 {
            bail!("width_chunks must be at least 8");
        }
        if self.view_radius as u32 * 2 + 1 >= self.width_chunks {
            bail!("view_radius is too large for this planet (it would see itself)");
        }
        if !(1..=32).contains(&self.view_radius) {
            bail!("view_radius must be 1..=32");
        }
        Ok(())
    }
}
