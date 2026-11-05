#!/usr/bin/env bash
# Headless verification (no headset, no Stardust session touched).
set -euo pipefail
cd "$(dirname "$0")/.."
step() { echo "== $*"; }
step build;  cargo build --release --workspace
step test;   cargo test --release --workspace
step syntax; for f in scripts/stardust-* scripts/*.in; do bash -n "$f"; done
step help;   target/release/host-mirror --help >/dev/null
             scripts/stardust-dual-mirror-flat --help >/dev/null
             scripts/stardust-dual-mirror-recenter --help >/dev/null
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
step "install into $tmp"; PREFIX="$tmp" just install >/dev/null
for b in host-mirror wrist-menu newtons-cradle spin-sign stardust-dual-mirror-flat; do test -x "$tmp/bin/$b"; done
test "$(ls "$tmp"/share/org.stardustxr.DeskToys/*.glb | wc -l)" -ge 5
grep -q "PREFIX:-$tmp}" "$tmp/libexec/stardust_panel_shell"
! grep -q @PREFIX@ "$tmp/libexec/stardust_panel_shell"
step "panels.conf parse"
mkdir -p "$tmp/bin2"; printf '#!/bin/sh\nenv | grep ^FLATLAND_ | sort\n' > "$tmp/bin2/flatland"; chmod +x "$tmp/bin2/flatland"
out=$(env -u FLATLAND_DENSITY -u FLATLAND_CURVE_RADIUS XDG_RUNTIME_DIR="$tmp" XDG_STATE_HOME="$tmp/state" \
  STARDUST_PANELS_CONF=examples/panels.conf FLATLAND_BIN="$tmp/bin2/flatland" SDXR_WL_APP_ID=host-mirror-meta-0 \
  bash -c "\"$tmp/libexec/stardust_panel_shell\"; cat \"$tmp/stardust-panel-shell.log\"")
echo "$out" | grep -q "FLATLAND_DENSITY=2070" && echo "$out" | grep -q "FLATLAND_CURVE_YAW_DEG=70.27"
echo "ALL OK"
