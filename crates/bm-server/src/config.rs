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

/// A creature the config places beside where players arrive on the start
/// planet, once its first player is there (tests: a given creature at a
/// given place, with natural spawning off).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlacedCreature {
    /// A kind in the creature catalogue.
    pub kind: String,
    pub tier: u8,
    /// Feet position, from where players arrive.
    pub offset: [f64; 3],
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
    /// Most wayshards a portal's tank holds (ADR-0009).
    pub portal_tank_capacity: u32,
    /// Wayshards one charged trip through a portal costs, to a portal on the
    /// same planet and to one on another.
    pub portal_trip_cost_same_planet: u32,
    pub portal_trip_cost_other_planet: u32,
    /// Seconds a traveller's charge at one portal lasts: trips through it
    /// within this time are free.
    pub portal_charge_cooldown: u64,
    /// Opening a portal's window needs the portal epic (Waywise), as in
    /// Boundless (Nathan, 2026-10-21: required, "maybe we change later").
    pub portal_epic_required: bool,
    /// An item catalogue to use instead of the built-in `assets/items.toml`
    /// (tests use one to try tools no recipe makes yet). Saves name items,
    /// so a catalogue may add items freely.
    pub items: Option<PathBuf>,
    /// A creature catalogue to use instead of the built-in
    /// `assets/creatures.toml` (tests use one with still creatures and fixed
    /// drops). Creatures are never saved, so any catalogue will do.
    pub creatures: Option<PathBuf>,
    /// Creatures appear around players by themselves. Off for tests that
    /// place their own (`placed_creatures`).
    pub creature_spawning: bool,
    /// No more spawn around a player who has this many within 48 blocks: a
    /// **guess** (Boundless's counts are unpublished; Nathan may count them).
    pub creatures_near_player: usize,
    /// The most creatures one planet holds: a **guess**, a ceiling for load.
    pub creatures_per_planet: usize,
    /// Creatures placed at startup (tests).
    pub placed_creatures: Vec<PlacedCreature>,

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
            // An untrained Boundless character's reach, 4 m (Control adds 1
            // a point once skills exist; Nathan, 2026-10-20).
            reach: 4.0,
            plot_allowance: 64,
            claim_keep_days: 183,
            claim_keep_seconds: None,
            shrine_guard: 4,
            portal_tank_capacity: 500,
            portal_trip_cost_same_planet: 1,
            portal_trip_cost_other_planet: 2,
            portal_charge_cooldown: 3600,
            portal_epic_required: true,
            items: None,
            creatures: None,
            creature_spawning: true,
            creatures_near_player: 8,
            creatures_per_planet: 600,
            placed_creatures: Vec::new(),
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

    /// The portal tunables, the cooldown in ticks.
    pub fn portal_rules(&self) -> crate::portals::PortalRules {
        crate::portals::PortalRules {
            capacity: self.portal_tank_capacity,
            cost_same_planet: self.portal_trip_cost_same_planet,
            cost_other_planet: self.portal_trip_cost_other_planet,
            cooldown_ticks: (self.portal_charge_cooldown as f64 / bm_core::TICK_DT).round() as u64,
        }
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
        if self.portal_trip_cost_same_planet == 0 || self.portal_trip_cost_other_planet == 0 {
            bail!("portal trip costs must be at least 1 (an empty tank must close a portal)");
        }
        if self.portal_tank_capacity
            < self
                .portal_trip_cost_same_planet
                .max(self.portal_trip_cost_other_planet)
        {
            bail!("portal_tank_capacity must hold at least one trip");
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
    fn reach_is_an_untrained_boundless_characters() {
        // 4 m; Control adds 1 a point once skills exist (Nathan, 2026-10-20).
        assert_eq!(ServerConfig::default().reach, 4.0);
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
        assert_eq!(cfg.portal_rules(), d.portal_rules());
    }
}
