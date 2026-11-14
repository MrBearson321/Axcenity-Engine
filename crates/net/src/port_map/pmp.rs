//! PCP (RFC 6887) and NAT-PMP (RFC 6886) port mapping: small UDP
//! requests to the router on port 5351. PCP is asked first; a router that
//! only speaks NAT-PMP answers it with version 0, and is asked again in
//! NAT-PMP.

use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::time::Duration;

use ring::rand::{SecureRandom, SystemRandom};

use super::{LEASE_SECONDS, MapMethod};

/// The port routers take PCP and NAT-PMP requests on.
pub(super) const PORT: u16 = 5351;
/// Waits between tries of one request (RFC 6886 starts at 250 ms and
/// doubles; three tries keep a silent router under two seconds).
const TRIES: [Duration; 3] = [
    Duration::from_millis(250),
    Duration::from_millis(500),
    Duration::from_millis(1000),
];

const PCP_VERSION: u8 = 2;
const PCP_MAP: u8 = 1;
const UDP: u8 = 17;
const TCP: u8 = 6;

/// The PCP or NAT-PMP port on `router`.
pub(super) fn at(router: Ipv4Addr) -> SocketAddr {
    SocketAddrV4::new(router, PORT).into()
}

/// How a mapping was made.
enum Protocol {
    /// PCP, with the nonce of each mapping (UDP, TCP), which renewing and
    /// removing must repeat, and this computer's address on the router's
    /// side.
    Pcp {
        nonces: [[u8; 12]; 2],
        local: Ipv4Addr,
    },
    NatPmp,
}

/// The game's port, mapped on a router over PCP or NAT-PMP.
pub(super) struct Mapped {
    socket: UdpSocket,
    protocol: Protocol,
    port: u16,
    external: SocketAddrV4,
    lease: u32,
}

impl Mapped {
    pub fn external(&self) -> SocketAddr {
        self.external.into()
    }

    pub fn method(&self) -> MapMethod {
        match self.protocol {
            Protocol::Pcp { .. } => MapMethod::Pcp,
            Protocol::NatPmp => MapMethod::NatPmp,
        }
    }

    pub fn lease(&self) -> u32 {
        self.lease
    }

    pub fn renew(&mut self) -> Result<(), String> {
        let (external, lease) = self.ask(self.external.port(), LEASE_SECONDS)?;
        if external.port() != self.external.port() {
            return Err(format!(
                "the router moved the game to outside port {}",
                external.port()
            ));
        }
        // NAT-PMP's mapping answer has no address: the one it had stays.
        if let Protocol::Pcp { .. } = self.protocol {
            self.external = external;
        }
        self.lease = lease;
        Ok(())
    }

    pub fn remove(&self) {
        let _ = self.ask(self.external.port(), 0);
    }

    /// Maps both protocols to `suggested` outside for `lease` seconds (0
    /// removes them); where they were mapped, and the shorter lease.
    fn ask(&self, suggested: u16, lease: u32) -> Result<(SocketAddrV4, u32), String> {
        let mut external = None;
        let mut shortest = u32::MAX;
        // TCP is asked for where UDP went, so both share one port.
        let mut suggested = suggested;
        for (index, protocol) in [UDP, TCP].into_iter().enumerate() {
            let (address, given) = match &self.protocol {
                Protocol::Pcp { nonces, local } => pcp_map(
                    &self.socket,
                    &nonces[index],
                    *local,
                    protocol,
                    self.port,
                    suggested,
                    lease,
                )?,
                Protocol::NatPmp => {
                    // NAT-PMP's remove names no outside port.
                    let suggested = if lease == 0 { 0 } else { suggested };
                    nat_pmp_map(&self.socket, protocol, self.port, suggested, lease)?
                }
            };
            if lease != 0 {
                if external.is_some_and(|external: SocketAddrV4| external.port() != address.port())
                {
                    return Err(format!(
                        "the router gave the game's UDP and TCP different outside ports ({} and {})",
                        external.map_or(0, |e| e.port()),
                        address.port()
                    ));
                }
                external = Some(address);
                suggested = address.port();
            }
            shortest = shortest.min(given);
        }
        Ok((
            external.unwrap_or(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0)),
            shortest,
        ))
    }
}

/// Maps UDP and TCP `port` on the router at `router` (PCP's address and
/// port), with PCP or, for a router that only speaks it, NAT-PMP.
pub(super) fn map(port: u16, router: SocketAddr) -> Result<Mapped, String> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))
        .and_then(|socket| socket.connect(router).map(|()| socket))
        .map_err(|err| format!("can't ask: {err}"))?;
    let local = match socket.local_addr().map(|address| address.ip()) {
        Ok(IpAddr::V4(local)) => local,
        _ => return Err("no address of this computer reaches it".to_owned()),
    };
    let random = SystemRandom::new();
    let mut nonces = [[0u8; 12]; 2];
    for nonce in &mut nonces {
        random
            .fill(nonce)
            .map_err(|_| "no random numbers".to_owned())?;
    }
    let mut mapped = Mapped {
        socket,
        protocol: Protocol::Pcp { nonces, local },
        port,
        external: SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port),
        lease: LEASE_SECONDS,
    };
    let asked = match mapped.ask(port, LEASE_SECONDS) {
        Err(why) if why == NAT_PMP_ONLY => {
            mapped.protocol = Protocol::NatPmp;
            mapped.ask(port, LEASE_SECONDS)
        }
        asked => asked,
    };
    let (mut external, lease) = asked.inspect_err(|_| mapped.remove())?;
    if let Protocol::NatPmp = mapped.protocol {
        // NAT-PMP's mapping answer has no address; it has a request of
        // its own.
        external.set_ip(nat_pmp_address(&mapped.socket).inspect_err(|_| mapped.remove())?);
    }
    mapped.external = external;
    mapped.lease = lease;
    Ok(mapped)
}

/// What `pcp_map` says when the router answers in NAT-PMP.
const NAT_PMP_ONLY: &str = "the router only speaks NAT-PMP";

/// Sends `request` until an answer `fits` (or the tries run out).
fn exchange(
    socket: &UdpSocket,
    request: &[u8],
    fits: impl Fn(&[u8]) -> bool,
) -> Result<Vec<u8>, String> {
    let mut buffer = [0u8; 1100];
    for wait in TRIES {
        socket
            .send(request)
            .map_err(|err| format!("can't ask: {err}"))?;
        let deadline = std::time::Instant::now() + wait;
        loop {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            if left.is_zero() {
                break;
            }
            let _ = socket.set_read_timeout(Some(left));
            match socket.recv(&mut buffer) {
                Ok(len) if fits(&buffer[..len]) => return Ok(buffer[..len].to_vec()),
                Ok(_) => {}
                // Nothing listening there (an ICMP unreachable came back).
                Err(err) if err.kind() == std::io::ErrorKind::ConnectionRefused => {
                    return Err("nothing answers there".to_owned());
                }
                Err(_) => break,
            }
        }
    }
    Err("no answer".to_owned())
}

/// An IPv4 address as PCP writes it: IPv4-mapped IPv6.
fn mapped_v4(ip: Ipv4Addr) -> [u8; 16] {
    ip.to_ipv6_mapped().octets()
}

/// One PCP MAP request: where the router mapped `protocol`, and for how
/// long.
fn pcp_map(
    socket: &UdpSocket,
    nonce: &[u8; 12],
    local: Ipv4Addr,
    protocol: u8,
    port: u16,
    suggested: u16,
    lease: u32,
) -> Result<(SocketAddrV4, u32), String> {
    let mut request = Vec::with_capacity(60);
    request.extend([PCP_VERSION, PCP_MAP, 0, 0]);
    request.extend(lease.to_be_bytes());
    request.extend(mapped_v4(local));
    request.extend(nonce);
    request.extend([protocol, 0, 0, 0]);
    request.extend(port.to_be_bytes());
    request.extend(suggested.to_be_bytes());
    request.extend(mapped_v4(Ipv4Addr::UNSPECIFIED));
    let answer = exchange(socket, &request, |answer| {
        // A NAT-PMP router's "unsupported version", or the answer to
        // this request (same nonce, protocol and port).
        (answer.len() >= 4 && answer[0] == 0)
            || (answer.len() >= 60
                && answer[0] == PCP_VERSION
                && answer[1] == 0x80 | PCP_MAP
                && &answer[24..36] == nonce
                && answer[36] == protocol
                && answer[40..42] == port.to_be_bytes())
    })?;
    if answer[0] == 0 {
        return Err(NAT_PMP_ONLY.to_owned());
    }
    match answer[3] {
        0 => {}
        code => return Err(pcp_refusal(code)),
    }
    let given = u32::from_be_bytes([answer[4], answer[5], answer[6], answer[7]]);
    let external_port = u16::from_be_bytes([answer[42], answer[43]]);
    let mut ip = [0u8; 16];
    ip.copy_from_slice(&answer[44..60]);
    let ip = std::net::Ipv6Addr::from(ip)
        .to_ipv4_mapped()
        .ok_or("the router's outside address isn't IPv4")?;
    Ok((SocketAddrV4::new(ip, external_port), given))
}

fn pcp_refusal(code: u8) -> String {
    let why = match code {
        1 => "it doesn't speak this PCP version",
        2 => "it doesn't allow mappings (NOT_AUTHORIZED)",
        3 | 6 => "it couldn't read the request",
        4 | 5 => "it doesn't map ports",
        7 => "it has no outside connection right now",
        8 | 10 => "it has no room for another mapping",
        9 => "it doesn't map this protocol",
        11 => "it can't give this outside port",
        12 => "it sees this computer at another address (another router in between?)",
        _ => "an unknown refusal",
    };
    format!("the router refused: {why} (PCP {code})")
}

/// One NAT-PMP mapping request.
fn nat_pmp_map(
    socket: &UdpSocket,
    protocol: u8,
    port: u16,
    suggested: u16,
    lease: u32,
) -> Result<(SocketAddrV4, u32), String> {
    let opcode = if protocol == UDP { 1 } else { 2 };
    let mut request = vec![0, opcode, 0, 0];
    request.extend(port.to_be_bytes());
    request.extend(suggested.to_be_bytes());
    request.extend(lease.to_be_bytes());
    let answer = exchange(socket, &request, |answer| {
        answer.len() >= 16
            && answer[0] == 0
            && answer[1] == 0x80 | opcode
            && answer[8..10] == port.to_be_bytes()
    })?;
    nat_pmp_result(&answer)?;
    let external_port = u16::from_be_bytes([answer[10], answer[11]]);
    let given = u32::from_be_bytes([answer[12], answer[13], answer[14], answer[15]]);
    Ok((
        SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, external_port),
        given,
    ))
}

/// NAT-PMP's outside address request.
fn nat_pmp_address(socket: &UdpSocket) -> Result<Ipv4Addr, String> {
    let answer = exchange(socket, &[0, 0], |answer| {
        answer.len() >= 12 && answer[0] == 0 && answer[1] == 0x80
    })?;
    nat_pmp_result(&answer)?;
    Ok(Ipv4Addr::new(answer[8], answer[9], answer[10], answer[11]))
}

fn nat_pmp_result(answer: &[u8]) -> Result<(), String> {
    let why = match u16::from_be_bytes([answer[2], answer[3]]) {
        0 => return Ok(()),
        1 => "it doesn't speak this NAT-PMP version",
        2 => "it doesn't allow mappings",
        3 => "it has no outside connection right now",
        4 => "it has no room for another mapping",
        5 => "it doesn't map ports",
        _ => "an unknown refusal",
    };
    Err(format!("the router refused: {why}"))
}
