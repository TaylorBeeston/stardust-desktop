#!/usr/bin/env bash
# Headless verification (no headset). Never touches live stardust-session layout sock.
set -euo pipefail
cd "$(dirname "$0")/.."
step() { echo "== $*"; }
step doctor;  scripts/verify/doctor.sh
step build;   cargo build --release --workspace
step test;    cargo test --release --workspace
step clippy;  cargo clippy --workspace --all-targets
step ipc;     scripts/verify/ipc-scenarios.sh
step syntax;  for f in scripts/stardust-* scripts/*.in scripts/verify/*.sh scripts/cyl-*; do [[ -f "$f" ]] && bash -n "$f"; done
step help;    target/release/host-mirror --help >/dev/null
              scripts/stardust-dual-mirror-flat --help >/dev/null || true
              scripts/stardust-dual-mirror-recenter --help >/dev/null || true
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
step "install into $tmp"
PREFIX="$tmp" just install >/dev/null
PREFIX="$tmp" just install-cylinder-shell >/dev/null
for b in host-mirror wrist-menu newtons-cradle spin-sign cylinder-shell cylinder-layout stardust-dual-mirror-flat; do
  test -x "$tmp/bin/$b"
done
test -x "$tmp/libexec/stardust_cylinder_shell"
! grep -q @PREFIX@ "$tmp/libexec/stardust_cylinder_shell"
step "optional side-harness (skip ok)"
scripts/verify/side-harness.sh || true
echo "ALL OK"
