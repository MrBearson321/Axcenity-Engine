//! Port mapping against fake routers on this computer: a UPnP one (SSDP
//! answer, description and SOAP control over HTTP) and a PCP or NAT-PMP
//! one. `scripts/nat-lab.sh mapped` checks real ones (miniupnpd).

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::*;

pub(crate) const OUTSIDE: Ipv4Addr = Ipv4Addr::new(203, 0, 113, 7);

fn wait_for_event(mapper: &PortMapper) -> MapEvent {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if let Some(event) = mapper.take_events().into_iter().next() {
            return event;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("no port mapping event");
}

/// A UDP port nothing answers on.
pub(crate) fn silent() -> SocketAddr {
    let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    socket.local_addr().unwrap()
}

/// A fake UPnP router: what it was asked, as `(action, arguments)`.
struct FakeIgd {
    ssdp: SocketAddr,
    calls: Arc<Mutex<Vec<(String, String)>>>,
}

impl FakeIgd {
    /// `permanent_only`: refuses leases (UPnP error 725), as some do.
    /// `taken`: an outside port another computer has (error 718).
    fn start(permanent_only: bool, taken: Option<u16>) -> FakeIgd {
        let http = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let http_port = http.local_addr().unwrap().port();
        let ssdp = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let ssdp_address = ssdp.local_addr().unwrap();
        std::thread::spawn(move || {
            let mut buffer = [0u8; 2048];
            while let Ok((len, from)) = ssdp.recv_from(&mut buffer) {
                let search = String::from_utf8_lossy(&buffer[..len]);
                assert!(search.starts_with("M-SEARCH * HTTP/1.1"));
                let st = upnp::header(&search, "ST").unwrap_or_default();
                let answer = format!(
                    "HTTP/1.1 200 OK\r\nCACHE-CONTROL: max-age=120\r\nST: {st}\r\n\
                     LOCATION: http://127.0.0.1:{http_port}/rootDesc.xml\r\n\r\n"
                );
                let _ = ssdp.send_to(answer.as_bytes(), from);
            }
        });
        let calls = Arc::new(Mutex::new(Vec::new()));
        let recorded = calls.clone();
        std::thread::spawn(move || {
            for stream in http.incoming() {
                let Ok(mut stream) = stream else { break };
                let request = read_request(&mut stream);
                let answer = igd_answer(&request, permanent_only, taken, &recorded);
                let _ = stream.write_all(answer.as_bytes());
            }
        });
        FakeIgd {
            ssdp: ssdp_address,
            calls,
        }
    }

    fn calls(&self) -> Vec<(String, String)> {
        self.calls.lock().unwrap().clone()
    }
}

fn read_request(stream: &mut std::net::TcpStream) -> String {
    let mut request = Vec::new();
    let mut buffer = [0u8; 4096];
    loop {
        let len = stream.read(&mut buffer).unwrap_or(0);
        if len == 0 {
            break;
        }
        request.extend_from_slice(&buffer[..len]);
        let text = String::from_utf8_lossy(&request).into_owned();
        if let Some(head_end) = text.find("\r\n\r\n") {
            let length = upnp::header(&text[..head_end], "Content-Length")
                .and_then(|n| n.parse::<usize>().ok())
                .unwrap_or(0);
            if request.len() >= head_end + 4 + length {
                break;
            }
        }
    }
    String::from_utf8_lossy(&request).into_owned()
}

const DESCRIPTION_XML: &str = r#"<?xml version="1.0"?>
<root xmlns="urn:schemas-upnp-org:device-1-0"><device>
<deviceType>urn:schemas-upnp-org:device:InternetGatewayDevice:1</deviceType>
<serviceList><service><serviceType>urn:schemas-upnp-org:service:Layer3Forwarding:1</serviceType>
<controlURL>/ctl/L3F</controlURL></service></serviceList>
<deviceList><device><deviceType>urn:schemas-upnp-org:device:WANDevice:1</deviceType>
<deviceList><device><deviceType>urn:schemas-upnp-org:device:WANConnectionDevice:1</deviceType>
<serviceList><service>
<serviceType>urn:schemas-upnp-org:service:WANIPConnection:1</serviceType>
<serviceId>urn:upnp-org:serviceId:WANIPConn1</serviceId>
<controlURL>ctl/IPConn</controlURL>
</service></serviceList></device></deviceList></device></deviceList>
</device></root>"#;

fn igd_answer(
    request: &str,
    permanent_only: bool,
    taken: Option<u16>,
    calls: &Mutex<Vec<(String, String)>>,
) -> String {
    if request.starts_with("GET /rootDesc.xml") {
        // Chunked, as some routers answer.
        let (first, second) = DESCRIPTION_XML.split_at(100);
        return format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/xml\r\nTransfer-Encoding: chunked\r\n\r\n\
             {:x}\r\n{first}\r\n{:x}\r\n{second}\r\n0\r\n\r\n",
            first.len(),
            second.len()
        );
    }
    assert!(
        request.starts_with("POST /ctl/IPConn HTTP/1.1"),
        "{request}"
    );
    let action = upnp::header(request, "SOAPAction")
        .and_then(|action| {
            action
                .trim_matches('"')
                .split('#')
                .nth(1)
                .map(str::to_owned)
        })
        .unwrap_or_default();
    let body = request.split("\r\n\r\n").nth(1).unwrap_or("").to_owned();
    calls.lock().unwrap().push((action.clone(), body.clone()));
    let fault = |code: u16| {
        let xml = format!(
            "<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body><s:Fault>\
             <detail><UPnPError xmlns=\"urn:schemas-upnp-org:control-1-0\"><errorCode>{code}</errorCode>\
             <errorDescription>OnlyPermanentLeasesSupported</errorDescription></UPnPError></detail>\
             </s:Fault></s:Body></s:Envelope>"
        );
        format!(
            "HTTP/1.1 500 Internal Server Error\r\nContent-Length: {}\r\n\r\n{xml}",
            xml.len()
        )
    };
    let ok = |inner: &str| {
        let xml = format!(
            "<?xml version=\"1.0\"?><s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\">\
             <s:Body><u:{action}Response xmlns:u=\"urn:schemas-upnp-org:service:WANIPConnection:1\">\
             {inner}</u:{action}Response></s:Body></s:Envelope>"
        );
        format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{xml}",
            xml.len()
        )
    };
    match action.as_str() {
        "GetExternalIPAddress" => ok(&format!(
            "<NewExternalIPAddress>{OUTSIDE}</NewExternalIPAddress>"
        )),
        "AddPortMapping" if permanent_only && !body.contains("<NewLeaseDuration>0<") => fault(725),
        "AddPortMapping"
            if taken.is_some_and(|port| {
                body.contains(&format!("<NewExternalPort>{port}</NewExternalPort>"))
            }) =>
        {
            fault(718)
        }
        _ => ok(""),
    }
}

#[test]
fn upnp_maps_the_game_and_token_ports_and_removes_them() {
    let igd = FakeIgd::start(false, None);
    let gateways = Gateways {
        ssdp: igd.ssdp,
        pmp: Some(Vec::new()),
        wait: Duration::from_secs(2),
    };
    let mapper = PortMapper::start_with(47_800, gateways);
    assert_eq!(
        wait_for_event(&mapper),
        MapEvent::Mapped {
            external: SocketAddr::from((OUTSIDE, 47_800)),
            method: MapMethod::Upnp,
            permanent: false,
        }
    );
    let adds: Vec<String> = igd
        .calls()
        .into_iter()
        .filter(|(action, _)| action == "AddPortMapping")
        .map(|(_, body)| body)
        .collect();
    assert_eq!(adds.len(), 2);
    for (body, protocol) in adds.iter().zip(["UDP", "TCP"]) {
        assert!(body.contains(&format!("<NewProtocol>{protocol}</NewProtocol>")));
        assert!(body.contains("<NewExternalPort>47800</NewExternalPort>"));
        assert!(body.contains("<NewInternalPort>47800</NewInternalPort>"));
        assert!(body.contains("<NewInternalClient>127.0.0.1</NewInternalClient>"));
        assert!(body.contains(&format!(
            "<NewLeaseDuration>{LEASE_SECONDS}</NewLeaseDuration>"
        )));
    }
    drop(mapper);
    let deletes = igd
        .calls()
        .into_iter()
        .filter(|(action, _)| action == "DeletePortMapping")
        .count();
    assert_eq!(deletes, 2, "dropping the mapper removes both mappings");
}

#[test]
fn upnp_takes_a_permanent_mapping_from_a_router_that_only_has_those() {
    let igd = FakeIgd::start(true, None);
    let gateways = Gateways {
        ssdp: igd.ssdp,
        pmp: Some(Vec::new()),
        wait: Duration::from_secs(2),
    };
    let mapper = PortMapper::start_with(47_801, gateways);
    match wait_for_event(&mapper) {
        MapEvent::Mapped { permanent, .. } => assert!(permanent),
        other => panic!("{other:?}"),
    }
}

#[test]
fn upnp_takes_the_next_outside_port_when_another_computer_has_the_game_s() {
    let igd = FakeIgd::start(false, Some(47_805));
    let gateways = Gateways {
        ssdp: igd.ssdp,
        pmp: Some(Vec::new()),
        wait: Duration::from_secs(2),
    };
    let mapper = PortMapper::start_with(47_805, gateways);
    match wait_for_event(&mapper) {
        MapEvent::Mapped { external, .. } => {
            assert_eq!(external, SocketAddr::from((OUTSIDE, 47_806)));
        }
        other => panic!("{other:?}"),
    }
    let calls = igd.calls();
    assert!(
        calls
            .iter()
            .all(|(action, body)| action != "DeletePortMapping"
                || !body.contains("<NewExternalPort>47805<")),
        "the other computer's mapping is left alone: {calls:?}"
    );
    assert!(calls.iter().any(|(action, body)| action == "AddPortMapping"
        && body.contains("<NewExternalPort>47806<")
        && body.contains("<NewInternalPort>47805<")));
}

/// A fake PCP router (or NAT-PMP only): what it was asked, as
/// `(version, opcode, lifetime)`.
pub(crate) struct FakePmp {
    pub address: SocketAddr,
    asked: Arc<Mutex<Vec<(u8, u8, u32)>>>,
}

impl FakePmp {
    pub fn start(nat_pmp_only: bool) -> FakePmp {
        let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = socket.local_addr().unwrap();
        let asked = Arc::new(Mutex::new(Vec::new()));
        let recorded = asked.clone();
        std::thread::spawn(move || {
            let mut buffer = [0u8; 1100];
            while let Ok((len, from)) = socket.recv_from(&mut buffer) {
                let request = &buffer[..len];
                let answer = pmp_answer(request, nat_pmp_only, &recorded);
                let _ = socket.send_to(&answer, from);
            }
        });
        FakePmp { address, asked }
    }

    pub fn asked(&self) -> Vec<(u8, u8, u32)> {
        self.asked.lock().unwrap().clone()
    }
}

fn pmp_answer(request: &[u8], nat_pmp_only: bool, asked: &Mutex<Vec<(u8, u8, u32)>>) -> Vec<u8> {
    let (version, opcode) = (request[0], request[1]);
    let epoch = 1234u32.to_be_bytes();
    match (version, nat_pmp_only) {
        (2, false) => {
            let lifetime = u32::from_be_bytes(request[4..8].try_into().unwrap());
            asked.lock().unwrap().push((2, opcode, lifetime));
            let mut answer = vec![2, 0x80 | opcode, 0, 0];
            answer.extend(lifetime.min(600).to_be_bytes());
            answer.extend(epoch);
            answer.extend([0; 12]);
            // The MAP payload as asked, with the outside address filled in.
            answer.extend(&request[24..42]);
            answer.extend(&request[42..44]);
            answer.extend(OUTSIDE.to_ipv6_mapped().octets());
            answer
        }
        (2, true) => {
            asked.lock().unwrap().push((2, opcode, 0));
            // NAT-PMP's "unsupported version".
            let mut answer = vec![0, 0x80 | opcode, 0, 1];
            answer.extend(epoch);
            answer
        }
        (_, _) if opcode == 0 => {
            asked.lock().unwrap().push((0, 0, 0));
            let mut answer = vec![0, 0x80, 0, 0];
            answer.extend(epoch);
            answer.extend(OUTSIDE.octets());
            answer
        }
        (_, _) => {
            let lifetime = u32::from_be_bytes(request[8..12].try_into().unwrap());
            asked.lock().unwrap().push((0, opcode, lifetime));
            let mut answer = vec![0, 0x80 | opcode, 0, 0];
            answer.extend(epoch);
            answer.extend(&request[4..6]);
            // A router that removes gives back port 0.
            let external = if lifetime == 0 {
                [0, 0]
            } else {
                [request[6], request[7]]
            };
            answer.extend(external);
            answer.extend(lifetime.min(600).to_be_bytes());
            answer
        }
    }
}

#[test]
fn pcp_maps_both_ports_when_upnp_is_off() {
    let router = FakePmp::start(false);
    let gateways = Gateways {
        ssdp: silent(),
        pmp: Some(vec![router.address]),
        wait: Duration::from_millis(200),
    };
    let mapper = PortMapper::start_with(47_802, gateways);
    assert_eq!(
        wait_for_event(&mapper),
        MapEvent::Mapped {
            external: SocketAddr::from((OUTSIDE, 47_802)),
            method: MapMethod::Pcp,
            permanent: false,
        }
    );
    drop(mapper);
    assert_eq!(
        router.asked(),
        vec![
            (2, 1, LEASE_SECONDS),
            (2, 1, LEASE_SECONDS),
            (2, 1, 0),
            (2, 1, 0)
        ],
        "a MAP each for UDP and TCP, then each again with no lifetime to remove it"
    );
}

#[test]
fn nat_pmp_is_used_when_the_router_only_speaks_that() {
    let router = FakePmp::start(true);
    let gateways = Gateways {
        ssdp: silent(),
        pmp: Some(vec![router.address]),
        wait: Duration::from_millis(200),
    };
    let mapper = PortMapper::start_with(47_803, gateways);
    assert_eq!(
        wait_for_event(&mapper),
        MapEvent::Mapped {
            external: SocketAddr::from((OUTSIDE, 47_803)),
            method: MapMethod::NatPmp,
            permanent: false,
        }
    );
    drop(mapper);
    assert_eq!(
        router.asked(),
        vec![
            (2, 1, 0),
            (0, 1, LEASE_SECONDS),
            (0, 2, LEASE_SECONDS),
            (0, 0, 0),
            (0, 1, 0),
            (0, 2, 0)
        ],
        "PCP, then NAT-PMP's UDP and TCP mappings and outside address, then removed"
    );
}

#[test]
fn no_router_says_what_was_tried() {
    let gateways = Gateways {
        ssdp: silent(),
        pmp: Some(vec![silent()]),
        wait: Duration::from_millis(200),
    };
    let mapper = PortMapper::start_with(47_804, gateways);
    match wait_for_event(&mapper) {
        MapEvent::Failed(why) => {
            assert!(why.contains("UPnP: no router answered"), "{why}");
            assert!(why.contains("PCP and NAT-PMP at 127.0.0.1"), "{why}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn router_descriptions_and_answers_are_read() {
    let (status, body) = upnp::parse_answer(
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n",
    )
    .unwrap();
    assert_eq!((status, body.as_str()), (200, "hello world"));
    let services = upnp::elements(DESCRIPTION_XML, "service");
    assert_eq!(services.len(), 2);
    assert_eq!(
        upnp::elements(services[1], "controlURL"),
        vec!["ctl/IPConn"]
    );
    assert_eq!(
        upnp::elements(
            "<s:Body><u:X><NewExternalIPAddress>1.2.3.4</NewExternalIPAddress></u:X></s:Body>",
            "NewExternalIPAddress"
        ),
        vec!["1.2.3.4"]
    );
}

#[test]
fn default_gateways_are_read_from_the_system_tables() {
    let linux = "Iface\tDestination\tGateway \tFlags\tRefCnt\tUse\tMetric\tMask\n\
                 eth0\t0000A8C0\t00000000\t0001\t0\t0\t0\t00FFFFFF\n\
                 eth0\t00000000\t0101A8C0\t0003\t0\t0\t100\t00000000\n";
    assert_eq!(linux_gateway(linux), Some(Ipv4Addr::new(192, 168, 1, 1)));
    let windows = "IPv4 Route Table\n=====\nActive Routes:\n\
                   Network Destination        Netmask          Gateway       Interface  Metric\n\
                   0.0.0.0          0.0.0.0      192.168.0.254    192.168.0.23     25\n";
    assert_eq!(
        windows_gateway(windows),
        Some(Ipv4Addr::new(192, 168, 0, 254))
    );
}

#[test]
fn shared_and_private_outside_addresses_are_not_public() {
    for ip in [
        "192.168.1.1",
        "10.0.0.1",
        "100.64.0.1",
        "100.127.255.1",
        "127.0.0.1",
    ] {
        assert!(!is_public(ip.parse().unwrap()), "{ip}");
    }
    for ip in ["203.0.113.7", "100.128.0.1", "8.8.8.8"] {
        assert!(is_public(ip.parse().unwrap()), "{ip}");
    }
}
