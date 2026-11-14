#!/usr/bin/env bash
# NAT lab (multiplayer readiness MP5): a host and players behind two
# different home routers, and an axcenity-relay on the "internet" between
# them, all on this Linux computer with network namespaces. Checks that
# players join by code: directly through a punched hole when both routers
# allow it, and through the relay when one doesn't; or (MP5b) by address
# when the host's router forwards the port it asked for.
#
#   sudo scripts/nat-lab.sh [cone|symmetric|upnp|pcp] [game] [bots]
#
# cone (default): both routers keep a port per inside address, as most
# home routers do, so hole punching works. symmetric: the players' router
# picks a new port for every destination, so it can't, and the relay
# carries the game. upnp and pcp: no relay; the host's router runs
# miniupnpd with only UPnP or only PCP and NAT-PMP, the host asks it to
# forward the game's port, and players join the router's address. Needs
# root, iproute2 and nftables (and miniupnpd-nftables for upnp and pcp);
# builds the binaries first. Leaves nothing behind (the namespaces go at
# the end).
set -euo pipefail

mode="${1:-cone}"
game="${2:-examples/hello-spawn}"
bots="${3:-4}"
root="$(cd "$(dirname "$0")/.." && pwd)"
out="${NAT_LAB_OUT:-$root/target/nat-lab/$mode}"
bin="$root/target/debug"

case "$mode" in cone | symmetric | upnp | pcp) ;; *) echo "mode is cone, symmetric, upnp or pcp" >&2; exit 2 ;; esac
[ "$(id -u)" = 0 ] || { echo "run as root (network namespaces)" >&2; exit 2; }

(cd "$root" && cargo build -q -p axcenity-relay -p axcenity-server --bins)
rm -rf "$out" && mkdir -p "$out"

spaces=(wan ra rb ha hb)
cleanup() {
    for pid in $(jobs -p); do kill "$pid" 2>/dev/null || true; done
    wait 2>/dev/null || true
    for space in "${spaces[@]}"; do ip netns del "lab-$space" 2>/dev/null || true; done
}
trap cleanup EXIT
cleanup

ns() { local space="$1"; shift; ip netns exec "lab-$space" "$@"; }
for space in "${spaces[@]}"; do
    ip netns add "lab-$space"
    ns "$space" ip link set lo up
done

# The internet: a bridge in `wan` with the relay on it, and each router's
# outside leg. Its addresses (11.0.0.0/24) look public, because miniupnpd
# won't forward on a private or documentation address; the namespaces
# never reach the real internet.
ns wan ip link add br0 type bridge
ns wan ip addr add 11.0.0.10/24 dev br0
ns wan ip link set br0 up
# `side` joins router $1 (outside address $2) to the internet and to its
# home network $3 (inside $3.1, the computer $3.2 in namespace $4).
side() {
    local router="$1" outside="$2" net="$3" home="$4"
    ip link add "w-$router" netns lab-wan type veth peer name wan netns "lab-$router"
    ns wan ip link set "w-$router" master br0 up
    ns "$router" ip addr add "$outside/24" dev wan
    ns "$router" ip link set wan up
    ip link add "h-$router" netns "lab-$router" type veth peer name eth0 netns "lab-$home"
    ns "$router" ip addr add "$net.1/24" dev "h-$router"
    ns "$router" ip link set "h-$router" up
    ns "$home" ip addr add "$net.2/24" dev eth0
    ns "$home" ip link set eth0 up
    ns "$home" ip route add default via "$net.1"
    ns "$router" sysctl -qw net.ipv4.ip_forward=1
}
side ra 11.0.0.1 10.1.0 ha
side rb 11.0.0.2 10.2.0 hb

# Routers: masquerade out of `wan`, and let nothing in that wasn't asked
# for (replies only), as a home router does.
router() {
    local router="$1" masquerade="$2"
    ns "$router" nft -f - <<EOF
table ip lab {
    chain outward { type nat hook postrouting priority 100; oifname "wan" $masquerade; }
    chain through {
        type filter hook forward priority 0; policy drop;
        ct state established,related accept
        # What the router was asked to forward (upnp and pcp).
        ct status dnat accept
        iifname != "wan" accept
    }
    # The router itself takes nothing unasked from outside either. (A
    # packet it accepted would leave a connection-tracking entry behind
    # that makes it map the next outgoing packet to that sender to another
    # port, which no home router does.)
    chain itself {
        type filter hook input priority 0; policy drop;
        ct state established,related accept
        iifname != "wan" accept
    }
}
EOF
}
router ra "masquerade"
if [ "$mode" = symmetric ]; then
    router rb "masquerade random,fully-random"
else
    router rb "masquerade"
fi

status=0
want=""
if [ "$mode" = cone ] || [ "$mode" = symmetric ]; then
    echo "== relay on 11.0.0.10:47810; host behind router A; $bots bots behind router B ($mode)"
    ns wan "$bin/axcenity-relay" --port 47810 >"$out/relay.log" 2>&1 &
    sleep 0.5
    AXCENITY_SAVE_DIR="$out/host-data" ns ha "$bin/axcenity-server" "$root/$game" \
        --relay 11.0.0.10:47810 --max-players 64 --no-port-map >"$out/server.log" 2>&1 &

    code=""
    for _ in $(seq 1 100); do
        code="$(grep -o 'Join code [A-Z0-9-]*' "$out/server.log" | head -1 | cut -d' ' -f3 || true)"
        [ -n "$code" ] && break
        sleep 0.1
    done
    [ -n "$code" ] || { echo "no join code"; cat "$out/server.log"; exit 1; }
    echo "== join code $code"
    ns hb "$bin/axcenity-bots" "$root/$game" --join "$code" --relay 11.0.0.10:47810 \
        --count "$bots" --seconds 15 --report 5 --report-file "$out/bots.jsonl" \
        >"$out/bots.log" 2>&1 || status=$?
    want=$([ "$mode" = cone ] && echo punched || echo relayed)
else
    # Router A forwards what the host asks for, over UPnP only or PCP and
    # NAT-PMP only. miniupnpd puts its forwarding rules in chains of its
    # own (`inet filter`); the lab's `through` chain lets them in.
    if [ "$mode" = upnp ]; then upnp=yes; pmp=no; else upnp=no; pmp=yes; fi
    ns ra nft -f - <<NFT
table inet filter {
    chain miniupnpd {}
    chain prerouting_miniupnpd {}
    chain postrouting_miniupnpd {}
    chain prerouting { type nat hook prerouting priority -100; jump prerouting_miniupnpd; }
    chain postrouting { type nat hook postrouting priority 100; jump postrouting_miniupnpd; }
}
NFT
    cat >"$out/miniupnpd.conf" <<CONF
ext_ifname=wan
listening_ip=h-ra
http_port=5000
enable_upnp=$upnp
enable_pcp_pmp=$pmp
secure_mode=yes
system_uptime=yes
uuid=4b1fa2c4-6f1d-4f43-9b52-6d1c6e2a7a10
allow 1024-65535 10.1.0.0/24 1024-65535
deny 0-65535 0.0.0.0/0 0-65535
CONF
    ns ra miniupnpd -d -f "$out/miniupnpd.conf" -P "$out/miniupnpd.pid" >"$out/miniupnpd.log" 2>&1 &
    sleep 0.5
    echo "== host behind router A (miniupnpd, $mode); $bots bots behind router B join its address"
    AXCENITY_SAVE_DIR="$out/host-data" ns ha "$bin/axcenity-server" "$root/$game" \
        --port 47800 --max-players 64 >"$out/server.log" 2>&1 &

    forwarded=""
    for _ in $(seq 1 100); do
        forwarded="$(grep 'The router forwards' "$out/server.log" | head -1 || true)"
        [ -n "$forwarded" ] && break
        grep -q "didn't forward" "$out/server.log" && break
        sleep 0.1
    done
    [ -n "$forwarded" ] || { echo "no port mapping"; cat "$out/server.log" "$out/miniupnpd.log"; exit 1; }
    link="$(echo "$forwarded" | grep -o 'axcenity://join/[^ ]*')"
    echo "== $(echo "$forwarded" | sed 's/^.*The router/The router/; s/;.*$//')"
    ns hb "$bin/axcenity-bots" "$root/$game" --join "$link" \
        --count "$bots" --seconds 15 --report 5 --report-file "$out/bots.jsonl" \
        >"$out/bots.log" 2>&1 || status=$?
    want=direct
    # A host that was killed leaves its mappings behind until their lease
    # ends; started again, it must get the port back.
    pkill -KILL -f "axcenity-server $root/$game" || true
    sleep 0.5
    AXCENITY_SAVE_DIR="$out/host-data" ns ha "$bin/axcenity-server" "$root/$game" \
        --port 47800 --max-players 64 >"$out/server-again.log" 2>&1 &
    again=""
    for _ in $(seq 1 100); do
        again="$(grep -E "The router forwards|didn't forward" "$out/server-again.log" | head -1 || true)"
        [ -n "$again" ] && break
        sleep 0.1
    done
    echo "== started again after a kill: $(echo "$again" | sed 's/^.*\(The router\)/\1/; s/;.*$//')"
    echo "$again" | grep -q "The router forwards" || status=1
fi
tail -n 1 "$out/bots.log"
summary="$(grep '"summary":true' "$out/bots.jsonl" | tail -1)"
echo "$summary" >"$out/summary.json"
python3 - "$mode" "$bots" "$want" "$out/summary.json" <<'EOF'
import json, sys
mode, bots, want, path = sys.argv[1], int(sys.argv[2]), sys.argv[3], sys.argv[4]
summary = json.load(open(path))
routes = summary.get("routes", {})
ok = summary["joined"] == bots and summary["connected"] == bots and routes.get(want) == bots
print(f"== {mode}: {summary['joined']} of {bots} joined, routes {routes}, "
      f"round trip p50 {summary['rtt_p50_ms']:.0f} ms, loss {summary['loss_mean'] * 100:.1f}%: "
      + ("PASS" if ok else f"FAIL (wanted every bot {want})"))
sys.exit(0 if ok else 1)
EOF
[ "$status" = 0 ] || { echo "bots exited with $status"; exit 1; }
echo "== logs in $out"
