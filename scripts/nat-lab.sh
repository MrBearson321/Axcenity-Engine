#!/usr/bin/env bash
# NAT lab (multiplayer readiness MP5): a host and players behind two
# different home routers, and an axcenity-relay on the "internet" between
# them, all on this Linux computer with network namespaces. Checks that
# players join by code: directly through a punched hole when both routers
# allow it, and through the relay when one doesn't.
#
#   sudo scripts/nat-lab.sh [cone|symmetric] [game] [bots]
#
# cone (default): both routers keep a port per inside address, as most
# home routers do, so hole punching works. symmetric: the players' router
# picks a new port for every destination, so it can't, and the relay
# carries the game. Needs root, iproute2 and nftables; builds the binaries
# first. Leaves nothing behind (the namespaces go at the end).
set -euo pipefail

mode="${1:-cone}"
game="${2:-examples/hello-spawn}"
bots="${3:-4}"
root="$(cd "$(dirname "$0")/.." && pwd)"
out="${NAT_LAB_OUT:-$root/target/nat-lab/$mode}"
bin="$root/target/debug"

case "$mode" in cone | symmetric) ;; *) echo "mode is cone or symmetric" >&2; exit 2 ;; esac
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
# outside leg.
ns wan ip link add br0 type bridge
ns wan ip addr add 198.51.100.10/24 dev br0
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
side ra 198.51.100.1 10.1.0 ha
side rb 198.51.100.2 10.2.0 hb

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

echo "== relay on 198.51.100.10:47810; host behind router A; $bots bots behind router B ($mode)"
ns wan "$bin/axcenity-relay" --port 47810 >"$out/relay.log" 2>&1 &
sleep 0.5
AXCENITY_SAVE_DIR="$out/host-data" ns ha "$bin/axcenity-server" "$root/$game" \
    --relay 198.51.100.10:47810 --max-players 64 >"$out/server.log" 2>&1 &

code=""
for _ in $(seq 1 100); do
    code="$(grep -o 'Join code [A-Z0-9-]*' "$out/server.log" | head -1 | cut -d' ' -f3 || true)"
    [ -n "$code" ] && break
    sleep 0.1
done
[ -n "$code" ] || { echo "no join code"; cat "$out/server.log"; exit 1; }
echo "== join code $code"

status=0
ns hb "$bin/axcenity-bots" "$root/$game" --join "$code" --relay 198.51.100.10:47810 \
    --count "$bots" --seconds 15 --report 5 --report-file "$out/bots.jsonl" \
    >"$out/bots.log" 2>&1 || status=$?
tail -n 1 "$out/bots.log"
summary="$(grep '"summary":true' "$out/bots.jsonl" | tail -1)"
echo "$summary" >"$out/summary.json"
python3 - "$mode" "$bots" "$out/summary.json" <<'EOF'
import json, sys
mode, bots, path = sys.argv[1], int(sys.argv[2]), sys.argv[3]
summary = json.load(open(path))
want = "punched" if mode == "cone" else "relayed"
routes = summary.get("routes", {})
ok = summary["joined"] == bots and summary["connected"] == bots and routes.get(want) == bots
print(f"== {mode}: {summary['joined']} of {bots} joined, routes {routes}, "
      f"round trip p50 {summary['rtt_p50_ms']:.0f} ms, loss {summary['loss_mean'] * 100:.1f}%: "
      + ("PASS" if ok else f"FAIL (wanted every bot {want})"))
sys.exit(0 if ok else 1)
EOF
[ "$status" = 0 ] || { echo "bots exited with $status"; exit 1; }
echo "== logs in $out"
