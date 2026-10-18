//! The client's network thread. The game loop never awaits: it sends
//! [`NetCmd`]s and drains [`NetEvent`]s each frame.

use bm_net::quinn::Connection;
use bm_net::{read_msg, write_msg};
use bm_protocol::{ClientDatagram, ClientMsg, PROTOCOL_VERSION, ServerDatagram, ServerMsg, Welcome};
use crossbeam_channel::{Receiver, Sender};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::mpsc;

pub enum NetEvent {
    Connected(Box<Welcome>),
    Msg(ServerMsg),
    Snapshot(ServerDatagram),
    Disconnected(String),
}

pub enum NetCmd {
    Reliable(ClientMsg),
    Datagram(ClientDatagram),
}

pub struct Net {
    pub events: Receiver<NetEvent>,
    cmds: mpsc::UnboundedSender<NetCmd>,
    rtt_micros: Arc<AtomicU64>,
}

impl Net {
    pub fn send(&self, msg: ClientMsg) {
        let _ = self.cmds.send(NetCmd::Reliable(msg));
    }

    pub fn send_datagram(&self, d: ClientDatagram) {
        let _ = self.cmds.send(NetCmd::Datagram(d));
    }

    /// Smoothed round trip, from QUIC's own estimate.
    pub fn rtt_ms(&self) -> f64 {
        self.rtt_micros.load(Ordering::Relaxed) as f64 / 1000.0
    }
}

pub struct ConnectOptions {
    /// `host:port`, resolved on the network thread so the window never
    /// waits on DNS.
    pub server: String,
    pub name: String,
    /// Kept only for the handshake; never written anywhere.
    pub password: String,
    /// Create the account rather than log in to it.
    pub register: bool,
    pub cert: PathBuf,
}

pub fn spawn(opts: ConnectOptions) -> Net {
    let (etx, events) = crossbeam_channel::unbounded();
    let (cmds, crx) = mpsc::unbounded_channel();
    let rtt = Arc::new(AtomicU64::new(0));
    let rtt2 = rtt.clone();
    std::thread::Builder::new()
        .name("net".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .expect("tokio runtime");
            let reason = rt.block_on(async {
                match session(opts, etx.clone(), crx, rtt2).await {
                    Ok(r) => r,
                    Err(e) => format!("{e:#}"),
                }
            });
            let _ = etx.send(NetEvent::Disconnected(reason));
        })
        .expect("spawn net thread");
    Net {
        events,
        cmds,
        rtt_micros: rtt,
    }
}

async fn session(
    opts: ConnectOptions,
    events: Sender<NetEvent>,
    mut cmds: mpsc::UnboundedReceiver<NetCmd>,
    rtt: Arc<AtomicU64>,
) -> anyhow::Result<String> {
    let cert = std::fs::read(&opts.cert).map_err(|e| {
        anyhow::anyhow!(
            "cannot read the server certificate {} ({e}); copy server.cert.der from the server's data folder",
            opts.cert.display()
        )
    })?;
    let endpoint = bm_net::client_endpoint(&cert.into())?;
    let addr: SocketAddr = tokio::net::lookup_host(opts.server.as_str())
        .await
        .map_err(|e| anyhow::anyhow!("cannot find the server {}: {e}", opts.server))?
        .next()
        .ok_or_else(|| anyhow::anyhow!("the server {} has no address", opts.server))?;
    let conn: Connection = bm_net::connect(&endpoint, addr).await?;
    let (mut send, mut recv) = conn.open_bi().await?;
    write_msg(
        &mut send,
        &ClientMsg::Hello {
            protocol: PROTOCOL_VERSION,
            name: opts.name,
            password: opts.password,
            register: opts.register,
        },
    )
    .await?;
    match read_msg::<ServerMsg>(&mut recv).await? {
        Some(ServerMsg::Welcome(w)) => {
            let _ = events.send(NetEvent::Connected(w));
        }
        Some(ServerMsg::Refused { reason }) => return Ok(reason),
        _ => return Ok("the server closed the connection".into()),
    }

    let dconn = conn.clone();
    let devents = events.clone();
    let datagrams = tokio::spawn(async move {
        while let Ok(bytes) = dconn.read_datagram().await {
            if let Ok(d) = bm_protocol::decode::<ServerDatagram>(&bytes) {
                rtt.store(dconn.rtt().as_micros() as u64, Ordering::Relaxed);
                if devents.send(NetEvent::Snapshot(d)).is_err() {
                    break;
                }
            }
        }
    });

    let writer_conn = conn.clone();
    let writer = tokio::spawn(async move {
        while let Some(cmd) = cmds.recv().await {
            match cmd {
                NetCmd::Reliable(m) => {
                    if write_msg(&mut send, &m).await.is_err() {
                        break;
                    }
                }
                NetCmd::Datagram(d) => bm_net::send_datagram(&writer_conn, &d),
            }
        }
    });

    let reason = loop {
        match read_msg::<ServerMsg>(&mut recv).await {
            Ok(Some(ServerMsg::Refused { reason })) => break reason,
            Ok(Some(m)) => {
                if events.send(NetEvent::Msg(m)).is_err() {
                    break "closed".into();
                }
            }
            Ok(None) => break "the server closed the connection".into(),
            Err(e) => break format!("connection lost: {e:#}"),
        }
    };
    datagrams.abort();
    writer.abort();
    conn.close(0u32.into(), b"bye");
    // Let the close reach the server, but never hang on a dead link.
    let _ = tokio::time::timeout(std::time::Duration::from_secs(1), endpoint.wait_idle()).await;
    Ok(reason)
}

/// Where the game's messages go: a real server, or the offline demo.
pub enum Link {
    Net(Net),
    Demo(Box<crate::demo::Demo>),
}

impl Link {
    pub fn send(&mut self, msg: ClientMsg) {
        match self {
            Link::Net(n) => n.send(msg),
            Link::Demo(d) => d.send(msg),
        }
    }

    pub fn send_datagram(&self, d: ClientDatagram) {
        if let Link::Net(n) = self {
            n.send_datagram(d);
        }
    }

    pub fn poll(&mut self) -> Vec<NetEvent> {
        match self {
            Link::Net(n) => n.events.try_iter().collect(),
            Link::Demo(d) => d.poll(),
        }
    }

    pub fn rtt_ms(&self) -> f64 {
        match self {
            Link::Net(n) => n.rtt_ms(),
            Link::Demo(_) => 0.0,
        }
    }
}
