//! Carrying Team Create's messages between computers (TC3; design §4.3 and
//! §9): WebSocket over TLS 1.3 on one TCP port.
//!
//! The host has a self-signed certificate it keeps from session to
//! session ([`HostCert`]); its fingerprint goes in every invite link, and
//! a joiner accepts exactly that certificate (it "pins" it), so nobody
//! between them can read or change what they say, with no certificate
//! authority involved.
//!
//! Each connection runs on its own thread, which hands whole messages to
//! the caller over a channel and sends the caller's. A connection that's
//! quiet for [`SILENCE`] counts as lost, so pulling a network cable is
//! noticed within seconds rather than when TCP gives up.

use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, WebPkiSupportedAlgorithms};
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use rustls::{
    ClientConfig, ClientConnection, DigitallySignedStruct, ServerConfig, ServerConnection,
};
use rustls::{SignatureScheme, StreamOwned};
use serde::{Deserialize, Serialize};
use tungstenite::protocol::WebSocketConfig;
use tungstenite::{Message, WebSocket};

use crate::identity::{self, Fingerprint};

/// The port a host listens on unless told otherwise.
pub const DEFAULT_PORT: u16 = 47810;

/// How long a connection waits for anything at all before it counts as
/// lost. Each end pings every `PING_EVERY` (4 s).
pub const SILENCE: Duration = Duration::from_secs(15);
const PING_EVERY: Duration = Duration::from_secs(4);

/// How long a connection thread waits for incoming data before it looks
/// for messages to send. The most a message waits to go out.
const POLL: Duration = Duration::from_millis(10);

/// How long connecting and the TLS and WebSocket handshakes may take.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);

/// The name the certificate is made for. Joiners check the fingerprint,
/// not the name.
const CERT_NAME: &str = "axcenity-team-create";

/// The largest message a host takes (one undo step: a pasted model can be
/// big) and a joiner takes (the whole World when joining).
const MAX_TO_HOST: usize = 256 << 20;
const MAX_TO_CLIENT: usize = 1 << 30;

fn provider() -> Arc<CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

fn ws_config(max: usize) -> WebSocketConfig {
    WebSocketConfig::default()
        .max_message_size(Some(max))
        .max_frame_size(Some(max))
}

/// The host's TLS certificate and its private key.
#[derive(Clone, Serialize, Deserialize)]
pub struct HostCert {
    /// The certificate (DER), in hex.
    cert: String,
    /// Its private key (PKCS#8 DER), in hex.
    key: String,
}

impl HostCert {
    /// A new self-signed certificate.
    pub fn generate() -> HostCert {
        let made = rcgen::generate_simple_self_signed(vec![CERT_NAME.to_owned()])
            .expect("making a self-signed certificate");
        HostCert {
            cert: identity::to_hex(made.cert.der()),
            key: identity::to_hex(&made.signing_key.serialize_der()),
        }
    }

    /// The certificate saved at `path`, or a new one saved there.
    pub fn load_or_make(path: &Path) -> io::Result<HostCert> {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).map_err(io::Error::other),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let made = HostCert::generate();
                if let Some(folder) = path.parent() {
                    std::fs::create_dir_all(folder)?;
                }
                let text = serde_json::to_string_pretty(&made).map_err(io::Error::other)?;
                identity::write_private(path, text.as_bytes())?;
                Ok(made)
            }
            Err(error) => Err(error),
        }
    }

    fn der(&self) -> Vec<u8> {
        identity::from_hex(&self.cert).unwrap_or_default()
    }

    /// What invite links carry and joiners check.
    pub fn fingerprint(&self) -> Fingerprint {
        Fingerprint::of(&self.der())
    }

    fn server_config(&self) -> io::Result<Arc<ServerConfig>> {
        let key = identity::from_hex(&self.key).unwrap_or_default();
        let config = ServerConfig::builder_with_provider(provider())
            .with_protocol_versions(&[&rustls::version::TLS13])
            .map_err(io::Error::other)?
            .with_no_client_auth()
            .with_single_cert(
                vec![CertificateDer::from(self.der())],
                PrivatePkcs8KeyDer::from(key).into(),
            )
            .map_err(io::Error::other)?;
        Ok(Arc::new(config))
    }
}

/// One connection to a host, as the host numbers them.
pub type ConnId = u64;

/// What happened on a host's connections.
#[derive(Debug, PartialEq, Eq)]
pub enum NetEvent {
    Opened(ConnId),
    Message(ConnId, Vec<u8>),
    Closed(ConnId),
}

enum Out {
    Bytes(Vec<u8>),
    Close,
}

/// A host's listening socket and its connections.
pub struct Listener {
    address: SocketAddr,
    events: Option<Receiver<NetEvent>>,
    senders: Arc<Mutex<HashMap<ConnId, Sender<Out>>>>,
    stop: Arc<AtomicBool>,
}

impl Listener {
    /// Listens on `address` (port 0 picks a free one) with `cert`.
    pub fn bind(address: SocketAddr, cert: &HostCert) -> io::Result<Listener> {
        let tls = cert.server_config()?;
        let socket = TcpListener::bind(address)?;
        socket.set_nonblocking(true)?;
        let address = socket.local_addr()?;
        let (events_in, events) = mpsc::channel();
        let senders: Arc<Mutex<HashMap<ConnId, Sender<Out>>>> = Arc::default();
        let stop = Arc::new(AtomicBool::new(false));
        let next = Arc::new(AtomicU64::new(1));
        {
            let senders = senders.clone();
            let stop = stop.clone();
            std::thread::Builder::new()
                .name("team-create-listen".into())
                .spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        match socket.accept() {
                            Ok((stream, _)) => {
                                let id = next.fetch_add(1, Ordering::Relaxed);
                                let (out_in, out) = mpsc::channel();
                                senders.lock().expect("senders").insert(id, out_in);
                                let tls = tls.clone();
                                let events = events_in.clone();
                                let senders = senders.clone();
                                let _ = std::thread::Builder::new()
                                    .name(format!("team-create-conn-{id}"))
                                    .spawn(move || {
                                        serve(id, stream, tls, out, &events);
                                        senders.lock().expect("senders").remove(&id);
                                    });
                            }
                            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                                std::thread::sleep(Duration::from_millis(20));
                            }
                            Err(error) => {
                                log::warn!("Team Create: accepting a connection failed: {error}");
                                std::thread::sleep(Duration::from_millis(200));
                            }
                        }
                    }
                })?;
        }
        Ok(Listener {
            address,
            events: Some(events),
            senders,
            stop,
        })
    }

    /// Where it listens (with the port picked, if it was 0).
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    /// The events channel, for a thread that waits on it. After this,
    /// [`poll`](Self::poll) returns nothing.
    pub fn take_events(&mut self) -> Option<Receiver<NetEvent>> {
        self.events.take()
    }

    /// What happened since the last call.
    pub fn poll(&self) -> Vec<NetEvent> {
        self.events
            .as_ref()
            .map(|events| events.try_iter().collect())
            .unwrap_or_default()
    }

    /// Sends `bytes` on connection `conn`, if it's still open.
    pub fn send(&self, conn: ConnId, bytes: Vec<u8>) {
        if let Some(sender) = self.senders.lock().expect("senders").get(&conn) {
            let _ = sender.send(Out::Bytes(bytes));
        }
    }

    /// Closes connection `conn` once what was sent before has gone.
    pub fn close(&self, conn: ConnId) {
        if let Some(sender) = self.senders.lock().expect("senders").get(&conn) {
            let _ = sender.send(Out::Close);
        }
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // Each connection's thread sees its channel close and closes.
        self.senders.lock().expect("senders").clear();
    }
}

/// One connection on the host, until it closes.
fn serve(
    id: ConnId,
    stream: TcpStream,
    tls: Arc<ServerConfig>,
    out: Receiver<Out>,
    events: &Sender<NetEvent>,
) {
    let opened = (|| -> io::Result<WebSocket<StreamOwned<ServerConnection, TcpStream>>> {
        stream.set_nonblocking(false)?;
        stream.set_nodelay(true)?;
        stream.set_read_timeout(Some(CONNECT_TIMEOUT))?;
        let conn = ServerConnection::new(tls).map_err(io::Error::other)?;
        let ws = tungstenite::accept_with_config(
            StreamOwned::new(conn, stream),
            Some(ws_config(MAX_TO_HOST)),
        )
        .map_err(|error| io::Error::other(error.to_string()))?;
        ws.get_ref().sock.set_read_timeout(Some(POLL))?;
        Ok(ws)
    })();
    let ws = match opened {
        Ok(ws) => ws,
        Err(error) => {
            log::info!("Team Create: a connection failed to open: {error}");
            return;
        }
    };
    if events.send(NetEvent::Opened(id)).is_err() {
        return;
    }
    pump(ws, &out, |bytes| {
        events.send(NetEvent::Message(id, bytes)).is_ok()
    });
    let _ = events.send(NetEvent::Closed(id));
}

/// Moves messages both ways until the connection closes, `out` closes or
/// says to close, or `deliver` says nobody is listening.
fn pump<S: Read + Write>(
    mut ws: WebSocket<S>,
    out: &Receiver<Out>,
    mut deliver: impl FnMut(Vec<u8>) -> bool,
) {
    let mut heard = Instant::now();
    let mut pinged = Instant::now();
    loop {
        loop {
            match out.try_recv() {
                Ok(Out::Bytes(bytes)) => {
                    if ws.write(Message::Binary(bytes.into())).is_err() {
                        return;
                    }
                }
                Ok(Out::Close) | Err(TryRecvError::Disconnected) => {
                    let _ = ws.close(None);
                    let _ = ws.flush();
                    return;
                }
                Err(TryRecvError::Empty) => break,
            }
        }
        if pinged.elapsed() >= PING_EVERY {
            pinged = Instant::now();
            if ws.write(Message::Ping(Default::default())).is_err() {
                return;
            }
        }
        match ws.flush() {
            Ok(()) => {}
            Err(tungstenite::Error::Io(error)) if waiting(&error) => {}
            Err(_) => return,
        }
        match ws.read() {
            Ok(message) => {
                heard = Instant::now();
                let bytes = match message {
                    Message::Binary(bytes) => bytes.to_vec(),
                    Message::Text(text) => text.as_bytes().to_vec(),
                    Message::Close(_) => return,
                    Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => continue,
                };
                if !deliver(bytes) {
                    let _ = ws.close(None);
                    let _ = ws.flush();
                    return;
                }
            }
            Err(tungstenite::Error::Io(error)) if waiting(&error) => {
                if heard.elapsed() > SILENCE {
                    return;
                }
            }
            Err(_) => return,
        }
    }
}

/// True for the errors a read timeout gives (which kind depends on the OS).
fn waiting(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted
    )
}

/// Why a [`Link`] closed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkClosed {
    /// Nothing answered at the address (or it stopped answering).
    Unreachable(String),
    /// Something answered with a certificate other than the one pinned:
    /// not the host the invite named, or a host that made a new one.
    WrongHost,
    /// The connection was open and then closed or went quiet.
    Lost,
}

impl std::fmt::Display for LinkClosed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LinkClosed::Unreachable(why) => write!(f, "couldn't reach the host ({why})"),
            LinkClosed::WrongHost => write!(
                f,
                "the computer at that address isn't the host the invite names (its certificate \
                 differs): ask for a new invite"
            ),
            LinkClosed::Lost => write!(f, "the connection to the host was lost"),
        }
    }
}

/// What happened on a [`Link`].
#[derive(Debug, PartialEq, Eq)]
pub enum LinkEvent {
    Opened,
    Message(Vec<u8>),
    Closed(LinkClosed),
}

/// A joiner's connection to a host.
pub struct Link {
    events: Receiver<LinkEvent>,
    out: Sender<Out>,
}

impl Link {
    /// Starts connecting to `host` ("address:port"), accepting only a
    /// certificate whose fingerprint is `cert`. Events say how it went.
    pub fn open(host: &str, cert: Fingerprint) -> Link {
        let (events_in, events) = mpsc::channel();
        let (out_in, out) = mpsc::channel();
        let host = host.to_owned();
        let spawned = std::thread::Builder::new()
            .name("team-create-link".into())
            .spawn(move || {
                let wrong = Arc::new(AtomicBool::new(false));
                let ws = match connect(&host, cert, wrong.clone()) {
                    Ok(ws) => ws,
                    Err(error) => {
                        let why = if wrong.load(Ordering::Relaxed) {
                            LinkClosed::WrongHost
                        } else {
                            LinkClosed::Unreachable(error.to_string())
                        };
                        let _ = events_in.send(LinkEvent::Closed(why));
                        return;
                    }
                };
                if events_in.send(LinkEvent::Opened).is_err() {
                    return;
                }
                pump(ws, &out, |bytes| {
                    events_in.send(LinkEvent::Message(bytes)).is_ok()
                });
                let _ = events_in.send(LinkEvent::Closed(LinkClosed::Lost));
            });
        if let Err(error) = spawned {
            log::warn!("Team Create: couldn't start a connection thread: {error}");
        }
        Link {
            events,
            out: out_in,
        }
    }

    /// What happened since the last call.
    pub fn poll(&self) -> Vec<LinkEvent> {
        self.events.try_iter().collect()
    }

    /// Waits up to `timeout` for the next event.
    pub fn wait(&self, timeout: Duration) -> Option<LinkEvent> {
        self.events.recv_timeout(timeout).ok()
    }

    pub fn send(&self, bytes: Vec<u8>) {
        let _ = self.out.send(Out::Bytes(bytes));
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        let _ = self.out.send(Out::Close);
    }
}

fn connect(
    host: &str,
    cert: Fingerprint,
    wrong: Arc<AtomicBool>,
) -> io::Result<WebSocket<StreamOwned<ClientConnection, TcpStream>>> {
    let mut last = io::Error::new(io::ErrorKind::NotFound, "no address");
    let mut stream = None;
    for address in host.to_socket_addrs()? {
        match TcpStream::connect_timeout(&address, CONNECT_TIMEOUT) {
            Ok(s) => {
                stream = Some(s);
                break;
            }
            Err(error) => last = error,
        }
    }
    let stream = stream.ok_or(last)?;
    stream.set_nodelay(true)?;
    stream.set_read_timeout(Some(CONNECT_TIMEOUT))?;
    let provider = provider();
    let verifier = Pinned {
        cert,
        algorithms: provider.signature_verification_algorithms,
        wrong,
    };
    let config = ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(io::Error::other)?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth();
    let name = ServerName::try_from(CERT_NAME).map_err(io::Error::other)?;
    let conn = ClientConnection::new(Arc::new(config), name).map_err(io::Error::other)?;
    let (ws, _) = tungstenite::client::client_with_config(
        format!("wss://{host}/teamcreate"),
        StreamOwned::new(conn, stream),
        Some(ws_config(MAX_TO_CLIENT)),
    )
    .map_err(|error| io::Error::other(error.to_string()))?;
    ws.get_ref().sock.set_read_timeout(Some(POLL))?;
    Ok(ws)
}

/// Accepts exactly one certificate, by fingerprint; the TLS signatures are
/// still checked against it, so only the holder of its key can answer.
#[derive(Debug)]
struct Pinned {
    cert: Fingerprint,
    algorithms: WebPkiSupportedAlgorithms,
    wrong: Arc<AtomicBool>,
}

impl ServerCertVerifier for Pinned {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        if Fingerprint::of(end_entity.as_ref()) == self.cert {
            Ok(ServerCertVerified::assertion())
        } else {
            self.wrong.store(true, Ordering::Relaxed);
            Err(rustls::Error::InvalidCertificate(
                rustls::CertificateError::ApplicationVerificationFailure,
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}
