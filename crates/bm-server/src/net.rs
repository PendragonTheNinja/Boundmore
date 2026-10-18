//! Connections: the async side of the server.
//!
//! Each connection gets a handshake (protocol check and login), a reader for
//! the reliable stream, a reader for datagrams, and a writer. They talk to the
//! game loop (a plain thread, see `game.rs`) only through [`Event`]s in and an
//! [`Outbox`] out, so the simulation never awaits anything and the network
//! never touches state. Logging in hashes passwords, which is slow by design,
//! so it happens here, before the game hears of the player at all.

use crate::accounts::{self, LoginError};
use crate::store::Store;
use bm_core::PlayerInput;
use bm_net::quinn::{self, Connection, Endpoint};
use bm_net::{read_msg, write_msg};
use bm_protocol::{ClientDatagram, ClientMsg, INPUT_REDUNDANCY, PROTOCOL_VERSION, ServerMsg};
use crossbeam_channel::Sender;
use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{Semaphore, mpsc};

/// Failed logins allowed per address per window; after that every login from
/// it is refused until the window passes (otherwise a correct guess would
/// still be let in).
const MAX_FAILED_LOGINS: usize = 5;
const FAILED_LOGIN_WINDOW: Duration = Duration::from_secs(60);
/// Password hashes computed at once, server wide. Each takes tens of
/// milliseconds and ~19 MiB, so a flood of logins queues here instead of
/// starving the machine the game loop runs on.
const CONCURRENT_HASHES: usize = 2;

pub enum Event {
    /// An authenticated player.
    Join {
        id: u32,
        /// Lowercased account name (what saves are keyed by).
        account: String,
        /// The account's name as registered.
        name: String,
        conn: Connection,
        out: Outbox,
    },
    Msg {
        id: u32,
        msg: ClientMsg,
    },
    Input {
        id: u32,
        latest: u32,
        inputs: Vec<PlayerInput>,
    },
    Leave {
        id: u32,
    },
}

enum Out {
    Shared(Arc<Vec<u8>>),
    Owned(Vec<u8>),
    Close(String),
}

/// The game loop's handle for sending reliable frames to one player. Sending
/// never blocks; `backlog` tells the loop how far behind the socket is so it
/// can hold back chunks rather than queue without bound.
#[derive(Clone)]
pub struct Outbox {
    tx: mpsc::UnboundedSender<Out>,
    queued: Arc<AtomicUsize>,
}

impl Outbox {
    pub fn send(&self, msg: &ServerMsg) {
        let frame = bm_protocol::encode_frame(msg);
        self.queued.fetch_add(frame.len(), Ordering::Relaxed);
        let _ = self.tx.send(Out::Owned(frame));
    }

    /// Sends a pre-encoded frame shared between players (chunks).
    pub fn send_shared(&self, frame: Arc<Vec<u8>>) {
        self.queued.fetch_add(frame.len(), Ordering::Relaxed);
        let _ = self.tx.send(Out::Shared(frame));
    }

    /// Sends a refusal and closes once it is written.
    pub fn close(&self, reason: &str) {
        self.send(&ServerMsg::Refused { reason: reason.into() });
        let _ = self.tx.send(Out::Close(reason.into()));
    }

    pub fn backlog(&self) -> usize {
        self.queued.load(Ordering::Relaxed)
    }
}

/// Failed logins per address, newest last.
#[derive(Default)]
struct LoginLimiter {
    failures: Mutex<HashMap<IpAddr, VecDeque<Instant>>>,
}

impl LoginLimiter {
    /// Reserves an attempt for `ip`, counted as a failure until forgiven, so
    /// many logins in flight at once cannot slip past the limit together.
    /// Returns None if the address has failed too often.
    fn begin(&self, ip: IpAddr) -> Option<Instant> {
        let now = Instant::now();
        let mut map = self.failures.lock().unwrap_or_else(|e| e.into_inner());
        if map.len() > 10_000 {
            map.retain(|_, q| q.back().is_some_and(|t| now - *t < FAILED_LOGIN_WINDOW));
        }
        let q = map.entry(ip).or_default();
        while q.front().is_some_and(|t| now - *t >= FAILED_LOGIN_WINDOW) {
            q.pop_front();
        }
        if q.len() >= MAX_FAILED_LOGINS {
            return None;
        }
        q.push_back(now);
        Some(now)
    }

    /// The attempt did not fail after all.
    fn forgive(&self, ip: IpAddr, attempt: Instant) {
        let mut map = self.failures.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(q) = map.get_mut(&ip)
            && let Some(i) = q.iter().position(|t| *t == attempt)
        {
            q.remove(i);
        }
    }
}

/// What every connection task shares.
struct Shared {
    events: Sender<Event>,
    store: Store,
    ids: AtomicU32,
    limiter: LoginLimiter,
    hashing: Semaphore,
}

pub async fn accept_loop(endpoint: Endpoint, events: Sender<Event>, store: Store) {
    let shared = Arc::new(Shared {
        events,
        store,
        ids: AtomicU32::new(1),
        limiter: LoginLimiter::default(),
        hashing: Semaphore::new(CONCURRENT_HASHES),
    });
    while let Some(incoming) = endpoint.accept().await {
        let shared = shared.clone();
        tokio::spawn(async move {
            match incoming.await {
                Ok(conn) => {
                    let id = shared.ids.fetch_add(1, Ordering::Relaxed);
                    if let Err(e) = serve(conn, id, &shared).await {
                        tracing::debug!("connection {id} ended: {e:#}");
                    }
                }
                Err(e) => tracing::debug!("handshake failed: {e}"),
            }
        });
    }
}

/// Checks the hello's credentials off the async threads. The password is
/// moved in and dropped with the closure.
async fn authenticate(
    shared: &Shared,
    ip: IpAddr,
    name: String,
    password: String,
    register: bool,
) -> Result<accounts::Account, LoginError> {
    // Only failed logins are limited: registering never fails on a password,
    // and a taken name is refused before any hashing.
    let attempt = if register {
        None
    } else {
        Some(shared.limiter.begin(ip).ok_or(LoginError::TooManyAttempts)?)
    };
    let permit = shared
        .hashing
        .acquire()
        .await
        .map_err(|_| LoginError::Server(anyhow::anyhow!("shutting down")))?;
    let store = shared.store.clone();
    let result = tokio::task::spawn_blocking(move || {
        if register {
            accounts::register(&store, &name, &password)
        } else {
            accounts::login(&store, &name, &password)
        }
    })
    .await
    .unwrap_or_else(|e| Err(LoginError::Server(anyhow::anyhow!("login task failed: {e}"))));
    drop(permit);
    if let Some(attempt) = attempt
        && !matches!(result, Err(LoginError::WrongCredentials))
    {
        shared.limiter.forgive(ip, attempt);
    }
    result
}

async fn serve(conn: Connection, id: u32, shared: &Shared) -> anyhow::Result<()> {
    let addr = conn.remote_address();
    let ip = addr.ip().to_canonical();
    let (mut send, mut recv) = tokio::time::timeout(Duration::from_secs(5), conn.accept_bi()).await??;
    let hello = tokio::time::timeout(Duration::from_secs(5), read_msg::<ClientMsg>(&mut recv)).await??;
    let refuse = |reason: &str| ServerMsg::Refused { reason: reason.into() };
    let account = match hello {
        Some(ClientMsg::Hello {
            protocol,
            name,
            password,
            register,
        }) if protocol == PROTOCOL_VERSION => match authenticate(shared, ip, name, password, register).await {
            Ok(a) => a,
            Err(e) => {
                if let LoginError::Server(err) = &e {
                    tracing::error!("login on #{id}: {err:#}");
                } else {
                    tracing::info!("login refused from {addr}: {}", e.reason());
                }
                write_msg(&mut send, &refuse(&e.reason())).await?;
                let _ = send.finish();
                // Let the refusal arrive before the connection closes.
                tokio::time::sleep(Duration::from_millis(200)).await;
                return Ok(());
            }
        },
        Some(ClientMsg::Hello { protocol, .. }) => {
            let why = format!("this server speaks protocol {PROTOCOL_VERSION}, your game speaks {protocol}; update");
            write_msg(&mut send, &refuse(&why)).await?;
            let _ = send.finish();
            tokio::time::sleep(Duration::from_millis(200)).await;
            return Ok(());
        }
        _ => return Ok(()),
    };
    tracing::info!("{} logged in from {addr} as #{id}", account.name);
    let events = &shared.events;

    let (tx, mut rx) = mpsc::unbounded_channel::<Out>();
    let queued = Arc::new(AtomicUsize::new(0));
    let out = Outbox {
        tx,
        queued: queued.clone(),
    };
    if events
        .send(Event::Join {
            id,
            account: account.key,
            name: account.name,
            conn: conn.clone(),
            out,
        })
        .is_err()
    {
        return Ok(()); // shutting down
    }

    // Writer: coalesces queued frames into one write per wakeup.
    let writer_conn = conn.clone();
    let writer = tokio::spawn(async move {
        let mut buf = Vec::with_capacity(64 << 10);
        while let Some(first) = rx.recv().await {
            let mut close = None;
            let mut push = |o: Out, buf: &mut Vec<u8>| match o {
                Out::Shared(f) => buf.extend_from_slice(&f),
                Out::Owned(f) => buf.extend_from_slice(&f),
                Out::Close(r) => close = Some(r),
            };
            push(first, &mut buf);
            while buf.len() < (256 << 10) {
                match rx.try_recv() {
                    Ok(o) => push(o, &mut buf),
                    Err(_) => break,
                }
            }
            let n = buf.len();
            if send.write_all(&buf).await.is_err() {
                break;
            }
            queued.fetch_sub(n, Ordering::Relaxed);
            buf.clear();
            if let Some(reason) = close {
                let _ = send.finish();
                tokio::time::sleep(Duration::from_millis(200)).await;
                writer_conn.close(quinn::VarInt::from_u32(1), reason.as_bytes());
                break;
            }
        }
    });

    let dgram_conn = conn.clone();
    let dgram_events = events.clone();
    let datagrams = tokio::spawn(async move {
        while let Ok(bytes) = dgram_conn.read_datagram().await {
            match bm_protocol::decode::<ClientDatagram>(&bytes) {
                Ok(ClientDatagram::Input { latest, mut inputs }) => {
                    inputs.truncate(INPUT_REDUNDANCY);
                    if dgram_events.send(Event::Input { id, latest, inputs }).is_err() {
                        break;
                    }
                }
                Err(_) => {
                    tracing::debug!("#{id} sent a malformed datagram");
                }
            }
        }
    });

    let result = async {
        while let Some(msg) = read_msg::<ClientMsg>(&mut recv).await? {
            if events.send(Event::Msg { id, msg }).is_err() {
                break;
            }
        }
        anyhow::Ok(())
    }
    .await;

    let _ = events.send(Event::Leave { id });
    datagrams.abort();
    writer.abort();
    conn.close(quinn::VarInt::from_u32(0), b"bye");
    result
}
