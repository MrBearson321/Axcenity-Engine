//! A host's UDP socket and netcode server (in place of `renet_netcode`'s
//! server transport, which it follows): the same, plus players who reach
//! the host through a relay (MP5). Their packets arrive from the relay
//! inside `Data` packets with the relay's number for the player; netcode
//! sees each such player at an address of its own in `fd61:7872::/32`
//! (never a real one), and packets to that address go back through the
//! relay.

use std::io;
use std::net::{Ipv6Addr, SocketAddr, SocketAddrV6, UdpSocket};
use std::time::Duration;

use axcenity_relay::protocol::Packet;
use renet::{ClientId, RenetServer};
use renetcode::{NETCODE_MAX_PACKET_BYTES, NetcodeServer, ServerConfig, ServerResult};

pub(crate) struct HostSocket {
    socket: UdpSocket,
    netcode: NetcodeServer,
    buffer: Box<[u8; 2048]>,
    /// The relay the host is reached through.
    relay: Option<SocketAddr>,
    /// Netcode packets wrapped for the relay go here.
    wrapped: Vec<u8>,
}

/// The address netcode knows relayed player `player` by.
pub(crate) fn relayed_address(player: u32) -> SocketAddr {
    let ip = Ipv6Addr::new(
        0xfd61,
        0x7872,
        0,
        0,
        0,
        0,
        (player >> 16) as u16,
        player as u16,
    );
    SocketAddrV6::new(ip, 1, 0, 0).into()
}

/// The relay's number for the player at `address`, if it's a relayed one.
pub(crate) fn relayed_player(address: SocketAddr) -> Option<u32> {
    let SocketAddr::V6(address) = address else {
        return None;
    };
    let segments = address.ip().segments();
    (segments[..6] == [0xfd61, 0x7872, 0, 0, 0, 0])
        .then(|| (u32::from(segments[6]) << 16) | u32::from(segments[7]))
}

impl HostSocket {
    pub fn new(config: ServerConfig, socket: UdpSocket) -> io::Result<HostSocket> {
        socket.set_nonblocking(true)?;
        Ok(HostSocket {
            socket,
            netcode: NetcodeServer::new(config),
            buffer: Box::new([0; 2048]),
            relay: None,
            wrapped: Vec::with_capacity(NETCODE_MAX_PACKET_BYTES + 16),
        })
    }

    /// Takes players through `relay` from now on.
    pub fn set_relay(&mut self, relay: Option<SocketAddr>) {
        self.relay = relay;
    }

    /// The same socket, for the relay link's packets.
    pub fn socket_clone(&self) -> io::Result<UdpSocket> {
        self.socket.try_clone()
    }

    pub fn local_port(&self) -> Option<u16> {
        self.socket.local_addr().ok().map(|address| address.port())
    }

    pub fn client_addr(&self, id: ClientId) -> Option<SocketAddr> {
        self.netcode.client_addr(id)
    }

    pub fn disconnect_all(&mut self, server: &mut RenetServer) {
        for id in self.netcode.clients_id() {
            let result = self.netcode.disconnect(id);
            handle(result, &self.socket, self.relay, &mut self.wrapped, server);
        }
    }

    /// Advances netcode by `elapsed` and takes in every packet that came.
    pub fn update(&mut self, elapsed: Duration, server: &mut RenetServer) -> io::Result<()> {
        self.netcode.update(elapsed);
        loop {
            let (len, from) = match self.socket.recv_from(&mut self.buffer[..]) {
                Ok(got) => got,
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => break,
                Err(err) if err.kind() == io::ErrorKind::Interrupted => break,
                // A packet to a player who has gone shows here on Windows.
                Err(err) if err.kind() == io::ErrorKind::ConnectionReset => continue,
                Err(err) => return Err(err),
            };
            // A relayed player's packet, one of the relay's own, or a
            // netcode packet straight from a player.
            let relayed = match Packet::read(&self.buffer[..len]) {
                Some(Packet::Data { player, .. }) if Some(from) == self.relay => Some(player),
                Some(_) => continue,
                None => None,
            };
            let (from, packet) = match relayed {
                // The relay's header is 8 bytes: `AXR`, the kind and the
                // player.
                Some(player) => (relayed_address(player), &mut self.buffer[8..len]),
                None => (from, &mut self.buffer[..len]),
            };
            let result = self.netcode.process_packet(from, packet);
            handle(result, &self.socket, self.relay, &mut self.wrapped, server);
        }
        for id in self.netcode.clients_id() {
            let result = self.netcode.update_client(id);
            handle(result, &self.socket, self.relay, &mut self.wrapped, server);
        }
        for id in server.disconnections_id() {
            let result = self.netcode.disconnect(id);
            handle(result, &self.socket, self.relay, &mut self.wrapped, server);
        }
        Ok(())
    }

    /// Sends what renet has for each player.
    pub fn send_packets(&mut self, server: &mut RenetServer) {
        for id in server.clients_id() {
            let Ok(packets) = server.get_packets_to_send(id) else {
                continue;
            };
            for packet in packets {
                match self.netcode.generate_payload_packet(id, &packet) {
                    Ok((to, payload)) => {
                        if send(&self.socket, self.relay, &mut self.wrapped, payload, to).is_err() {
                            break;
                        }
                    }
                    Err(err) => {
                        log::warn!("network: can't seal a packet for {id}: {err}");
                        break;
                    }
                }
            }
        }
    }
}

/// Sends `packet` to `to`, through the relay for a relayed player.
fn send(
    socket: &UdpSocket,
    relay: Option<SocketAddr>,
    wrapped: &mut Vec<u8>,
    packet: &[u8],
    to: SocketAddr,
) -> io::Result<()> {
    match (relayed_player(to), relay) {
        (Some(player), Some(relay)) => {
            wrapped.clear();
            wrapped.extend(
                Packet::Data {
                    player,
                    payload: packet,
                }
                .to_bytes(),
            );
            socket.send_to(wrapped, relay).map(|_| ())
        }
        (Some(_), None) => Ok(()),
        (None, _) => socket.send_to(packet, to).map(|_| ()),
    }
}

fn handle(
    result: ServerResult,
    socket: &UdpSocket,
    relay: Option<SocketAddr>,
    wrapped: &mut Vec<u8>,
    server: &mut RenetServer,
) {
    match result {
        ServerResult::None => {}
        ServerResult::PacketToSend { payload, addr } => {
            let _ = send(socket, relay, wrapped, payload, addr);
        }
        ServerResult::Payload { client_id, payload } => {
            // A packet renet can't read is the player's problem; renet
            // drops it.
            let _ = server.process_packet_from(payload, client_id);
        }
        ServerResult::ClientConnected {
            client_id,
            addr,
            payload,
            ..
        } => {
            server.add_connection(client_id);
            let _ = send(socket, relay, wrapped, payload, addr);
        }
        ServerResult::ClientDisconnected {
            client_id,
            addr,
            payload,
        } => {
            server.remove_connection(client_id);
            if let Some(payload) = payload {
                let _ = send(socket, relay, wrapped, payload, addr);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relayed_players_have_addresses_of_their_own() {
        for player in [1, 2, 65_536, u32::MAX] {
            assert_eq!(relayed_player(relayed_address(player)), Some(player));
        }
        assert_eq!(relayed_player("203.0.113.4:47800".parse().unwrap()), None);
        assert_eq!(relayed_player("[2001:db8::1]:47800".parse().unwrap()), None);
    }
}
