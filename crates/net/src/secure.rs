//! Secure connections (multiplayer readiness MP4, ADR-0026): a server on
//! the network hands out its own netcode connect tokens over TLS, and
//! netcode then encrypts every packet with the keys in the token.
//!
//! A player opens TLS to the server's **token port** (TCP, the same number
//! as the game's UDP port) and checks the server's certificate by its
//! fingerprint: from a join link (`host:port#fingerprint`), else the one
//! remembered for that address (the first seen is kept, as SSH does).
//! It sends a [`TokenRequest`] (the join request's game, version and
//! password) and gets a connect token valid for [`TOKEN_SECONDS`], or the
//! refusal, worded as the join handshake words it. A wrong password never
//! gets a token, and nothing without one can open a connection.
//!
//! The certificate and its key live in the host's data folder, never in
//! the project. The key that seals tokens is new each run.

use std::collections::{BTreeMap, HashMap};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use renet_netcode::{ConnectToken, NETCODE_KEY_BYTES};
use ring::digest;
use ring::rand::{SecureRandom, SystemRandom};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, WebPkiSupportedAlgorithms};
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use rustls::{
    ClientConfig, ClientConnection, DigitallySignedStruct, ServerConfig, ServerConnection,
    SignatureScheme, StreamOwned,
};
use serde::{Deserialize, Serialize};

use crate::handshake::{Admission, GameInfo, Message, admit};

/// How long a connect token may wait before it's used.
pub const TOKEN_SECONDS: u64 = 30;
/// How long a connection made with a token may go without a packet.
const TIMEOUT_SECONDS: i32 = 15;
/// How long reaching the token port and its answer may take.
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(5);
/// The most bytes of a request or an answer.
const MOST_BYTES: u64 = 8 * 1024;
/// Token requests answered at once; more wait for the operating system.
const MOST_AT_ONCE: usize = 64;
/// Tokens one address may get a minute: more than a person joining and
/// rejoining needs, few enough that one computer can't fill a server.
/// This computer's own addresses aren't limited (bots and scale tests run
/// many players from one).
const TOKENS_PER_MINUTE: u32 = 30;
/// The name in the certificate; nobody checks it (the fingerprint is the
/// identity).
const CERT_NAME: &str = "axcenity-game-server";

fn provider() -> Arc<CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

/// A certificate's fingerprint: SHA-256 of it, in lowercase hex, as join
/// links carry it.
pub fn fingerprint_of(der: &[u8]) -> String {
    digest::digest(&digest::SHA256, der)
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The first 16 hex digits of a fingerprint, in groups of four, for
/// people to compare.
pub fn short(fingerprint: &str) -> String {
    let digits: Vec<char> = fingerprint.chars().take(16).collect();
    digits
        .chunks(4)
        .map(|group| group.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join("-")
}

/// A server's TLS certificate and its private key.
#[derive(Clone, Serialize, Deserialize)]
pub struct ServerCert {
    /// The certificate (DER), in hex.
    cert: String,
    /// Its private key (PKCS#8 DER), in hex.
    key: String,
}

impl ServerCert {
    /// A new self-signed certificate.
    pub fn generate() -> ServerCert {
        let made = rcgen::generate_simple_self_signed(vec![CERT_NAME.to_owned()])
            .expect("making a self-signed certificate");
        ServerCert {
            cert: to_hex(made.cert.der()),
            key: to_hex(&made.signing_key.serialize_der()),
        }
    }

    /// The certificate saved at `path`, or a new one saved there (readable
    /// only by this user).
    pub fn load_or_make(path: &Path) -> io::Result<ServerCert> {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).map_err(io::Error::other),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let made = ServerCert::generate();
                if let Some(folder) = path.parent() {
                    std::fs::create_dir_all(folder)?;
                }
                let text = serde_json::to_string_pretty(&made).map_err(io::Error::other)?;
                write_private(path, text.as_bytes())?;
                Ok(made)
            }
            Err(error) => Err(error),
        }
    }

    fn der(&self) -> Vec<u8> {
        from_hex(&self.cert).unwrap_or_default()
    }

    /// What join links carry and players check.
    pub fn fingerprint(&self) -> String {
        fingerprint_of(&self.der())
    }

    fn server_config(&self) -> io::Result<Arc<ServerConfig>> {
        let key = from_hex(&self.key).unwrap_or_default();
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

/// What a player sends the token port: the join request's checks, and the
/// address it will send its packets to (the token names it, so the player
/// connects there).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TokenRequest {
    pub protocol: u32,
    pub game: String,
    pub version: String,
    #[serde(default)]
    pub defaults: u64,
    pub password: Option<String>,
    pub address: SocketAddr,
}

impl TokenRequest {
    /// A request for `game` with `password`, to connect to `address`.
    pub fn new(game: &GameInfo, password: Option<String>, address: SocketAddr) -> TokenRequest {
        TokenRequest {
            protocol: crate::PROTOCOL_VERSION,
            game: game.name.clone(),
            version: game.version.clone(),
            defaults: crate::handshake::class_defaults(),
            password,
            address,
        }
    }
}

/// The token port's answer: a token (hex), or why not.
#[derive(Serialize, Deserialize)]
struct TokenAnswer {
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    refused: Option<String>,
}

/// The address every Axcenity server lists as its own in tokens besides
/// the one the player asked for: netcode checks that a token names the
/// server, and a server behind a router doesn't know the address players
/// reach it at. The token's key, new each run, is what ties a token to
/// one server.
fn listed(port: u16) -> SocketAddr {
    (Ipv4Addr::UNSPECIFIED, port).into()
}

/// What a secure server needs: its certificate and the key that seals its
/// tokens.
pub struct Secured {
    pub cert: ServerCert,
    pub(crate) key: [u8; NETCODE_KEY_BYTES],
}

impl Secured {
    /// `cert`, with a new token key.
    pub fn new(cert: ServerCert) -> Secured {
        let mut key = [0u8; NETCODE_KEY_BYTES];
        SystemRandom::new()
            .fill(&mut key)
            .expect("the system's random numbers");
        Secured { cert, key }
    }

    /// The addresses netcode takes as this server's in a token.
    pub(crate) fn public_addresses(port: u16) -> Vec<SocketAddr> {
        vec![listed(port)]
    }
}

/// Who may get a token: the game and its password (how full the game is
/// waits for the join request, which comes next).
#[derive(Clone)]
pub struct TokenRules {
    pub game: GameInfo,
    pub password: Option<String>,
}

/// A server's token port: answers requests on threads of its own until
/// dropped.
pub struct TokenPort {
    #[cfg(test)]
    port: u16,
    issuer: Arc<Issuer>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    issued: Arc<AtomicUsize>,
    log: Arc<Mutex<Vec<String>>>,
}

impl TokenPort {
    /// Listens on TCP `port` of every address of this computer.
    pub fn start(port: u16, secured: &Secured, rules: TokenRules) -> io::Result<TokenPort> {
        let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, port))?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let config = secured.cert.server_config()?;
        let stop = Arc::new(AtomicBool::new(false));
        let issued = Arc::new(AtomicUsize::new(0));
        let log = Arc::new(Mutex::new(Vec::new()));
        let shared = Arc::new(Issuer {
            config,
            key: secured.key,
            port,
            rules,
            busy: AtomicUsize::new(0),
            recent: Recent::default(),
            issued: issued.clone(),
            log: log.clone(),
        });
        let stopping = stop.clone();
        let issuer = shared.clone();
        let thread = std::thread::Builder::new()
            .name("token port".to_owned())
            .spawn(move || accept(listener, shared, stopping))?;
        Ok(TokenPort {
            #[cfg(test)]
            port,
            issuer,
            stop,
            thread: Some(thread),
            issued,
            log,
        })
    }

    #[cfg(test)]
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Answers token requests that come another way (through a relay).
    pub(crate) fn handle(&self) -> TokenHandle {
        TokenHandle(self.issuer.clone())
    }

    /// Tokens handed out so far.
    pub fn issued(&self) -> usize {
        self.issued.load(Ordering::Relaxed)
    }

    /// What happened since the last call, for the host's log.
    pub fn take_log(&self) -> Vec<String> {
        self.log
            .lock()
            .map(|mut log| std::mem::take(&mut *log))
            .unwrap_or_default()
    }
}

impl Drop for TokenPort {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// A token port's answers, for requests through a relay (MP5).
#[derive(Clone)]
pub(crate) struct TokenHandle(Arc<Issuer>);

impl TokenHandle {
    /// Answers the request on `stream` from a player at `from`, on this
    /// thread; false when too many are being answered already.
    pub(crate) fn answer(&self, stream: TcpStream, from: SocketAddr) -> bool {
        let issuer = &self.0;
        if issuer.busy.fetch_add(1, Ordering::Relaxed) >= MOST_AT_ONCE {
            issuer.busy.fetch_sub(1, Ordering::Relaxed);
            return false;
        }
        let _ = answer(stream, from, issuer);
        issuer.busy.fetch_sub(1, Ordering::Relaxed);
        true
    }

    pub(crate) fn note(&self, line: String) {
        self.0.note(line);
    }
}

struct Issuer {
    config: Arc<ServerConfig>,
    key: [u8; NETCODE_KEY_BYTES],
    port: u16,
    rules: TokenRules,
    busy: AtomicUsize,
    recent: Recent,
    issued: Arc<AtomicUsize>,
    log: Arc<Mutex<Vec<String>>>,
}

fn accept(listener: TcpListener, issuer: Arc<Issuer>, stop: Arc<AtomicBool>) {
    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, from)) => {
                if issuer.busy.load(Ordering::Relaxed) >= MOST_AT_ONCE {
                    continue;
                }
                issuer.busy.fetch_add(1, Ordering::Relaxed);
                let mine = issuer.clone();
                let spawned = std::thread::Builder::new()
                    .name("token request".to_owned())
                    .spawn(move || {
                        let _ = answer(stream, from, &mine);
                        mine.busy.fetch_sub(1, Ordering::Relaxed);
                    });
                if spawned.is_err() {
                    issuer.busy.fetch_sub(1, Ordering::Relaxed);
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(_) => std::thread::sleep(Duration::from_millis(20)),
        }
    }
}

/// Answers one token request.
fn answer(stream: TcpStream, from: SocketAddr, issuer: &Issuer) -> io::Result<()> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(FETCH_TIMEOUT))?;
    stream.set_write_timeout(Some(FETCH_TIMEOUT))?;
    stream.set_nodelay(true)?;
    let conn = ServerConnection::new(issuer.config.clone()).map_err(io::Error::other)?;
    let mut tls = StreamOwned::new(conn, stream);
    let mut line = String::new();
    BufReader::new((&mut tls).take(MOST_BYTES)).read_line(&mut line)?;
    let reply = match serde_json::from_str::<TokenRequest>(line.trim()) {
        Ok(request) => issuer.decide(&request, from.ip()),
        Err(_) => TokenAnswer {
            token: None,
            refused: Some("The server didn't understand the join request.".to_owned()),
        },
    };
    let mut bytes = serde_json::to_vec(&reply).map_err(io::Error::other)?;
    bytes.push(b'\n');
    tls.write_all(&bytes)?;
    tls.flush()?;
    tls.conn.send_close_notify();
    let _ = tls.flush();
    Ok(())
}

impl Issuer {
    fn decide(&self, request: &TokenRequest, from: IpAddr) -> TokenAnswer {
        let refused = |reason: String| TokenAnswer {
            token: None,
            refused: Some(reason),
        };
        let hello = Message::Hello {
            protocol: request.protocol,
            game: request.game.clone(),
            version: request.version.clone(),
            name: String::new(),
            password: request.password.clone(),
            user_id: 0,
            defaults: request.defaults,
            rejoin: 0,
        };
        let admission = Admission {
            game: &self.rules.game,
            password: self.rules.password.as_deref(),
            players: 0,
            max_players: 1,
        };
        if let Err(refusal) = admit(Some(&hello), &admission) {
            self.note(format!("Token port {}", refusal.for_host));
            return refused(refusal.to_player);
        }
        if !self.recent.allow(from, Instant::now()) {
            self.note(format!(
                "Token port refused {from}: more than {TOKENS_PER_MINUTE} tokens in a minute"
            ));
            return refused("Too many joins from your address: wait a minute.".to_owned());
        }
        let mut client_id = [0u8; 8];
        if SystemRandom::new().fill(&mut client_id).is_err() {
            return refused("The server couldn't make a token.".to_owned());
        }
        let token = ConnectToken::generate(
            unix_now(),
            crate::transport::NETCODE_PROTOCOL_ID,
            TOKEN_SECONDS,
            u64::from_le_bytes(client_id),
            TIMEOUT_SECONDS,
            vec![request.address, listed(self.port)],
            None,
            &self.key,
        );
        let mut bytes = Vec::new();
        match token.map(|token| token.write(&mut bytes)) {
            Ok(Ok(())) => {
                self.issued.fetch_add(1, Ordering::Relaxed);
                TokenAnswer {
                    token: Some(to_hex(&bytes)),
                    refused: None,
                }
            }
            _ => refused("The server couldn't make a token.".to_owned()),
        }
    }

    fn note(&self, line: String) {
        if let Ok(mut log) = self.log.lock() {
            log.push(line);
        }
    }
}

/// Tokens each address got this minute, and when its minute began.
#[derive(Default)]
pub(crate) struct Recent(Mutex<HashMap<IpAddr, (u32, Instant)>>);

impl Recent {
    /// Counts a token for `from` at `now`: false when it has had its
    /// minute's. Loopback addresses always may.
    pub(crate) fn allow(&self, from: IpAddr, now: Instant) -> bool {
        if from.is_loopback() {
            return true;
        }
        let Ok(mut recent) = self.0.lock() else {
            return false;
        };
        recent.retain(|_, (_, since)| now.duration_since(*since) < Duration::from_secs(60));
        let (count, _) = recent.entry(from).or_insert((0, now));
        *count += 1;
        *count <= TOKENS_PER_MINUTE
    }
}

/// Which certificate a player accepts.
#[derive(Clone, Debug, PartialEq)]
pub enum Trust {
    /// Only this fingerprint (from a join link).
    Pinned(String),
    /// The fingerprint remembered for the address in this file, or the
    /// first one seen, which is then remembered.
    Remember(PathBuf),
    /// Any (bots and tests).
    Any,
}

/// Why a player has no token.
#[derive(Clone, Debug, PartialEq)]
pub enum FetchError {
    /// Nothing answers on the token port: the server isn't secured (LAN
    /// hosting, or `--insecure`), or a firewall is in the way.
    NoTokenPort(String),
    /// The server answered and turned the player away, or isn't who it
    /// should be: said to the player as it is.
    Refused(String),
}

/// Asks the token port at `server` for a token, checking it by `trust`.
/// Blocks for up to about [`FETCH_TIMEOUT`] twice: run it on a thread.
pub fn fetch_token(
    server: SocketAddr,
    request: &TokenRequest,
    trust: &Trust,
) -> Result<ConnectToken, FetchError> {
    let stream = TcpStream::connect_timeout(&server, FETCH_TIMEOUT).map_err(|error| {
        FetchError::NoTokenPort(format!(
            "TCP port {} at {}: {error}",
            server.port(),
            server.ip()
        ))
    })?;
    token_over(stream, server, request, trust)
}

/// [`fetch_token`] over `stream`, which reaches the token port of the
/// server known as `server` (directly, or spliced through a relay).
pub(crate) fn token_over(
    stream: TcpStream,
    server: SocketAddr,
    request: &TokenRequest,
    trust: &Trust,
) -> Result<ConnectToken, FetchError> {
    let io_error = |error: io::Error| {
        FetchError::Refused(format!(
            "The server's token port stopped answering: {error}."
        ))
    };
    stream
        .set_read_timeout(Some(FETCH_TIMEOUT))
        .map_err(io_error)?;
    stream
        .set_write_timeout(Some(FETCH_TIMEOUT))
        .map_err(io_error)?;
    stream.set_nodelay(true).map_err(io_error)?;
    let provider = provider();
    let seen = Arc::new(Mutex::new(None));
    let verifier = Recorder {
        pinned: match trust {
            Trust::Pinned(fingerprint) => Some(fingerprint.to_ascii_lowercase()),
            _ => None,
        },
        seen: seen.clone(),
        algorithms: provider.signature_verification_algorithms,
    };
    let config = ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|error| io_error(io::Error::other(error)))?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth();
    let name =
        ServerName::try_from(CERT_NAME).map_err(|error| io_error(io::Error::other(error)))?;
    let conn = ClientConnection::new(Arc::new(config), name)
        .map_err(|error| io_error(io::Error::other(error)))?;
    let mut tls = StreamOwned::new(conn, stream);
    // The handshake first, so the certificate is checked before the
    // password goes out.
    while tls.conn.is_handshaking() {
        if let Err(error) = tls.conn.complete_io(&mut tls.sock) {
            let seen = seen.lock().ok().and_then(|seen| seen.clone());
            return Err(match (trust, seen) {
                (Trust::Pinned(wanted), Some(got)) if !got.eq_ignore_ascii_case(wanted) => {
                    FetchError::Refused(wrong_certificate(server, wanted, &got))
                }
                _ => io_error(error),
            });
        }
    }
    let got = seen
        .lock()
        .ok()
        .and_then(|seen| seen.clone())
        .unwrap_or_default();
    if let Trust::Remember(path) = trust {
        let mut known = KnownServers::load(path);
        match known.servers.get(&server.to_string()) {
            Some(wanted) if !wanted.eq_ignore_ascii_case(&got) => {
                return Err(FetchError::Refused(wrong_certificate(server, wanted, &got)));
            }
            Some(_) => {}
            None => {
                known.servers.insert(server.to_string(), got.clone());
                // Not being able to remember it doesn't stop the join.
                let _ = known.save(path);
            }
        }
    }
    let mut bytes =
        serde_json::to_vec(request).map_err(|error| io_error(io::Error::other(error)))?;
    bytes.push(b'\n');
    tls.write_all(&bytes).map_err(io_error)?;
    tls.flush().map_err(io_error)?;
    let mut line = String::new();
    BufReader::new((&mut tls).take(MOST_BYTES))
        .read_line(&mut line)
        .map_err(io_error)?;
    let answer: TokenAnswer = serde_json::from_str(line.trim()).map_err(|_| {
        FetchError::Refused("The server's token port answered nonsense.".to_owned())
    })?;
    if let Some(reason) = answer.refused {
        return Err(FetchError::Refused(reason));
    }
    let bytes = answer
        .token
        .as_deref()
        .and_then(from_hex)
        .ok_or_else(|| FetchError::Refused("The server sent no token.".to_owned()))?;
    ConnectToken::read(&mut bytes.as_slice())
        .map_err(|_| FetchError::Refused("The server sent a token that can't be read.".to_owned()))
}

fn wrong_certificate(server: SocketAddr, wanted: &str, got: &str) -> String {
    format!(
        "The server at {server} isn't the one you expected: its certificate is {}, not {}. \
         If its owner made a new certificate, join with its new link; otherwise someone \
         may be pretending to be it.",
        short(got),
        short(wanted)
    )
}

/// Accepts any certificate (or only a pinned one) and notes its
/// fingerprint; the TLS signatures are still checked against it, so only
/// the holder of its key can answer.
#[derive(Debug)]
struct Recorder {
    pinned: Option<String>,
    seen: Arc<Mutex<Option<String>>>,
    algorithms: WebPkiSupportedAlgorithms,
}

impl ServerCertVerifier for Recorder {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let fingerprint = fingerprint_of(end_entity.as_ref());
        if let Ok(mut seen) = self.seen.lock() {
            *seen = Some(fingerprint.clone());
        }
        match &self.pinned {
            Some(wanted) if *wanted != fingerprint => Err(rustls::Error::InvalidCertificate(
                rustls::CertificateError::ApplicationVerificationFailure,
            )),
            _ => Ok(ServerCertVerified::assertion()),
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

/// The servers a player has joined, by address, with their certificates'
/// fingerprints (`known-servers.json` in the player's data folder).
#[derive(Default, Serialize, Deserialize)]
struct KnownServers {
    servers: BTreeMap<String, String>,
}

impl KnownServers {
    fn load(path: &Path) -> KnownServers {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        std::fs::write(path, text)
    }
}

/// Whether `server` has a fingerprint remembered in the file at `path`:
/// it was secured when last joined.
pub fn is_known(path: &Path, server: SocketAddr) -> bool {
    KnownServers::load(path)
        .servers
        .contains_key(&server.to_string())
}

/// `"host:port#fingerprint"` or `"axcenity://join/host:port#fingerprint"`
/// as the address and the fingerprint (if any).
pub fn split_join_link(link: &str) -> (String, Option<String>) {
    let link = link.trim();
    let link = link.strip_prefix("axcenity://join/").unwrap_or(link);
    match link.split_once('#') {
        Some((address, fingerprint)) if !fingerprint.trim().is_empty() => (
            address.trim_end_matches('/').to_owned(),
            Some(fingerprint.trim().to_ascii_lowercase()),
        ),
        Some((address, _)) => (address.trim_end_matches('/').to_owned(), None),
        None => (link.trim_end_matches('/').to_owned(), None),
    }
}

/// The link players join a secured server with.
pub fn join_link(address: &str, fingerprint: &str) -> String {
    format!("axcenity://join/{address}#{fingerprint}")
}

fn unix_now() -> Duration {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn from_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) || !text.is_ascii() {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).ok())
        .collect()
}

fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(bytes)
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, bytes)
    }
}
