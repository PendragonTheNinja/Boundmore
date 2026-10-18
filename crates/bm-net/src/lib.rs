//! QUIC transport shared by server and client.
//!
//! QUIC gives us, on one UDP port: encrypted connections, reliable streams
//! without TCP's head-of-line blocking across streams, and unreliable
//! datagrams for movement (ADR-0004). TLS is mandatory in QUIC; for now the
//! server uses a self-signed certificate that clients pin (they must be handed
//! the server's `server.cert.der`). Real certificates come with accounts.

use anyhow::{Context, Result, bail};
use bm_protocol::{ALPN, MAX_FRAME, decode, encode_frame};
use quinn::crypto::rustls::{QuicClientConfig, QuicServerConfig};
use quinn::{ClientConfig, Connection, Endpoint, RecvStream, SendStream, ServerConfig, TransportConfig, VarInt};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use serde::{Serialize, de::DeserializeOwned};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

pub use quinn;

/// The name the server's certificate is issued for; clients verify against it.
pub const SERVER_NAME: &str = "boundmore";
pub const CERT_FILE: &str = "server.cert.der";
const KEY_FILE: &str = "server.key.der";

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

pub struct ServerIdentity {
    pub cert: CertificateDer<'static>,
    key: PrivateKeyDer<'static>,
}

/// Loads the server's certificate and key from `dir`, creating them on first
/// run. The key file is written owner-only where the platform allows.
pub fn load_or_create_identity(dir: &Path) -> Result<ServerIdentity> {
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let (cert_path, key_path) = (dir.join(CERT_FILE), dir.join(KEY_FILE));
    if cert_path.exists() && key_path.exists() {
        let cert = std::fs::read(&cert_path)?;
        let key = std::fs::read(&key_path)?;
        return Ok(ServerIdentity {
            cert: CertificateDer::from(cert),
            key: PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key)),
        });
    }
    let made = rcgen::generate_simple_self_signed(vec![SERVER_NAME.to_string(), "localhost".to_string()])?;
    let cert = made.cert.der().to_vec();
    let key = made.signing_key.serialize_der();
    write_private(&key_path, &key)?;
    std::fs::write(&cert_path, &cert)?;
    Ok(ServerIdentity {
        cert: CertificateDer::from(cert),
        key: PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key)),
    })
}

#[cfg(unix)]
fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    f.write_all(bytes)?;
    Ok(())
}

#[cfg(not(unix))]
fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    std::fs::write(path, bytes)?;
    Ok(())
}

/// Transport tuning for a game: frequent keep-alives so NAT bindings stay
/// open, a short idle timeout so a dead client frees its slot quickly, large
/// windows so a burst of chunks on login is not throttled by flow control.
fn transport(server: bool) -> TransportConfig {
    let mut t = TransportConfig::default();
    t.keep_alive_interval(Some(Duration::from_secs(2)))
        .max_idle_timeout(Some(VarInt::from_u32(15_000).into()))
        .stream_receive_window(VarInt::from_u32(8 << 20))
        .receive_window(VarInt::from_u32(16 << 20))
        .send_window(16 << 20)
        .datagram_receive_buffer_size(Some(256 << 10))
        .datagram_send_buffer_size(256 << 10)
        .max_concurrent_uni_streams(VarInt::from_u32(0))
        // Only the client opens the one game stream.
        .max_concurrent_bidi_streams(VarInt::from_u32(if server { 1 } else { 0 }));
    t
}

pub fn server_endpoint(addr: SocketAddr, id: &ServerIdentity) -> Result<Endpoint> {
    let mut tls = rustls::ServerConfig::builder_with_provider(provider())
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .with_no_client_auth()
        .with_single_cert(vec![id.cert.clone()], id.key.clone_key())?;
    tls.alpn_protocols = vec![ALPN.to_vec()];
    let mut cfg = ServerConfig::with_crypto(Arc::new(QuicServerConfig::try_from(tls)?));
    cfg.transport_config(Arc::new(transport(true)));
    Ok(Endpoint::server(cfg, addr)?)
}

/// A client endpoint that trusts exactly `server_cert`.
pub fn client_endpoint(server_cert: &CertificateDer<'static>) -> Result<Endpoint> {
    let mut roots = rustls::RootCertStore::empty();
    roots.add(server_cert.clone())?;
    let mut tls = rustls::ClientConfig::builder_with_provider(provider())
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .with_root_certificates(roots)
        .with_no_client_auth();
    tls.alpn_protocols = vec![ALPN.to_vec()];
    let mut cfg = ClientConfig::new(Arc::new(QuicClientConfig::try_from(tls)?));
    cfg.transport_config(Arc::new(transport(false)));
    let bind: SocketAddr = "[::]:0".parse().expect("valid");
    let mut endpoint = match Endpoint::client(bind) {
        Ok(e) => e,
        // Hosts without IPv6.
        Err(_) => Endpoint::client("0.0.0.0:0".parse().expect("valid"))?,
    };
    endpoint.set_default_client_config(cfg);
    Ok(endpoint)
}

pub async fn connect(endpoint: &Endpoint, addr: SocketAddr) -> Result<Connection> {
    let conn = endpoint
        .connect(addr, SERVER_NAME)?
        .await
        .with_context(|| format!("connecting to {addr}"))?;
    Ok(conn)
}

/// Writes one length-prefixed frame.
pub async fn write_msg<T: Serialize>(send: &mut SendStream, msg: &T) -> Result<()> {
    send.write_all(&encode_frame(msg)).await?;
    Ok(())
}

/// Reads one frame. `Ok(None)` when the peer finished the stream cleanly.
pub async fn read_msg<T: DeserializeOwned>(recv: &mut RecvStream) -> Result<Option<T>> {
    let mut len = [0u8; 4];
    match recv.read_exact(&mut len).await {
        Ok(()) => {}
        Err(quinn::ReadExactError::FinishedEarly(0)) => return Ok(None),
        Err(e) => return Err(e.into()),
    }
    let len = u32::from_le_bytes(len) as usize;
    if len > MAX_FRAME {
        bail!("frame of {len} bytes exceeds the limit");
    }
    let mut body = vec![0u8; len];
    recv.read_exact(&mut body).await?;
    Ok(Some(decode(&body)?))
}

/// Sends a datagram, dropping it if the connection cannot take it now. A
/// datagram is always superseded by the next one, so dropping is correct.
pub fn send_datagram<T: Serialize>(conn: &Connection, msg: &T) {
    let bytes = bm_protocol::encode(msg);
    let _ = conn.send_datagram(bytes::Bytes::from(bytes));
}
