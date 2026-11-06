#!/usr/bin/env bash
# Read-only: is this checkout worth driving? Never touches live stardust-session.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"
ok=0; bad=0
check() { if "$@"; then echo "OK  $*"; ok=$((ok+1)); else echo "BAD $*"; bad=$((bad+1)); fi; }
check test -f Cargo.toml
check test -f crates/cylinder-shell/Cargo.toml
check test -f crates/host-mirror/Cargo.toml
check test -f crates/desk-toys/Cargo.toml
check test -x scripts/verify.sh
check test -x scripts/stardust-cylinder-nudge
check command -v cargo >/dev/null
check command -v rustc >/dev/null
# Refuse to claim a live session is "ours"
if [[ -S "${XDG_RUNTIME_DIR:-/run/user/$UID}/stardust-cylinder/layout.sock" ]]; then
  echo "NOTE live cylinder layout.sock present — verification must use side harness / temp socks, never this one"
fi
echo "doctor: $ok ok, $bad bad"
[[ "$bad" -eq 0 ]]
