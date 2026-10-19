//! Server settings: `server.toml` in the working directory if present, then
//! command-line overrides. Every field has a default so a bare `bm-server`
//! runs a playable local universe of four planets.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;

/// One planet of the universe, as the config creates it. Used only when the
/// planet is first created; afterwards the saved planet's own settings win
/// (its name included).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlanetSpec {
    pub name: String,
    /// A kind in `assets/planets.toml`.
    pub kind: String,
    pub seed: u64,
    pub width_chunks: u32,
}

impl Default for PlanetSpec {
    fn default() -> Self {
        Self {
            name: "Verdance".into(),
            kind: "temperate".into(),
            seed: 20261018,
            width_chunks: 64,
        }
    }
}

impl PlanetSpec {
    pub fn new(name: &str, kind: &str, seed: u64, width_chunks: u32) -> Self {
        Self {
            name: name.into(),
            kind: kind.into(),
            seed,
            width_chunks,
        }
    }
}

/// The default universe: one planet of each built-in kind, the gentle one
/// first.
pub fn default_planets() -> Vec<PlanetSpec> {
    vec![
        // 2 048 blocks around: a few minutes' sprint to lap the planet.
        PlanetSpec::new("Verdance", "temperate", 20261018, 64),
        PlanetSpec::new("Emberdune", "arid", 20261019, 48),
        PlanetSpec::new("Rimehold", "frozen", 20261020, 48),
        PlanetSpec::new("Wildmere", "lush", 20261021, 64),
    ]
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub bind: SocketAddr,
    /// Saves, the certificate, logs.
    pub data_dir: PathBuf,
    /// The universe. A planet's id is its position in this list, so add new
    /// planets at the end and never reorder or remove one.
    pub planets: Vec<PlanetSpec>,
    /// Index into `planets` of the planet new players start on.
    pub start_planet: usize,
    /// Height of every planet, in chunks.
    pub height_chunks: u32,
    /// Horizontal chunk radius streamed around each player.
    pub view_radius: u8,
    pub max_players: usize,
    /// Furthest a player may edit from their eye, in blocks.
    pub reach: f64,
    /// Plots one account may claim in all, across the universe.
    pub plot_allowance: u32,
    /// Days without a visit before a claim lapses: its build turns to ash
    /// and its stored items go back to the owner (decided 2026-10-18).
    pub claim_keep_days: u32,
    /// Seconds instead of days, for testing a lapse (`claim_keep_days` is
    /// ignored when this is set).
    pub claim_keep_seconds: Option<u64>,
    /// Blocks around each warp shrine that nobody may change or claim: this
    /// far each way across, down, and twice as far up. 0 protects the shrine
    /// alone.
    pub shrine_guard: u32,

    /// Save version 2 settings, still read so an old `server.toml` keeps its
    /// planet: they apply to the first planet.
    #[serde(skip_serializing)]
    pub planet_name: Option<String>,
    #[serde(skip_serializing)]
    pub seed: Option<u64>,
    #[serde(skip_serializing)]
    pub width_chunks: Option<u32>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: "0.0.0.0:7777".parse().expect("valid"),
            data_dir: PathBuf::from("data"),
            planets: default_planets(),
            start_planet: 0,
            height_chunks: 12,
            view_radius: 10,
            max_players: 200,
            reach: 7.0,
            plot_allowance: 64,
            claim_keep_days: 183,
            claim_keep_seconds: None,
            shrine_guard: 4,
            planet_name: None,
            seed: None,
            width_chunks: None,
        }
    }
}

impl ServerConfig {
    pub fn load(args: impl Iterator<Item = String>) -> Result<Self> {
        let mut cfg = match std::fs::read_to_string("server.toml") {
            Ok(s) => toml::from_str(&s).context("server.toml")?,
            Err(_) => Self::default(),
        };
        cfg.apply_legacy();
        let mut args = args.peekable();
        while let Some(a) = args.next() {
            let mut val = || args.next().with_context(|| format!("{a} needs a value"));
            match a.as_str() {
                "--bind" => cfg.bind = val()?.parse()?,
                "--data" => cfg.data_dir = PathBuf::from(val()?),
                // Planet i gets seed N + i.
                "--seed" => {
                    let seed: u64 = val()?.parse()?;
                    for (i, p) in cfg.planets.iter_mut().enumerate() {
                        p.seed = seed.wrapping_add(i as u64);
                    }
                }
                "--view" => cfg.view_radius = val()?.parse()?,
                "--width" => {
                    let w = val()?.parse()?;
                    for p in &mut cfg.planets {
                        p.width_chunks = w;
                    }
                }
                // Host only the first N planets.
                "--planets" => {
                    let n: usize = val()?.parse()?;
                    cfg.planets.truncate(n.max(1));
                }
                "--help" | "-h" => {
                    println!(
                        "bm-server [--bind ADDR] [--data DIR] [--seed N] [--view CHUNKS] [--width CHUNKS] [--planets N]"
                    );
                    std::process::exit(0);
                }
                other => bail!("unknown argument {other}"),
            }
        }
        cfg.validate()?;
        Ok(cfg)
    }

    /// How long a claim is kept without a visit, in seconds.
    pub fn claim_keep_secs(&self) -> u64 {
        self.claim_keep_seconds.unwrap_or(self.claim_keep_days as u64 * 86_400)
    }

    /// Moves version 2's single-planet settings onto the first planet.
    pub fn apply_legacy(&mut self) {
        let (name, seed, width) = (self.planet_name.take(), self.seed.take(), self.width_chunks.take());
        if name.is_none() && seed.is_none() && width.is_none() {
            return;
        }
        tracing::warn!("server.toml: `planet_name`, `seed` and `width_chunks` are old settings; use [[planets]]");
        if self.planets.is_empty() {
            self.planets.push(PlanetSpec::default());
        }
        let first = &mut self.planets[0];
        if let Some(n) = name {
            first.name = n;
        }
        if let Some(s) = seed {
            first.seed = s;
        }
        if let Some(w) = width {
            first.width_chunks = w;
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.planets.is_empty() {
            bail!("the universe needs at least one planet");
        }
        if self.planets.len() > u16::MAX as usize {
            bail!("too many planets");
        }
        if self.start_planet >= self.planets.len() {
            bail!("start_planet {} is not in the planet list", self.start_planet);
        }
        // Lush peaks reach about y 340 (planets.toml), and every planet
        // shares the height.
        if self.height_chunks < 12 {
            bail!("height_chunks must be at least 12 (384 blocks): the tallest planet kinds need it");
        }
        if !(1..=32).contains(&self.view_radius) {
            bail!("view_radius must be 1..=32");
        }
        for p in &self.planets {
            if p.width_chunks < 8 {
                bail!("planet {}: width_chunks must be at least 8", p.name);
            }
            if self.view_radius as u32 * 2 + 1 >= p.width_chunks {
                bail!("view_radius is too large for planet {} (it would see itself)", p.name);
            }
        }
        if self.shrine_guard > 16 {
            bail!("shrine_guard must be at most 16");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_old_server_toml_keeps_its_planet() {
        let mut cfg: ServerConfig = toml::from_str("planet_name = \"Home\"\nseed = 5\nwidth_chunks = 32\n").unwrap();
        cfg.apply_legacy();
        assert_eq!(cfg.planets[0], PlanetSpec::new("Home", "temperate", 5, 32));
        assert_eq!(cfg.planets.len(), 4);
        cfg.validate().unwrap();
    }

    #[test]
    fn planets_are_listed_in_toml() {
        let cfg: ServerConfig =
            toml::from_str("[[planets]]\nname = \"A\"\nkind = \"arid\"\nseed = 1\nwidth_chunks = 32\n").unwrap();
        assert_eq!(cfg.planets, vec![PlanetSpec::new("A", "arid", 1, 32)]);
        cfg.validate().unwrap();
    }

    #[test]
    fn the_example_config_matches_the_defaults() {
        let cfg: ServerConfig = toml::from_str(include_str!("../../../server.example.toml")).unwrap();
        cfg.validate().unwrap();
        let d = ServerConfig::default();
        assert_eq!(cfg.planets, d.planets);
        assert_eq!(
            (cfg.start_planet, cfg.height_chunks, cfg.shrine_guard),
            (d.start_planet, d.height_chunks, d.shrine_guard)
        );
    }
}
