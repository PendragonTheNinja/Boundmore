//! Boundmore server library. `bm-server` is a thin binary over [`start`]; tests
//! start real servers on ephemeral ports the same way.

pub mod config;
pub mod game;
pub mod net;
pub mod store;
pub mod world;

use anyhow::{Context, Result};
use bm_core::{BlockRegistry, CHUNK_FORMAT_VERSION, PlanetShape};
use bm_net::quinn::Endpoint;
use bm_worldgen::{GENERATOR_VERSION, Generator, PlanetGenConfig};
use config::ServerConfig;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use store::{PlanetMeta, SAVE_VERSION, Store};

pub use bm_net::quinn::rustls::pki_types::CertificateDer;

pub struct RunningServer {
    pub addr: SocketAddr,
    /// The certificate clients must trust (also written to the data dir).
    pub cert: CertificateDer<'static>,
    running: Arc<AtomicBool>,
    endpoint: Endpoint,
    game: Option<JoinHandle<()>>,
    net: Option<JoinHandle<()>>,
    writer: Option<JoinHandle<()>>,
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

fn file_name(planet: &str) -> String {
    let s: String = planet
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    format!("{s}.redb")
}

pub fn start(cfg: ServerConfig) -> Result<RunningServer> {
    cfg.validate()?;
    std::fs::create_dir_all(&cfg.data_dir).with_context(|| format!("creating {}", cfg.data_dir.display()))?;
    let registry = Arc::new(BlockRegistry::builtin());
    let store = Store::open(&cfg.data_dir.join(file_name(&cfg.planet_name)))?;
    let meta = store.planet_meta(PlanetMeta {
        save_version: SAVE_VERSION,
        generator_version: GENERATOR_VERSION,
        chunk_format: CHUNK_FORMAT_VERSION,
        name: cfg.planet_name.clone(),
        shape: PlanetShape::new(cfg.width_chunks, cfg.height_chunks),
        generation: PlanetGenConfig {
            seed: cfg.seed,
            ..Default::default()
        },
    })?;
    tracing::info!(
        "planet {} | {}×{} blocks, {} high | seed {} | {} edited chunks on disk",
        meta.name,
        meta.shape.width_blocks(),
        meta.shape.width_blocks(),
        meta.shape.height_blocks(),
        meta.generation.seed,
        store.stored_chunks()?
    );
    let generator = Arc::new(Generator::new(meta.shape, meta.generation.clone(), &registry)?);
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
    let net = std::thread::Builder::new().name("net".into()).spawn(move || {
        rt.block_on(net::accept_loop(accept_endpoint, events_tx));
        rt.shutdown_timeout(std::time::Duration::from_secs(1));
    })?;

    let (writer_tx, writer) = store::spawn_writer(store.clone());
    let world = world::World::new(meta.shape, registry, generator.clone(), store.clone());
    let game = game::Game::new(cfg, meta.name, world, &generator, store, writer_tx, events_rx);
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
        endpoint,
        game: Some(game),
        net: Some(net),
        writer: Some(writer),
    })
}
