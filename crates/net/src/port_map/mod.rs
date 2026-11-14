//! Port mapping on the host (multiplayer readiness MP5b, ADR-0026 D2):
//! asking the home router to forward the game's port from the internet,
//! so players outside the host's network can join by address without the
//! host setting up the router by hand.
//!
//! The game's UDP port and its token port (TCP, the same number) are
//! mapped together, to the same outside port, because a join link names
//! one port for both. UPnP IGD is asked first (`upnp`), then PCP and its
//! older form NAT-PMP (`pmp`), which Apple and many other routers speak.
//! Everything runs on a thread of its own: a router can take seconds to
//! answer, or never does. Mappings are leased and renewed at half their
//! lease, and removed when the [`PortMapper`] is dropped (a host that is
//! killed leaves them until the lease ends).

mod pmp;
mod upnp;

#[cfg(test)]
pub(crate) mod tests;

use std::fmt;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// How long a mapping is asked for. Short, so one left behind by a host
/// that was killed goes soon; renewed at half.
const LEASE_SECONDS: u32 = 30 * 60;
/// How long to wait for a router to answer a UPnP search.
const SEARCH_WAIT: Duration = Duration::from_secs(2);
/// After a renewal fails, when to try again.
const RETRY_AFTER: Duration = Duration::from_secs(60);
/// How long dropping a [`PortMapper`] that mapped the port waits for the
/// router to remove the mappings (a router on the home network answers in
/// milliseconds).
const REMOVE_WAIT: Duration = Duration::from_secs(1);
/// What the router shows the mappings as.
const DESCRIPTION: &str = "Axcenity game";

/// How a port was mapped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapMethod {
    Upnp,
    Pcp,
    NatPmp,
}

impl fmt::Display for MapMethod {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            MapMethod::Upnp => "UPnP",
            MapMethod::Pcp => "PCP",
            MapMethod::NatPmp => "NAT-PMP",
        })
    }
}

/// What a [`PortMapper`] reports.
#[derive(Clone, Debug, PartialEq)]
pub enum MapEvent {
    /// The router forwards `external` (its outside address and port) to
    /// the game. `permanent`: the router only takes mappings without a
    /// lease, so it keeps this one until it's removed.
    Mapped {
        external: SocketAddr,
        method: MapMethod,
        permanent: bool,
    },
    /// No router mapped the port: why, for each way tried.
    Failed(String),
    /// A mapping that worked couldn't be renewed (tried again each
    /// minute; a `Mapped` follows if it comes back).
    Lost(String),
}

/// Where to look for routers (tests point these at fakes).
#[derive(Clone, Debug)]
pub(crate) struct Gateways {
    /// Where UPnP searches go: the SSDP multicast address.
    pub ssdp: SocketAddr,
    /// The routers PCP and NAT-PMP are asked at; `None` finds the default
    /// gateway.
    pub pmp: Option<Vec<SocketAddr>>,
    pub wait: Duration,
}

impl Gateways {
    fn real() -> Gateways {
        Gateways {
            ssdp: upnp::SSDP,
            pmp: None,
            wait: SEARCH_WAIT,
        }
    }
}

/// Maps a host's port while it lives; see the module docs.
pub struct PortMapper {
    events: Arc<Mutex<Vec<MapEvent>>>,
    /// The router has the mappings (so dropping waits for their removal).
    mapped: Arc<AtomicBool>,
    stop: Option<Sender<()>>,
    removed: Receiver<()>,
}

impl PortMapper {
    /// Maps UDP and TCP `port` on whichever router answers.
    pub fn start(port: u16) -> PortMapper {
        PortMapper::start_with(port, Gateways::real())
    }

    pub(crate) fn start_with(port: u16, gateways: Gateways) -> PortMapper {
        let events = Arc::new(Mutex::new(Vec::new()));
        let (stop, stopped) = channel();
        let (removed_send, removed) = channel();
        let reports = events.clone();
        let mapped = Arc::new(AtomicBool::new(false));
        let has_mapped = mapped.clone();
        let started = std::thread::Builder::new()
            .name("port mapping".to_owned())
            .spawn(move || {
                run(port, &gateways, &reports, &has_mapped, &stopped);
                let _ = removed_send.send(());
            });
        if let Err(err) = started {
            push(
                &events,
                MapEvent::Failed(format!("can't start a thread: {err}")),
            );
        }
        PortMapper {
            events,
            mapped,
            stop: Some(stop),
            removed,
        }
    }

    /// What happened since the last call.
    pub fn take_events(&self) -> Vec<MapEvent> {
        self.events
            .lock()
            .map(|mut events| std::mem::take(&mut *events))
            .unwrap_or_default()
    }
}

impl Drop for PortMapper {
    /// Removes the mappings, waiting a moment for the router (one that
    /// doesn't answer, or a host that exits first, leaves them until their
    /// lease ends). A mapping still being asked for is removed once it's
    /// made, without waiting.
    fn drop(&mut self) {
        drop(self.stop.take());
        if self.mapped.load(Ordering::Acquire) {
            let _ = self.removed.recv_timeout(REMOVE_WAIT);
        }
    }
}

fn push(events: &Mutex<Vec<MapEvent>>, event: MapEvent) {
    if let Ok(mut events) = events.lock() {
        events.push(event);
    }
}

/// A mapping of the game's UDP and TCP port on one router.
enum Mapping {
    Upnp(upnp::Mapped),
    Pmp(pmp::Mapped),
}

impl Mapping {
    fn event(&self) -> MapEvent {
        let (external, method, permanent) = match self {
            Mapping::Upnp(mapped) => (mapped.external(), MapMethod::Upnp, mapped.permanent()),
            Mapping::Pmp(mapped) => (mapped.external(), mapped.method(), false),
        };
        MapEvent::Mapped {
            external,
            method,
            permanent,
        }
    }

    /// When to renew: at half the lease the router gave.
    fn renew_in(&self) -> Duration {
        let lease = match self {
            Mapping::Upnp(mapped) => mapped.lease(),
            Mapping::Pmp(mapped) => mapped.lease(),
        };
        match lease {
            0 => Duration::from_secs(24 * 60 * 60),
            seconds => Duration::from_secs(u64::from(seconds.max(120)) / 2),
        }
    }

    fn renew(&mut self) -> Result<(), String> {
        match self {
            Mapping::Upnp(mapped) => mapped.renew(),
            Mapping::Pmp(mapped) => mapped.renew(),
        }
    }

    fn remove(&self) {
        match self {
            Mapping::Upnp(mapped) => mapped.remove(),
            Mapping::Pmp(mapped) => mapped.remove(),
        }
    }
}

fn run(
    port: u16,
    gateways: &Gateways,
    events: &Mutex<Vec<MapEvent>>,
    mapped: &AtomicBool,
    stop: &Receiver<()>,
) {
    let mut mapping = match map(port, gateways) {
        Ok(mapping) => mapping,
        Err(why) => {
            push(events, MapEvent::Failed(why));
            return;
        }
    };
    mapped.store(true, Ordering::Release);
    push(events, mapping.event());
    let mut wait = mapping.renew_in();
    let mut lost = false;
    // Until stopped (the sender dropped), renew whenever the wait runs out.
    while let Err(RecvTimeoutError::Timeout) = stop.recv_timeout(wait) {
        match mapping.renew() {
            Ok(()) => {
                if lost {
                    push(events, mapping.event());
                    lost = false;
                }
                wait = mapping.renew_in();
            }
            Err(why) => {
                if !lost {
                    push(events, MapEvent::Lost(why));
                    lost = true;
                }
                wait = RETRY_AFTER;
            }
        }
    }
    mapping.remove();
}

/// Maps `port` on the first router that will: UPnP, then PCP or NAT-PMP.
fn map(port: u16, gateways: &Gateways) -> Result<Mapping, String> {
    let mut tried = Vec::new();
    match upnp::map(port, gateways.ssdp, gateways.wait) {
        Ok(mapped) => return Ok(Mapping::Upnp(mapped)),
        Err(why) => tried.push(format!("UPnP: {why}")),
    }
    let routers = gateways
        .pmp
        .clone()
        .unwrap_or_else(|| default_gateways().into_iter().map(pmp::at).collect());
    if routers.is_empty() {
        tried.push("PCP and NAT-PMP: no default gateway found".to_owned());
    }
    for router in routers {
        match pmp::map(port, router) {
            Ok(mapped) => return Ok(Mapping::Pmp(mapped)),
            Err(why) => tried.push(format!("PCP and NAT-PMP at {}: {why}", router.ip())),
        }
    }
    Err(tried.join("; "))
}

/// The address this computer uses towards `to` (sends nothing).
fn local_address(to: SocketAddr) -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect(to).ok()?;
    match socket.local_addr().ok()?.ip() {
        IpAddr::V4(ip) if !ip.is_unspecified() => Some(ip),
        _ => None,
    }
}

/// Whether players on the internet can reach `ip`: not when it's a private
/// address, or a carrier's shared one (100.64.0.0/10), which means another
/// router in front of this one.
pub fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, ..] = ip.octets();
            !(ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_unspecified()
                || (a == 100 && (64..128).contains(&b)))
        }
        IpAddr::V6(ip) => !(ip.is_loopback() || ip.is_unspecified()),
    }
}

/// The router this computer sends internet traffic through: the system's
/// default gateway where it can be read, else a guess from this computer's
/// own address (most home networks put the router at .1).
fn default_gateways() -> Vec<Ipv4Addr> {
    if let Some(gateway) = system_gateway() {
        return vec![gateway];
    }
    let probe = SocketAddrV4::new(Ipv4Addr::new(198, 51, 100, 1), 9).into();
    match local_address(probe) {
        Some(local) if local.is_private() => {
            let [a, b, c, _] = local.octets();
            vec![Ipv4Addr::new(a, b, c, 1)]
        }
        _ => Vec::new(),
    }
}

#[cfg(target_os = "linux")]
fn system_gateway() -> Option<Ipv4Addr> {
    let routes = std::fs::read_to_string("/proc/net/route").ok()?;
    linux_gateway(&routes)
}

/// The default route's gateway in `/proc/net/route` (addresses are hex,
/// in the computer's byte order: little-endian on every desktop).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn linux_gateway(routes: &str) -> Option<Ipv4Addr> {
    routes.lines().skip(1).find_map(|line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let (destination, gateway) = (*fields.get(1)?, *fields.get(2)?);
        if destination != "00000000" {
            return None;
        }
        let gateway = u32::from_str_radix(gateway, 16).ok()?;
        (gateway != 0).then(|| Ipv4Addr::from(gateway.to_le_bytes()))
    })
}

#[cfg(target_os = "macos")]
fn system_gateway() -> Option<Ipv4Addr> {
    let output = std::process::Command::new("/sbin/route")
        .args(["-n", "get", "default"])
        .output()
        .ok()?;
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| line.trim().strip_prefix("gateway:")?.trim().parse().ok())
}

#[cfg(windows)]
fn system_gateway() -> Option<Ipv4Addr> {
    use std::os::windows::process::CommandExt;
    // No console window flashing up behind the player's.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let output = std::process::Command::new("route")
        .args(["print", "-4", "0.0.0.0"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    windows_gateway(&String::from_utf8_lossy(&output.stdout))
}

/// The default route's gateway in `route print`'s table: the line
/// `0.0.0.0  0.0.0.0  <gateway>  <interface>  <metric>`.
#[cfg_attr(not(windows), allow(dead_code))]
fn windows_gateway(table: &str) -> Option<Ipv4Addr> {
    table.lines().find_map(|line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        match fields.as_slice() {
            ["0.0.0.0", "0.0.0.0", gateway, ..] => gateway.parse().ok(),
            _ => None,
        }
    })
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn system_gateway() -> Option<Ipv4Addr> {
    None
}
