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
    pub server: SocketAddr,
    pub name: String,
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
    let conn: Connection = bm_net::connect(&endpoint, opts.server).await?;
    let (mut send, mut recv) = conn.open_bi().await?;
    write_msg(
        &mut send,
        &ClientMsg::Hello {
            protocol: PROTOCOL_VERSION,
            name: opts.name,
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
    endpoint.wait_idle().await;
    Ok(reason)
}
