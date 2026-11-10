#!/usr/bin/env bash
# Checks this computer's platform (macOS or Linux) without GitHub: what CI
# runs, plus the game test runner on an example and the dedicated server
# with no display. Windows: scripts/check-platform.ps1. See
# docs/development.md ("Checking another platform").
#
#   scripts/check-platform.sh
#
# Every step runs even if one fails. The summary is printed at the end and
# saved to target/platform-check-<os>.txt; the exit code is 1 if any failed.

set -u
cd "$(dirname "$0")/.."

os="$(uname -s | tr '[:upper:]' '[:lower:]')"
summary="target/platform-check-$os.txt"
results=()
failed=0

step() {
    local name="$1"
    shift
    echo
    echo "==> $name"
    if "$@"; then
        results+=("PASS  $name")
    else
        results+=("FAIL  $name")
        failed=1
    fi
}

no_editor_code() {
    ! cargo tree -p axcenity-player -e normal --prefix none --locked |
        grep -E '^(axcenity-editor|axcenity-blocks|axcenity-plugins|axcenity-cook|intel_tex_2|eframe|egui|subsecond) '
}

all_tests() {
    if cargo nextest --version >/dev/null 2>&1; then
        cargo nextest run --workspace --locked
    else
        echo "(cargo-nextest isn't installed: using cargo test)"
        cargo test --workspace --locked --no-fail-fast
    fi
}

# The server test starts the real binary; with no display it must still run.
headless_server() {
    env -u DISPLAY -u WAYLAND_DISPLAY cargo test -p axcenity-server --test headless --locked
}

step "Build" cargo build --workspace --all-targets --locked
step "The player has no editor code" no_editor_code
step "Clippy" cargo clippy --workspace --all-targets --locked -- -D warnings
step "Every test" all_tests
step "Doc tests" cargo test --workspace --doc --locked
step "axcenity-test examples/hello-character" \
    cargo run -p axcenity-testing --locked -- examples/hello-character
step "axcenity-server with no display" headless_server

mkdir -p target
{
    echo "Platform check on $(uname -sm), commit $(git rev-parse --short HEAD 2>/dev/null), $(date)"
    printf '%s\n' "${results[@]}"
} | tee "$summary"
echo "Saved to $summary"
exit $failed
