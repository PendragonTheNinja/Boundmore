//! Headless bots for load testing and for seeing other players.
//!
//! `cargo run --release -p bm-server --example bots -- --count 50 --server 127.0.0.1:7777 --cert data/server.cert.der [--prefix Bot] [--seconds N] [--planets N]`
//!
//! Each bot registers an account (password `botpassword123`), or logs in if a
//! previous run registered it, then wanders: walks, turns, jumps,
//! sends inputs at 60 Hz with the same redundancy the game uses, and reads
//! everything the server sends. With `--planets N`, bot `i` first travels
//! from the shrine it arrived beside to planet `i % N`, spreading the bots
//! over the first N planets.

use bm_core::{BlockPos, PlayerInput, TICK_DT};
use bm_net::quinn::{Connection, Endpoint, RecvStream, SendStream};
use bm_net::{read_msg, write_msg};
use bm_protocol::{ClientDatagram, ClientMsg, INPUT_REDUNDANCY, PROTOCOL_VERSION, ServerMsg, Welcome};
use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[derive(Default)]
struct Totals {
    connected: AtomicU64,
    reliable_bytes: AtomicU64,
    chunks: AtomicU64,
    snapshots: AtomicU64,
    travelled: AtomicU64,
    /// Creatures in view across all bots (milestone 13): came into view
    /// less left.
    creatures: AtomicU64,
    creatures_gone: AtomicU64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut count = 20usize;
    let mut server: SocketAddr = "127.0.0.1:7777".parse()?;
    let mut cert = std::path::PathBuf::from("data").join(bm_net::CERT_FILE);
    let mut seconds = 0u64;
    let mut prefix = "Bot".to_string();
    let mut planets = 1u16;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let v = args.next().unwrap_or_default();
        match a.as_str() {
            "--count" => count = v.parse()?,
            "--server" => server = v.parse()?,
            "--cert" => cert = v.into(),
            "--seconds" => seconds = v.parse()?,
            // Bots are saved like players; a new prefix makes new bots that
            // start at the spawn instead of where the last run left them.
            "--prefix" => prefix = v,
            "--planets" => planets = v.parse::<u16>()?.max(1),
            other => anyhow::bail!("unknown argument {other}"),
        }
    }
    let cert = std::fs::read(&cert)?;
    let totals = Arc::new(Totals::default());
    for i in 0..count {
        let (cert, totals, name) = (cert.clone(), totals.clone(), format!("{prefix} {i}"));
        tokio::spawn(async move {
            let target = (i % planets as usize) as u16;
            if let Err(e) = bot(i, name, server, cert, totals, target).await {
                eprintln!("bot {i}: {e:#}");
            }
        });
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    let start = Instant::now();
    loop {
        tokio::time::sleep(Duration::from_secs(5)).await;
        let t = start.elapsed().as_secs_f64();
        println!(
            "{:>5.0}s | {} bots connected, {} travelled | {} chunks, {:.1} MiB reliable | {:.0} snapshots/s | {} creatures in view",
            t,
            totals.connected.load(Ordering::Relaxed),
            totals.travelled.load(Ordering::Relaxed),
            totals.chunks.load(Ordering::Relaxed),
            totals.reliable_bytes.load(Ordering::Relaxed) as f64 / (1 << 20) as f64,
            totals.snapshots.load(Ordering::Relaxed) as f64 / t,
            totals
                .creatures
                .load(Ordering::Relaxed)
                .saturating_sub(totals.creatures_gone.load(Ordering::Relaxed)),
        );
        if seconds > 0 && t >= seconds as f64 {
            return Ok(());
        }
    }
}

const BOT_PASSWORD: &str = "botpassword123";

/// A welcomed connection: every handle must outlive the bot.
type Session = (Endpoint, Connection, SendStream, RecvStream, Box<Welcome>);

/// Connects and says hello; the session on a welcome, the refusal's reason
/// otherwise.
async fn connect(
    name: &str,
    register: bool,
    server: SocketAddr,
    cert: &[u8],
) -> anyhow::Result<Result<Session, String>> {
    let endpoint = bm_net::client_endpoint(&cert.to_vec().into())?;
    let conn = bm_net::connect(&endpoint, server).await?;
    let (mut send, mut recv) = conn.open_bi().await?;
    write_msg(
        &mut send,
        &ClientMsg::Hello {
            protocol: PROTOCOL_VERSION,
            name: name.into(),
            password: BOT_PASSWORD.into(),
            register,
        },
    )
    .await?;
    match read_msg::<ServerMsg>(&mut recv).await? {
        Some(ServerMsg::Welcome(w)) => Ok(Ok((endpoint, conn, send, recv, w))),
        Some(ServerMsg::Refused { reason }) => Ok(Err(reason)),
        other => anyhow::bail!("not welcomed: {other:?}"),
    }
}

async fn bot(
    i: usize,
    name: String,
    server: SocketAddr,
    cert: Vec<u8>,
    totals: Arc<Totals>,
    target: u16,
) -> anyhow::Result<()> {
    let (_endpoint, conn, mut send, mut recv, welcome) = match connect(&name, true, server, &cert).await? {
        Ok(c) => c,
        Err(reason) if reason == "that name is taken" => match connect(&name, false, server, &cert).await? {
            Ok(c) => c,
            Err(reason) => anyhow::bail!("refused: {reason}"),
        },
        Err(reason) => anyhow::bail!("refused: {reason}"),
    };
    totals.connected.fetch_add(1, Ordering::Relaxed);
    if welcome.planet.id != target {
        // Arrivals stand two blocks along +X from the shrine. The server
        // must have sent its chunk before it lets the bot use it.
        let s = welcome.spawn.pos;
        let shrine = BlockPos::containing(s.x, s.y, s.z).offset(-2, 0, 0);
        let shrine = welcome.planet.shape.wrap_block(shrine);
        while let Some(m) = read_msg::<ServerMsg>(&mut recv).await? {
            if let ServerMsg::Chunk { pos, .. } = m
                && pos == shrine.chunk()
            {
                break;
            }
        }
        write_msg(&mut send, &ClientMsg::Warp { shrine, planet: target }).await?;
        loop {
            match read_msg::<ServerMsg>(&mut recv).await? {
                Some(ServerMsg::Travel(_)) => break,
                Some(ServerMsg::Notice { text }) => anyhow::bail!("could not travel: {text}"),
                Some(_) => {}
                None => anyhow::bail!("closed before travelling"),
            }
        }
        totals.travelled.fetch_add(1, Ordering::Relaxed);
    }
    let _send = send;
    let t2 = totals.clone();
    tokio::spawn(async move {
        let mut seen = std::collections::HashSet::new();
        while let Ok(Some(m)) = read_msg::<ServerMsg>(&mut recv).await {
            match &m {
                ServerMsg::Chunk { data, .. } => {
                    t2.chunks.fetch_add(1, Ordering::Relaxed);
                    t2.reliable_bytes.fetch_add(data.len() as u64, Ordering::Relaxed);
                }
                ServerMsg::Creature(v) if seen.insert(v.id) => {
                    t2.creatures.fetch_add(1, Ordering::Relaxed);
                }
                ServerMsg::CreatureGone { id } if seen.remove(id) => {
                    t2.creatures_gone.fetch_add(1, Ordering::Relaxed);
                }
                _ => {}
            }
        }
    });
    let dconn = conn.clone();
    let t3 = totals.clone();
    tokio::spawn(async move {
        while dconn.read_datagram().await.is_ok() {
            t3.snapshots.fetch_add(1, Ordering::Relaxed);
        }
    });

    // A cheap deterministic wander per bot.
    let mut rng = 0x9e37_79b9_7f4a_7c15u64 ^ (i as u64).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    let mut next = || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        rng
    };
    let mut yaw: u16 = next() as u16;
    let mut seq = 0u32;
    let mut recent: VecDeque<PlayerInput> = VecDeque::new();
    let mut tick = tokio::time::interval(Duration::from_secs_f64(TICK_DT));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Burst);
    loop {
        tick.tick().await;
        if next() % 90 == 0 {
            yaw = yaw.wrapping_add((next() % 20000) as u16).wrapping_sub(10000);
        }
        let input = PlayerInput {
            forward: 1,
            strafe: 0,
            jump: next() % 25 == 0,
            sprint: i % 2 == 0,
            yaw,
            ..Default::default()
        };
        seq += 1;
        recent.push_front(input);
        recent.truncate(INPUT_REDUNDANCY);
        bm_net::send_datagram(
            &conn,
            &ClientDatagram::Input {
                latest: seq,
                inputs: recent.iter().copied().collect(),
            },
        );
        if let Some(reason) = conn.close_reason() {
            anyhow::bail!("closed: {reason}");
        }
    }
}
