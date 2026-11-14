use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, UdpSocket};
use std::time::Duration;

use super::protocol::{
    Bound, FromRelay, Packet, ToRelay, display_code, normalize_code, read_line, send_line,
};
use super::{Relay, RelaySettings, secret_from_hex};

fn relay() -> Relay {
    Relay::start(RelaySettings {
        port: 0,
        max_games: 2,
        max_players: 2,
    })
    .expect("a free port")
}

fn local(port: u16) -> SocketAddr {
    ([127, 0, 0, 1], port).into()
}

fn tcp(relay: &Relay) -> TcpStream {
    let stream = TcpStream::connect(local(relay.port())).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream
}

fn udp() -> UdpSocket {
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    socket
}

/// A host: its connection, code, secret and game socket (the relay knows
/// where it is).
fn host(relay: &Relay) -> (TcpStream, String, [u8; 16], UdpSocket) {
    let mut control = tcp(relay);
    send_line(
        &mut control,
        &ToRelay::Host {
            game: "Castle".to_owned(),
            reclaim: None,
        },
    )
    .unwrap();
    let FromRelay::Hosted { code, secret } = read_line(&mut control).unwrap() else {
        panic!("not hosted");
    };
    let secret = secret_from_hex(&secret).unwrap();
    let game = udp();
    game.send_to(&Packet::Host { secret }.to_bytes(), local(relay.port()))
        .unwrap();
    // Until the relay has taken it in.
    std::thread::sleep(Duration::from_millis(200));
    (control, code, secret, game)
}

fn receive(socket: &UdpSocket) -> (Vec<u8>, SocketAddr) {
    let mut buffer = [0u8; 2048];
    let (len, from) = socket.recv_from(&mut buffer).expect("a packet");
    (buffer[..len].to_vec(), from)
}

fn bind(relay: &Relay, player: &UdpSocket, code: &str) -> Bound {
    let code: [u8; 6] = code.as_bytes().try_into().unwrap();
    player
        .send_to(&Packet::Bind { code }.to_bytes(), local(relay.port()))
        .unwrap();
    let (bytes, _) = receive(player);
    match Packet::read(&bytes) {
        Some(Packet::Bound(bound)) => bound,
        other => panic!("{other:?}"),
    }
}

#[test]
fn packets_and_codes_read_back() {
    let to: SocketAddr = "203.0.113.4:47800".parse().unwrap();
    for packet in [
        Packet::Data {
            player: 7,
            payload: b"netcode",
        },
        Packet::Punch,
        Packet::Host { secret: [9; 16] },
        Packet::Bind { code: *b"K7QM2P" },
        Packet::Bound(Bound::To {
            host: None,
            you: to,
        }),
        Packet::Bound(Bound::To {
            host: Some(to),
            you: to,
        }),
        Packet::Bound(Bound::NoGame),
        Packet::Bound(Bound::Full),
    ] {
        assert_eq!(Packet::read(&packet.to_bytes()), Some(packet));
    }
    assert_eq!(Packet::read(b"\x05netcode"), None);
    assert_eq!(normalize_code(" k7q-m2p "), Some("K7QM2P".to_owned()));
    assert_eq!(normalize_code("K7QM2"), None);
    assert_eq!(normalize_code("K7QM2O"), None, "no O: it looks like 0");
    assert_eq!(display_code("K7QM2P"), "K7Q-M2P");
}

#[test]
fn a_player_bound_by_code_learns_where_the_host_is_and_the_host_is_asked_to_punch() {
    let relay = relay();
    let (mut control, code, _, game) = host(&relay);
    let player = udp();
    let Bound::To { host, you } = bind(&relay, &player, &code) else {
        panic!("not bound");
    };
    assert_eq!(host, Some(game.local_addr().unwrap()));
    assert_eq!(you, player.local_addr().unwrap());
    assert_eq!(
        read_line::<FromRelay>(&mut control).unwrap(),
        FromRelay::Punch { to: you }
    );
    assert_eq!(bind(&relay, &udp(), "ZZZZZZ"), Bound::NoGame);
}

#[test]
fn a_bound_players_packets_reach_the_host_and_its_answers_come_back() {
    let relay = relay();
    let (_control, code, _, game) = host(&relay);
    let player = udp();
    bind(&relay, &player, &code);
    player.send_to(b"\x05hello", local(relay.port())).unwrap();
    let (bytes, from) = receive(&game);
    assert_eq!(from, local(relay.port()));
    let Some(Packet::Data {
        player: id,
        payload,
    }) = Packet::read(&bytes)
    else {
        panic!("{bytes:?}");
    };
    assert_eq!(payload, b"\x05hello");
    game.send_to(
        &Packet::Data {
            player: id,
            payload: b"\x05back",
        }
        .to_bytes(),
        local(relay.port()),
    )
    .unwrap();
    let (bytes, from) = receive(&player);
    assert_eq!(
        (bytes.as_slice(), from),
        (&b"\x05back"[..], local(relay.port()))
    );

    // Two players at most: a third is told the game is full.
    bind(&relay, &udp(), &code);
    assert_eq!(bind(&relay, &udp(), &code), Bound::Full);
    assert_eq!(relay.players(), 2);
}

#[test]
fn a_join_by_code_is_spliced_to_a_connection_the_host_opens() {
    let relay = relay();
    let (mut control, code, secret, _game) = host(&relay);
    let mut player = tcp(&relay);
    send_line(
        &mut player,
        &ToRelay::Join {
            code: display_code(&code).to_lowercase(),
        },
    )
    .unwrap();
    let FromRelay::Open { id, from } = read_line(&mut control).unwrap() else {
        panic!("no open");
    };
    assert_eq!(from, player.local_addr().unwrap());
    let mut data = tcp(&relay);
    send_line(
        &mut data,
        &ToRelay::Data {
            id,
            secret: super::protocol::to_hex(&secret),
        },
    )
    .unwrap();
    assert_eq!(
        read_line::<FromRelay>(&mut player).unwrap(),
        FromRelay::Joined
    );
    player.write_all(b"client hello").unwrap();
    let mut got = [0u8; 12];
    data.read_exact(&mut got).unwrap();
    assert_eq!(&got, b"client hello");
    data.write_all(b"token").unwrap();
    drop(data);
    let mut answer = String::new();
    player.read_to_string(&mut answer).unwrap();
    assert_eq!(answer, "token");

    // No such game.
    let mut stranger = tcp(&relay);
    send_line(
        &mut stranger,
        &ToRelay::Join {
            code: "ZZZZZZ".to_owned(),
        },
    )
    .unwrap();
    let FromRelay::Refused { reason } = read_line(&mut stranger).unwrap() else {
        panic!("not refused");
    };
    assert!(reason.contains("ZZZ-ZZZ"), "{reason}");
}

#[test]
fn a_host_whose_connection_dropped_takes_its_code_back() {
    let relay = relay();
    let (control, code, secret, _game) = host(&relay);
    drop(control);
    std::thread::sleep(Duration::from_millis(1500));
    let mut again = tcp(&relay);
    send_line(
        &mut again,
        &ToRelay::Host {
            game: "Castle".to_owned(),
            reclaim: Some((code.clone(), super::protocol::to_hex(&secret))),
        },
    )
    .unwrap();
    let FromRelay::Hosted { code: same, .. } = read_line(&mut again).unwrap() else {
        panic!("not hosted");
    };
    assert_eq!(same, code);
    assert_eq!(relay.games(), 1);

    // A wrong secret gets a new code, and the relay's limit holds.
    let (_c, other, _, _) = host(&relay);
    assert_ne!(other, code);
    let mut third = tcp(&relay);
    send_line(
        &mut third,
        &ToRelay::Host {
            game: "Castle".to_owned(),
            reclaim: Some((code, "00".repeat(16))),
        },
    )
    .unwrap();
    assert!(matches!(
        read_line::<FromRelay>(&mut third).unwrap(),
        FromRelay::Refused { .. }
    ));
}
