#!/usr/bin/env bash
# Optional: side cylinder compositor smoke against the LIVE server (no session restart).
# Skips cleanly if stardust-0 is down. Never touches live layout.sock / live -p.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"
EVID="${VERIFY_EVIDENCE_DIR:-$ROOT/.verify-evidence/side-$(date +%Y%m%d-%H%M%S)}"
mkdir -p "$EVID"
SERVER="${XDG_RUNTIME_DIR:-/run/user/$UID}/stardust-server/stardust-0"
if [[ ! -S "$SERVER" && ! -e "$SERVER" ]]; then
  echo "SKIP side-harness: no live server at $SERVER" | tee "$EVID/skip.txt"
  exit 0
fi
# Ensure bins
cargo build -p cylinder-shell --release >/dev/null
scripts/cyl-headset-side-stop >/dev/null 2>&1 || true
if ! scripts/cyl-headset-side-start >"$EVID/start.log" 2>&1; then
  echo "side start failed"; tail -40 "$EVID/start.log"; exit 1
fi
cleanup() { scripts/cyl-headset-side-stop >>"$EVID/stop.log" 2>&1 || true; }
trap cleanup EXIT
STATE="${XDG_STATE_HOME:-$HOME/.local/state}/cylinder-headset-side"
SOCK=$(cat "$STATE/layout.sock.path")
# Hello probe
python3 - "$SOCK" "$EVID" <<'PY'
import json,socket,sys
sock, evid = sys.argv[1], sys.argv[2]
s=socket.socket(socket.AF_UNIX,socket.SOCK_STREAM); s.settimeout(2); s.connect(sock)
s.sendall(b'{"Hello":{"metrics":{"size_px":[640,480],"density_px_per_m":3000.0,"app_id":"verify-probe","title":"probe"}}}\n')
buf=b''
while b'\n' not in buf: buf+=s.recv(65536)
open(evid+'/assigned.json','wb').write(buf.split(b'\n',1)[0]+b'\n')
ev=json.loads(buf.split(b'\n',1)[0])
assert 'Assigned' in ev, ev
s.sendall((json.dumps({'Bye':{'id':ev['Assigned']['slot']['id']}})+'\n').encode())
s.close()
print('side Hello/Assigned/Bye OK')
PY
cp "$STATE/wayland-display" "$EVID/" 2>/dev/null || true
echo "side-harness OK; evidence $EVID"
