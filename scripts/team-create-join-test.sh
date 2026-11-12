#!/usr/bin/env bash
# The joining machine's half of the two-machine Team Create test (macOS or
# Linux): joins through scripts/net-cut-relay.py, makes edits with
# scripts/team-create-edits.py, cuts the link for 5 minutes, and dumps the
# scene to compare with the host's. Run from the repository root:
#
#   scripts/team-create-join-test.sh "axcenity://teamcreate/join?host=…&cert=…&invite=…"
#
# The host machine runs its own edits.py with --prefix PC --other MAC
# --start-when MAC-001. Everything goes to $OUT (default ./tc-hardware).
set -euo pipefail

LINK=${1:?usage: $0 <invite link> [cut seconds]}
CUT_FOR=${2:-300}
OUT=${OUT:-$PWD/tc-hardware}
PORT=47899
mkdir -p "$OUT"
say() { echo "$(date '+%H:%M:%S') $*" | tee -a "$OUT/run.log"; }

# The invite's address goes to the relay; the certificate still pins the host.
TARGET=$(sed -E 's/.*[?&]host=([^&]+).*/\1/' <<<"$LINK")
RELAYED=$(sed -E "s/host=[^&]+/host=127.0.0.1:$PORT/" <<<"$LINK")
say "host $TARGET; checking it answers"
nc -z -w 5 "${TARGET%:*}" "${TARGET##*:}" || { say "can't reach $TARGET"; exit 1; }

say "building the editor"
start=$(date +%s)
cargo build --release -p axcenity-editor
say "built in $(( $(date +%s) - start )) s"
EDITOR_BIN=$PWD/target/release/axcenity-editor

rm -f "$OUT/CUT"
nohup python3 scripts/net-cut-relay.py --listen $PORT --target "$TARGET" \
    --cut-now-file "$OUT/CUT" --log "$OUT/relay.log" >/dev/null 2>&1 &
say "relay running (pid $!)"
sleep 1

touch "$OUT/.since"
nohup "$EDITOR_BIN" --team-join "$RELAYED" >"$OUT/editor.log" 2>&1 &
say "editor joining (pid $!)"

say "When the editor has opened the session, turn on Settings > AI connections > Allow AI connections in it."
MIRROR=""
while [ -z "$MIRROR" ]; do
    sleep 2
    # The newest endpoint file written since the editor started.
    for file in "$HOME/Library/Application Support/Axcenity Editor" "${XDG_DATA_HOME:-$HOME/.local/share}"; do
        [ -d "$file" ] || continue
        found=$(find "$file" -path '*eam Create*' -name mcp-endpoint.json -newer "$OUT/.since" 2>/dev/null | head -1)
        [ -n "$found" ] && MIRROR=$(dirname "$(dirname "$found")")
    done
done
say "session folder: $MIRROR"

python3 scripts/team-create-edits.py dump --editor "$EDITOR_BIN" --project "$MIRROR" \
    --out "$OUT/mac-scene-before.txt"
python3 scripts/team-create-edits.py edit --editor "$EDITOR_BIN" --project "$MIRROR" \
    --prefix MAC --other PC --every 15 --for 900 --settle 120 --x -6 \
    --out "$OUT/mac-scene.txt" --log "$OUT/mac-edits.log" >/dev/null 2>&1 &
DRIVER=$!
say "edits started (pid $DRIVER)"

sleep 120
touch "$OUT/CUT"
say "LINK CUT for $CUT_FOR s"
sleep "$CUT_FOR"
rm -f "$OUT/CUT"
say "LINK RESTORED"

say "waiting for the edits to end and the session to settle (about 13 minutes)"
wait $DRIVER || true
say "done: compare $OUT/mac-scene.txt.later with the host's pc-scene.txt.later"
say "editor lines about the session:"
grep -i 'team create' "$OUT/editor.log" | tail -30 | tee -a "$OUT/run.log" || true
