//! Connections: the async side of the server.
//!
//! Each connection gets a handshake, a reader for the reliable stream, a
//! reader for datagrams, and a writer. They talk to the game loop (a plain
//! thread, see `game.rs`) only through [`Event`]s in and an [`Outbox`] out, so
//! the simulation never awaits anything and the network never touches state.

use bm_core::PlayerInput;
use bm_net::quinn::{self, Connection, Endpoint};
use bm_net::{read_msg, write_msg};
use bm_protocol::{ClientDatagram, ClientMsg, INPUT_REDUNDANCY, PROTOCOL_VERSION, ServerMsg, clean_name};
use crossbeam_channel::Sender;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::time::Duration;
use tokio::sync::mpsc;

pub enum Event {
    Join {
        id: u32,
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

pub async fn accept_loop(endpoint: Endpoint, events: Sender<Event>) {
    let ids = Arc::new(AtomicU32::new(1));
    while let Some(incoming) = endpoint.accept().await {
        let (events, ids) = (events.clone(), ids.clone());
        tokio::spawn(async move {
            match incoming.await {
                Ok(conn) => {
                    let id = ids.fetch_add(1, Ordering::Relaxed);
                    if let Err(e) = serve(conn, id, events).await {
                        tracing::debug!("connection {id} ended: {e:#}");
                    }
                }
                Err(e) => tracing::debug!("handshake failed: {e}"),
            }
        });
    }
}

async fn serve(conn: Connection, id: u32, events: Sender<Event>) -> anyhow::Result<()> {
    let addr = conn.remote_address();
    let (mut send, mut recv) = tokio::time::timeout(Duration::from_secs(5), conn.accept_bi()).await??;
    let hello = tokio::time::timeout(Duration::from_secs(5), read_msg::<ClientMsg>(&mut recv)).await??;
    let refuse = |reason: &str| ServerMsg::Refused { reason: reason.into() };
    let name = match hello {
        Some(ClientMsg::Hello { protocol, name }) if protocol == PROTOCOL_VERSION => match clean_name(&name) {
            Some(n) => n,
            None => {
                write_msg(&mut send, &refuse("that name cannot be used")).await?;
                let _ = send.finish();
                return Ok(());
            }
        },
        Some(ClientMsg::Hello { protocol, .. }) => {
            let why = format!("this server speaks protocol {PROTOCOL_VERSION}, your game speaks {protocol}; update");
            write_msg(&mut send, &refuse(&why)).await?;
            let _ = send.finish();
            return Ok(());
        }
        _ => return Ok(()),
    };
    tracing::info!("{name} connecting from {addr} as #{id}");

    let (tx, mut rx) = mpsc::unbounded_channel::<Out>();
    let queued = Arc::new(AtomicUsize::new(0));
    let out = Outbox {
        tx,
        queued: queued.clone(),
    };
    if events
        .send(Event::Join {
            id,
            name,
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
