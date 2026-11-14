//! UPnP IGD port mapping: find the router with an SSDP search, read its
//! description for the WAN connection service, and call that service's
//! `AddPortMapping` over SOAP. Plain HTTP on the home network, written
//! here (a few requests, never through a proxy).

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpStream, ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant};

use super::{DESCRIPTION, LEASE_SECONDS, local_address};

/// The SSDP multicast address UPnP searches go to.
pub(super) const SSDP: SocketAddr =
    SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(239, 255, 255, 250), 1900));
/// The device types searched for (IGD 2 and 1).
const DEVICES: [&str; 2] = [
    "urn:schemas-upnp-org:device:InternetGatewayDevice:2",
    "urn:schemas-upnp-org:device:InternetGatewayDevice:1",
];
/// The services that map ports, best first.
const SERVICES: [&str; 3] = [
    "urn:schemas-upnp-org:service:WANIPConnection:2",
    "urn:schemas-upnp-org:service:WANIPConnection:1",
    "urn:schemas-upnp-org:service:WANPPPConnection:1",
];
/// How long a router has to answer an HTTP request.
const HTTP_WAIT: Duration = Duration::from_secs(3);
/// The largest answer read from a router.
const MOST_BYTES: usize = 256 * 1024;

/// UPnP errors that change what's asked next.
const ONLY_PERMANENT_LEASES: u16 = 725;
const CONFLICT: u16 = 718;
/// How many outside ports are tried, from the game's own.
const OUTSIDE_PORTS: u16 = 8;

/// The game's port, mapped on a router over UPnP.
pub(super) struct Mapped {
    control: Url,
    service: &'static str,
    /// The game's port on this computer.
    port: u16,
    /// The router's port forwarded to it: the same, unless another
    /// computer has that one.
    outside_port: u16,
    local: Ipv4Addr,
    external: Ipv4Addr,
    /// Seconds; 0 is permanent (the router takes nothing else).
    lease: u32,
}

impl Mapped {
    pub fn external(&self) -> SocketAddr {
        SocketAddrV4::new(self.external, self.outside_port).into()
    }

    pub fn permanent(&self) -> bool {
        self.lease == 0
    }

    pub fn lease(&self) -> u32 {
        self.lease
    }

    /// Asks again, which extends the lease.
    pub fn renew(&mut self) -> Result<(), String> {
        for protocol in ["UDP", "TCP"] {
            self.add(protocol, self.lease)
                .map_err(|err| err.to_string())?;
        }
        if let Ok(external) = external_address(&self.control, self.service) {
            self.external = external;
        }
        Ok(())
    }

    pub fn remove(&self) {
        for protocol in ["UDP", "TCP"] {
            self.delete(protocol);
        }
    }

    /// Removes one of this game's mappings (only ever its own: another
    /// computer's on the same port isn't touched).
    fn delete(&self, protocol: &str) {
        let _ = soap(
            &self.control,
            self.service,
            "DeletePortMapping",
            &[
                ("NewRemoteHost", String::new()),
                ("NewExternalPort", self.outside_port.to_string()),
                ("NewProtocol", protocol.to_owned()),
            ],
        );
    }

    /// Maps UDP and TCP at `outside_port`; on failure, nothing of this
    /// game's stays mapped there.
    fn add_both(&mut self) -> Result<(), SoapError> {
        for (index, protocol) in ["UDP", "TCP"].into_iter().enumerate() {
            let added = match self.add(protocol, self.lease) {
                Err(SoapError::Fault(ONLY_PERMANENT_LEASES, _)) if self.lease != 0 => {
                    self.lease = 0;
                    self.add(protocol, 0)
                }
                added => added,
            };
            if let Err(err) = added {
                if index == 1 {
                    self.delete("UDP");
                }
                return Err(err);
            }
        }
        Ok(())
    }

    fn add(&self, protocol: &str, lease: u32) -> Result<(), SoapError> {
        soap(
            &self.control,
            self.service,
            "AddPortMapping",
            &[
                ("NewRemoteHost", String::new()),
                ("NewExternalPort", self.outside_port.to_string()),
                ("NewProtocol", protocol.to_owned()),
                ("NewInternalPort", self.port.to_string()),
                ("NewInternalClient", self.local.to_string()),
                ("NewEnabled", "1".to_owned()),
                ("NewPortMappingDescription", DESCRIPTION.to_owned()),
                ("NewLeaseDuration", lease.to_string()),
            ],
        )
        .map(|_| ())
    }
}

/// Maps UDP and TCP `port` (to the same port outside) on the first
/// router that answers a search sent to `ssdp` within `wait`.
pub(super) fn map(port: u16, ssdp: SocketAddr, wait: Duration) -> Result<Mapped, String> {
    let locations = search(ssdp, wait)?;
    let mut tried = Vec::new();
    for location in locations {
        match map_on(port, &location) {
            Ok(mapped) => return Ok(mapped),
            Err(why) => tried.push(why),
        }
    }
    Err(tried.join("; "))
}

fn map_on(port: u16, location: &str) -> Result<Mapped, String> {
    let description_url = Url::parse(location)?;
    let (status, description) = http(&description_url, "GET", &[], "")?;
    if status != 200 {
        return Err(format!("the router's description answered HTTP {status}"));
    }
    let (service, control) = connection_service(&description, &description_url)?;
    let local = local_address(control.address)
        .ok_or_else(|| format!("no address of this computer reaches {}", control.address))?;
    let external = external_address(&control, service).map_err(|err| err.to_string())?;
    let mut mapped = Mapped {
        control,
        service,
        port,
        outside_port: port,
        local,
        external,
        lease: LEASE_SECONDS,
    };
    // Another computer here may have the game's port on the router (two
    // hosts in one house): the next few are tried, and the join link says
    // which one players use.
    for outside_port in (0..OUTSIDE_PORTS).filter_map(|step| port.checked_add(step)) {
        mapped.outside_port = outside_port;
        match mapped.add_both() {
            Ok(()) => return Ok(mapped),
            Err(SoapError::Fault(CONFLICT, _)) => continue,
            Err(err) => return Err(format!("the router refused port {outside_port}: {err}")),
        }
    }
    Err(format!(
        "the router already forwards ports {port} to {} to other computers",
        port.saturating_add(OUTSIDE_PORTS - 1)
    ))
}

fn external_address(control: &Url, service: &'static str) -> Result<Ipv4Addr, SoapError> {
    let answer = soap(control, service, "GetExternalIPAddress", &[])?;
    let text = elements(&answer, "NewExternalIPAddress")
        .into_iter()
        .next()
        .unwrap_or_default();
    text.trim()
        .parse()
        .map_err(|_| SoapError::Other(format!("the router's outside address is {text:?}")))
}

/// Searches for internet gateways: their description URLs, in the order
/// they answered.
fn search(ssdp: SocketAddr, wait: Duration) -> Result<Vec<String>, String> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))
        .map_err(|err| format!("can't search: {err}"))?;
    // Past the computer's own network, never further.
    let _ = socket.set_multicast_ttl_v4(2);
    for device in DEVICES {
        let search = format!(
            "M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nST: {device}\r\n\
             MAN: \"ssdp:discover\"\r\nMX: 1\r\n\r\n"
        );
        socket
            .send_to(search.as_bytes(), ssdp)
            .map_err(|err| format!("can't search: {err}"))?;
    }
    let deadline = Instant::now() + wait;
    let mut locations = Vec::new();
    let mut buffer = [0u8; 2048];
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        let _ = socket.set_read_timeout(Some(left));
        let Ok((len, _)) = socket.recv_from(&mut buffer) else {
            break;
        };
        let answer = String::from_utf8_lossy(&buffer[..len]);
        let gateway = header(&answer, "ST")
            .is_some_and(|st| DEVICES.iter().any(|device| st.eq_ignore_ascii_case(device)));
        if let (true, Some(location)) = (gateway, header(&answer, "LOCATION"))
            && !locations.iter().any(|known| known == location)
        {
            locations.push(location.to_owned());
            // One router answering both searches answers twice; the first
            // is enough to try, others still come while it's tried.
            break;
        }
    }
    if locations.is_empty() {
        return Err("no router answered a search (UPnP off, or not a home router)".to_owned());
    }
    Ok(locations)
}

/// Header `name`'s value in an HTTP-shaped message.
pub(super) fn header<'a>(message: &'a str, name: &str) -> Option<&'a str> {
    message.lines().skip(1).find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.trim().eq_ignore_ascii_case(name).then(|| value.trim())
    })
}

/// The best WAN connection service in a device description, and its
/// control URL.
fn connection_service(description: &str, base: &Url) -> Result<(&'static str, Url), String> {
    let base = match elements(description, "URLBase").first() {
        Some(url_base) if !url_base.trim().is_empty() => Url::parse(url_base.trim())?,
        _ => base.clone(),
    };
    let services = elements(description, "service");
    for wanted in SERVICES {
        for service in &services {
            let kind = elements(service, "serviceType");
            if kind.first().map(|kind| kind.trim()) != Some(wanted) {
                continue;
            }
            let Some(control) = elements(service, "controlURL").first().map(|c| c.trim()) else {
                continue;
            };
            return Ok((wanted, base.join(control)?));
        }
    }
    Err("the router has no service that forwards ports".to_owned())
}

/// The contents of every element called `name` (any namespace prefix) in
/// `xml`. Enough for routers' descriptions and SOAP answers, which are
/// plain; not a general XML reader.
pub(super) fn elements<'a>(xml: &'a str, name: &str) -> Vec<&'a str> {
    let mut found = Vec::new();
    let mut rest = xml;
    while let Some(open) = rest.find('<') {
        let after = &rest[open + 1..];
        let end = after.find('>').unwrap_or(after.len());
        let tag = &after[..end];
        let tag_name = tag
            .split(|c: char| c.is_whitespace() || c == '/')
            .next()
            .unwrap_or("");
        let local = tag_name.rsplit(':').next().unwrap_or("");
        if local == name && !tag.ends_with('/') {
            let body = &after[(end + 1).min(after.len())..];
            match closing(body, name) {
                Some((inner, after_close)) => {
                    found.push(inner);
                    rest = after_close;
                    continue;
                }
                None => break,
            }
        }
        rest = after;
    }
    found
}

/// `body` up to the `</name>` (any prefix) that closes it, and what's
/// after that. Elements of the same name aren't nested in what's read.
fn closing<'a>(body: &'a str, name: &str) -> Option<(&'a str, &'a str)> {
    let mut from = 0;
    while let Some(at) = body[from..].find("</") {
        let start = from + at;
        let after = &body[start + 2..];
        let end = after.find('>')?;
        let tag = after[..end].trim();
        if tag.rsplit(':').next() == Some(name) {
            return Some((&body[..start], &after[end + 1..]));
        }
        from = start + 2;
    }
    None
}

/// Why a SOAP call failed.
#[derive(Debug)]
enum SoapError {
    /// The router's UPnP error code and its description.
    Fault(u16, String),
    Other(String),
}

impl std::fmt::Display for SoapError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            SoapError::Fault(code, description) => write!(f, "UPnP error {code} {description}"),
            SoapError::Other(why) => f.write_str(why),
        }
    }
}

/// Calls `action` on `service` at `control`; the answer's body.
fn soap(
    control: &Url,
    service: &str,
    action: &str,
    arguments: &[(&str, String)],
) -> Result<String, SoapError> {
    let arguments: String = arguments
        .iter()
        .map(|(name, value)| format!("<{name}>{value}</{name}>"))
        .collect();
    let body = format!(
        "<?xml version=\"1.0\"?>\r\n<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" \
         s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\"><s:Body>\
         <u:{action} xmlns:u=\"{service}\">{arguments}</u:{action}></s:Body></s:Envelope>\r\n"
    );
    let action_header = format!("\"{service}#{action}\"");
    let (status, answer) = http(
        control,
        "POST",
        &[
            ("Content-Type", "text/xml; charset=\"utf-8\""),
            ("SOAPAction", &action_header),
        ],
        &body,
    )
    .map_err(SoapError::Other)?;
    if status == 200 {
        return Ok(answer);
    }
    let code = elements(&answer, "errorCode")
        .first()
        .and_then(|code| code.trim().parse().ok());
    let description = elements(&answer, "errorDescription")
        .first()
        .map(|text| format!("({})", text.trim()))
        .unwrap_or_default();
    Err(match code {
        Some(code) => SoapError::Fault(code, description),
        None => SoapError::Other(format!("{action} answered HTTP {status}")),
    })
}

/// An `http://` URL on the home network.
#[derive(Clone, Debug)]
pub(super) struct Url {
    /// As the `Host` header says it.
    host: String,
    address: SocketAddr,
    path: String,
}

impl Url {
    fn parse(url: &str) -> Result<Url, String> {
        let rest = url
            .strip_prefix("http://")
            .ok_or_else(|| format!("the router gave a URL that isn't http:// ({url})"))?;
        let (host, path) = match rest.find('/') {
            Some(at) => (&rest[..at], &rest[at..]),
            None => (rest, "/"),
        };
        let with_port = if host
            .rsplit_once(':')
            .is_some_and(|(_, port)| port.parse::<u16>().is_ok())
        {
            host.to_owned()
        } else {
            format!("{host}:80")
        };
        let address = with_port
            .to_socket_addrs()
            .ok()
            .and_then(|mut addresses| addresses.next())
            .ok_or_else(|| format!("can't find the router at {host}"))?;
        Ok(Url {
            host: host.to_owned(),
            address,
            path: path.to_owned(),
        })
    }

    /// `reference` (absolute, or a path) from this URL.
    fn join(&self, reference: &str) -> Result<Url, String> {
        if reference.starts_with("http://") {
            return Url::parse(reference);
        }
        let path = if reference.starts_with('/') {
            reference.to_owned()
        } else {
            let directory = &self.path[..self.path.rfind('/').map_or(0, |at| at + 1)];
            format!("{directory}{reference}")
        };
        Ok(Url {
            path: if path.starts_with('/') {
                path
            } else {
                format!("/{path}")
            },
            ..self.clone()
        })
    }
}

/// One HTTP/1.1 request; the status and body of the answer.
fn http(
    url: &Url,
    method: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> Result<(u16, String), String> {
    let mut stream = TcpStream::connect_timeout(&url.address, HTTP_WAIT)
        .map_err(|err| format!("can't reach the router at {}: {err}", url.address))?;
    let _ = stream.set_read_timeout(Some(HTTP_WAIT));
    let _ = stream.set_write_timeout(Some(HTTP_WAIT));
    let mut request = format!(
        "{method} {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nContent-Length: {}\r\n",
        url.path,
        url.host,
        body.len()
    );
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("\r\n");
    request.push_str(body);
    stream
        .write_all(request.as_bytes())
        .map_err(|err| format!("the router closed the connection: {err}"))?;
    let answer = read_answer(&mut stream)?;
    parse_answer(&answer)
}

/// Reads an answer until it's complete: its length is reached, its last
/// chunk came, or the router closes the connection.
fn read_answer(stream: &mut TcpStream) -> Result<Vec<u8>, String> {
    let mut answer = Vec::new();
    let mut buffer = [0u8; 4096];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(len) => answer.extend_from_slice(&buffer[..len]),
            Err(_) if !answer.is_empty() => break,
            Err(err) => return Err(format!("the router didn't answer: {err}")),
        }
        if answer.len() > MOST_BYTES {
            return Err("the router's answer is too long".to_owned());
        }
        if complete(&answer) {
            break;
        }
    }
    Ok(answer)
}

fn complete(answer: &[u8]) -> bool {
    let Some(head_end) = find(answer, b"\r\n\r\n") else {
        return false;
    };
    let head = String::from_utf8_lossy(&answer[..head_end]);
    let body = &answer[head_end + 4..];
    if let Some(length) = header(&head, "Content-Length").and_then(|n| n.parse::<usize>().ok()) {
        return body.len() >= length;
    }
    header(&head, "Transfer-Encoding").is_some_and(|te| te.eq_ignore_ascii_case("chunked"))
        && body.ends_with(b"0\r\n\r\n")
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// The status and body of an HTTP answer (chunked or not).
pub(super) fn parse_answer(answer: &[u8]) -> Result<(u16, String), String> {
    let head_end = find(answer, b"\r\n\r\n").ok_or("the router's answer isn't HTTP")?;
    let head = String::from_utf8_lossy(&answer[..head_end]);
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .ok_or("the router's answer isn't HTTP")?;
    let mut body = &answer[head_end + 4..];
    let chunked =
        header(&head, "Transfer-Encoding").is_some_and(|te| te.eq_ignore_ascii_case("chunked"));
    let body = if chunked {
        let mut joined = Vec::new();
        loop {
            let line_end = find(body, b"\r\n").ok_or("a chunked answer ended early")?;
            let size_text = String::from_utf8_lossy(&body[..line_end]);
            let size = usize::from_str_radix(size_text.split(';').next().unwrap_or("").trim(), 16)
                .map_err(|_| "a chunked answer with a bad size")?;
            body = &body[line_end + 2..];
            if size == 0 {
                break;
            }
            let chunk = body.get(..size).ok_or("a chunked answer ended early")?;
            joined.extend_from_slice(chunk);
            body = body.get(size + 2..).unwrap_or_default();
        }
        joined
    } else {
        match header(&head, "Content-Length").and_then(|n| n.parse::<usize>().ok()) {
            Some(length) => body[..length.min(body.len())].to_vec(),
            None => body.to_vec(),
        }
    };
    Ok((status, String::from_utf8_lossy(&body).into_owned()))
}
