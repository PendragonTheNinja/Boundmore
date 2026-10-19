//! Boundmore server library. `bm-server` is a thin binary over [`start`]; tests
//! start real servers on ephemeral ports the same way.

pub mod accounts;
pub mod claims;
pub mod config;
pub mod crafting;
pub mod game;
pub mod net;
pub mod planet;
pub mod store;
pub mod world;

use anyhow::{Context, Result};
use bm_core::{BlockRegistry, ItemRegistry, RecipeBook};
use bm_net::quinn::Endpoint;
use bm_worldgen::PlanetKinds;
use config::ServerConfig;
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use store::Store;

pub use bm_net::quinn::rustls::pki_types::CertificateDer;

pub struct RunningServer {
    pub addr: SocketAddr,
    /// The certificate clients must trust (also written to the data dir).
    pub cert: CertificateDer<'static>,
    running: Arc<AtomicBool>,
    load: Arc<std::sync::Mutex<Vec<game::PlanetLoad>>>,
    endpoint: Endpoint,
    game: Option<JoinHandle<()>>,
    net: Option<JoinHandle<()>>,
    writer: Option<JoinHandle<()>>,
    store: Option<Store>,
}

impl RunningServer {
    /// Stops accepting players, saves everything, and waits for it to finish.
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(g) = self.game.take() {
            let _ = g.join();
        }
        self.endpoint.close(0u32.into(), b"server stopped");
        if let Some(n) = self.net.take() {
            let _ = n.join();
        }
        if let Some(w) = self.writer.take() {
            let _ = w.join();
        }
        // Chunk loads still running on the thread pool hold the database;
        // let them finish so the file is closed when `stop` returns.
        if let Some(store) = self.store.take()
            && !store.wait_until_sole_owner(std::time::Duration::from_secs(10))
        {
            tracing::warn!("chunk loads were still running at shutdown");
        }
    }

    /// Players and chunks per planet, as of the last second.
    pub fn planet_load(&self) -> Vec<game::PlanetLoad> {
        self.load.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn is_running(&self) -> bool {
        self.game.as_ref().is_some_and(|g| !g.is_finished())
    }
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub fn start(cfg: ServerConfig) -> Result<RunningServer> {
    cfg.validate()?;
    std::fs::create_dir_all(&cfg.data_dir).with_context(|| format!("creating {}", cfg.data_dir.display()))?;
    let registry = Arc::new(BlockRegistry::builtin());
    let items = ItemRegistry::builtin(&registry);
    let recipes = RecipeBook::builtin(&items);
    let kinds = PlanetKinds::builtin(&registry);
    for spec in &cfg.planets {
        if kinds.get(&spec.kind).is_none() {
            anyhow::bail!("planet {}: no planet kind `{}` in planets.toml", spec.name, spec.kind);
        }
    }
    let (store, _upgraded) = Store::open_universe(&cfg.data_dir, &cfg.planets[0].name, &kinds)?;
    for id in store.planet_ids()? {
        if id as usize >= cfg.planets.len() {
            tracing::warn!(
                "planet {id} is saved but not in the config's planet list; it is not hosted (its players start on {})",
                cfg.planets[cfg.start_planet].name
            );
        }
    }
    let (claims, next_claim) = store.claims()?;
    let mut claims_by_planet: BTreeMap<u16, Vec<_>> = BTreeMap::new();
    for (id, c) in claims {
        claims_by_planet.entry(c.planet).or_default().push((id, c));
    }
    let cat = planet::Catalogue {
        registry: &registry,
        items: &items,
        recipes: &recipes,
        kinds: &kinds,
    };
    let mut planets = Vec::new();
    for (i, spec) in cfg.planets.iter().enumerate() {
        let id = i as u16;
        planets.push(planet::open(
            &store,
            id,
            spec,
            cfg.height_chunks,
            cfg.shrine_guard,
            &cat,
            claims_by_planet.remove(&id).unwrap_or_default(),
        )?);
    }
    let identity = bm_net::load_or_create_identity(&cfg.data_dir)?;

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("net")
        .enable_all()
        .build()?;
    let endpoint = {
        let _guard = rt.enter();
        bm_net::server_endpoint(cfg.bind, &identity)?
    };
    let addr = endpoint.local_addr()?;
    let (events_tx, events_rx) = crossbeam_channel::unbounded();
    let accept_endpoint = endpoint.clone();
    let net_store = store.clone();
    let net = std::thread::Builder::new().name("net".into()).spawn(move || {
        rt.block_on(net::accept_loop(accept_endpoint, events_tx, net_store));
        rt.shutdown_timeout(std::time::Duration::from_secs(1));
    })?;

    let (writer_tx, writer) = store::spawn_writer(store.clone());
    let held = store.clone();
    let start = cfg.start_planet;
    let game = game::Game::new(
        cfg, planets, start, next_claim, items, recipes, store, writer_tx, events_rx,
    )?;
    let load = game.load_handle();
    let running = Arc::new(AtomicBool::new(true));
    let game_running = running.clone();
    let game = std::thread::Builder::new()
        .name("game".into())
        .spawn(move || game.run(game_running))?;
    tracing::info!("listening on {addr}");
    Ok(RunningServer {
        addr,
        cert: identity.cert,
        running,
        load,
        endpoint,
        game: Some(game),
        net: Some(net),
        writer: Some(writer),
        store: Some(held),
    })
}
