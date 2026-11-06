#!/usr/bin/env bash
# Isolated headless Stardust desktop for agent verification.
#
# Mimics being in VR with stardust-xr-server --force-flatscreen under Xvfb.
# Full stack (layout + cylinder-shell + Wayland clients) uses the NVIDIA Vulkan
# ICD. Frames are captured in-engine via CYLVERIFY_SHOT_REQUEST (requires
# stardust-xr-server-verify). X11 grab of NVIDIA+Xvfb is black — do not rely on it.
#
# NEVER touches live /run/user/$UID stardust socks, WiVRn, WayVR, GNOME, or dashboard.
#
# Usage: scripts/verify/headless-vr.sh <start|stop|status|doctor|launch|pose|click|pinch|key|screenshot|state|scenario>
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
BOOT="${STARDUST_BOOTSTRAP:-$HOME/dev/open-source/stardust-bootstrap}"
PREFIX="${PREFIX:-$HOME/.local}"
STATE="${CYLVERIFY_STATE:-${XDG_STATE_HOME:-$HOME/.local/state}/cylinder-headless-vr}"
HV_XDG="$STATE/xdg"
HV_CONFIG="$STATE/config"
HV_LOG="$STATE/logs"
EVID_ROOT="${VERIFY_EVIDENCE_DIR:-$ROOT/.verify-evidence}"
DISPLAY_NUM="${CYLVERIFY_DISPLAY_NUM:-97}"
# nvidia (default, full stack) | lavapipe (capturable empty scene only; wayland clients crash)
VK_MODE="${CYLVERIFY_VK:-nvidia}"
SERVER_BIN="${CYLVERIFY_SERVER_BIN:-}"
AABB="${CYLVERIFY_AABB_DEBUG:-1}"

die() { echo "headless-vr: $*" >&2; exit 1; }
need() { command -v "$1" >/dev/null || die "missing dependency: $1"; }

pick_server() {
  if [[ -n "$SERVER_BIN" ]]; then echo "$SERVER_BIN"; return; fi
  if [[ -x "$PREFIX/bin/stardust-xr-server-verify" ]]; then
    echo "$PREFIX/bin/stardust-xr-server-verify"; return
  fi
  if [[ -x "$BOOT/prefix/usr/bin/stardust-xr-server" ]]; then
    echo "$BOOT/prefix/usr/bin/stardust-xr-server"; return
  fi
  die "no stardust-xr-server / stardust-xr-server-verify"
}

ensure_bins() {
  need Xvfb; need ffmpeg; need xdotool; need python3
  pick_server >/dev/null
  [[ -x "$BOOT/prefix/usr/bin/stardust-xr-wayland-service" ]] || die "no wayland-service"
  [[ -x "$BOOT/prefix/usr/bin/display-socket-finder" ]] || die "no display-socket-finder"
  if [[ ! -x "$PREFIX/bin/cylinder-layout" || ! -x "$PREFIX/bin/cylinder-shell" ]]; then
    (cd "$ROOT" && cargo build --release -p cylinder-shell) || die "build cylinder-shell failed"
    install -Dm755 "$ROOT/target/release/cylinder-layout" "$PREFIX/bin/cylinder-layout"
    install -Dm755 "$ROOT/target/release/cylinder-shell" "$PREFIX/bin/cylinder-shell"
  fi
  mkdir -p "$PREFIX/libexec"
  if [[ -f "$ROOT/scripts/stardust_cylinder_shell.in" ]]; then
    sed "s|@PREFIX@|$PREFIX|g" "$ROOT/scripts/stardust_cylinder_shell.in" \
      > "$PREFIX/libexec/stardust_cylinder_shell"
    chmod 755 "$PREFIX/libexec/stardust_cylinder_shell"
  fi
  if [[ "$VK_MODE" == lavapipe && ! -f /usr/share/vulkan/icd.d/lvp_icd.json ]]; then
    die "lavapipe ICD missing"
  fi
}

write_env() {
  local shot_base="$STATE/shots/frame.png"
  mkdir -p "$STATE/shots"
  cat > "$STATE/env.sh" <<EOF
export CYLVERIFY_ROOT="$STATE"
export XDG_RUNTIME_DIR="$HV_XDG"
export XDG_CONFIG_HOME="$HV_CONFIG"
export XDG_STATE_HOME="$STATE/xdg-state"
export DISPLAY=":$DISPLAY_NUM"
unset WAYLAND_DISPLAY WAYLAND_SOCKET || true
export WINIT_UNIX_BACKEND=x11
export GDK_BACKEND=x11
export SDL_VIDEODRIVER=x11
export STARDUST_RES_PREFIXES="$BOOT/prefix/usr/share:\${STARDUST_RES_PREFIXES:-/usr/share}"
export PATH="$BOOT/prefix/usr/bin:$PREFIX/bin:\$PATH"
export CYLINDER_LAYOUT_SOCK="$STATE/layout.sock"
export CYLINDER_RADIUS_M="\${CYLINDER_RADIUS_M:-0.70}"
export CYLVERIFY_SHOT_REQUEST="$shot_base"
# Aim roughly at the first RIGHT-packed panel (~20°); override as needed
export CYLVERIFY_CAM_YAW_DEG="\${CYLVERIFY_CAM_YAW_DEG:-20}"
export CYLVERIFY_CAM_PITCH_DEG="\${CYLVERIFY_CAM_PITCH_DEG:-0}"
export CYLVERIFY_CAM_POS="\${CYLVERIFY_CAM_POS:-0,0,0}"
EOF
  if [[ "$VK_MODE" == lavapipe ]]; then
    cat >> "$STATE/env.sh" <<EOF
export VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json
export LIBGL_ALWAYS_SOFTWARE=1
EOF
  else
    cat >> "$STATE/env.sh" <<'EOF'
unset VK_ICD_FILENAMES LIBGL_ALWAYS_SOFTWARE || true
EOF
  fi
  if [[ -f "$STATE/wayland-display" ]]; then
    echo "export WAYLAND_DISPLAY=\"\$(cat \"$STATE/wayland-display\")\"" >> "$STATE/env.sh"
    echo "export STARDUST_WAYLAND_DISPLAY=\"\$(cat \"$STATE/wayland-display\")\"" >> "$STATE/env.sh"
  fi
  if [[ -f "$STATE/instance" ]]; then
    echo "export STARDUST_INSTANCE=\"\$(cat \"$STATE/instance\")\"" >> "$STATE/env.sh"
  fi
}

load_env() {
  [[ -f "$STATE/env.sh" ]] || die "not started; run: $0 start"
  # shellcheck disable=SC1091
  source "$STATE/env.sh"
}

pid_alive() { [[ -f "$1" ]] && kill -0 "$(cat "$1")" 2>/dev/null; }

cmd_stop() {
  local f p
  for f in "$STATE"/pids/*.pid; do
    [[ -f "$f" ]] || continue
    p=$(cat "$f" 2>/dev/null || true)
    if [[ -n "${p:-}" ]] && kill -0 "$p" 2>/dev/null; then
      kill "$p" 2>/dev/null || true
      for _ in 1 2 3 4 5 6 7 8; do kill -0 "$p" 2>/dev/null || break; sleep 0.1; done
      kill -9 "$p" 2>/dev/null || true
    fi
    rm -f "$f"
  done
  rm -f "$STATE/layout.sock"
  echo "headless-vr stopped (evidence kept under $EVID_ROOT and docs/verify-screenshots/)"
}

cmd_status() {
  echo "STATE=$STATE VK_MODE=$VK_MODE"
  [[ -f "$STATE/env.sh" ]] || { echo "not started"; return 0; }
  load_env
  echo "DISPLAY=$DISPLAY instance=$(cat "$STATE/instance" 2>/dev/null || echo ?)"
  echo "wayland=$(cat "$STATE/wayland-display" 2>/dev/null || echo none)"
  echo "server_bin=$(cat "$STATE/server_bin" 2>/dev/null || echo ?)"
  echo "layout_sock=$CYLINDER_LAYOUT_SOCK"
  local f name p
  for f in "$STATE"/pids/*.pid; do
    [[ -f "$f" ]] || continue
    name=$(basename "$f" .pid); p=$(cat "$f")
    if kill -0 "$p" 2>/dev/null; then echo "  UP  $name pid=$p"; else echo "  DEAD $name pid=$p"; fi
  done
}

cmd_start() {
  ensure_bins
  if pid_alive "$STATE/pids/server.pid"; then
    die "already running; stop first"
  fi
  mkdir -p "$STATE/pids" "$HV_XDG" "$HV_CONFIG/stardust" "$STATE/xdg-state" "$HV_LOG" "$STATE/evidence" "$STATE/shots"
  rm -rf "$HV_XDG/stardust-server" "$HV_XDG/stardust-debug" 2>/dev/null || true
  mkdir -p "$HV_XDG"

  # Minimal startup — no atmosphere (needs installed sky pack)
  cat > "$HV_CONFIG/stardust/startup" <<EOF
#!/usr/bin/env bash
echo "startup \$(date -Iseconds)" > "$STATE/startup.ran"
EOF
  chmod +x "$HV_CONFIG/stardust/startup"

  write_env
  # shellcheck disable=SC1091
  source "$STATE/env.sh"

  if ! pid_alive "$STATE/pids/xvfb.pid"; then
    Xvfb ":$DISPLAY_NUM" -screen 0 1280x720x24 -nolisten tcp >"$HV_LOG/xvfb.log" 2>&1 &
    echo $! > "$STATE/pids/xvfb.pid"
    sleep 0.4
    kill -0 "$(cat "$STATE/pids/xvfb.pid")" 2>/dev/null || die "Xvfb failed; see $HV_LOG/xvfb.log"
  fi

  local SBIN; SBIN=$(pick_server)
  echo "$SBIN" > "$STATE/server_bin"
  local EXTRA=()
  [[ "$AABB" == 1 ]] && EXTRA+=(--aabb-debug)

  # shellcheck disable=SC2086
  "$SBIN" -f -e "$HV_CONFIG/stardust/startup" --disable-startup-recenter "${EXTRA[@]}" \
    >"$HV_LOG/server.log" 2>&1 &
  echo $! > "$STATE/pids/server.pid"

  local inst=""
  for _ in $(seq 1 80); do
    if ! kill -0 "$(cat "$STATE/pids/server.pid")" 2>/dev/null; then
      tail -40 "$HV_LOG/server.log" || true
      die "server died during start"
    fi
    if [[ -d "$HV_XDG/stardust-server" ]]; then
      inst=$(find "$HV_XDG/stardust-server" -maxdepth 1 -type s -printf '%f\n' 2>/dev/null | head -1 || true)
      [[ -n "$inst" ]] && { echo "$inst" > "$STATE/instance"; break; }
    fi
    sleep 0.1
  done
  [[ -n "$inst" ]] || die "no instance sock under $HV_XDG/stardust-server"
  export STARDUST_INSTANCE="$inst"
  write_env; source "$STATE/env.sh"

  for _ in $(seq 1 60); do
    DISPLAY=":$DISPLAY_NUM" xdotool search --name 'Stardust' >/dev/null 2>&1 && break
    sleep 0.15
  done
  sleep 0.4

  rm -f "$STATE/layout.sock"
  CYLINDER_LAYOUT_SOCK="$STATE/layout.sock" CYLINDER_RADIUS_M="${CYLINDER_RADIUS_M:-0.70}" \
    cylinder-layout >"$HV_LOG/layout.log" 2>&1 &
  echo $! > "$STATE/pids/layout.pid"
  sleep 0.25
  kill -0 "$(cat "$STATE/pids/layout.pid")" 2>/dev/null || die "layout failed; see $HV_LOG/layout.log"
  echo "$STATE/layout.sock" > "$STATE/layout.sock.path"

  local WL; WL=$(display-socket-finder wayland)
  echo "$WL" > "$STATE/wayland-display"
  mkdir -p "$HV_XDG/stardust-protected-panel-provider"
  WAYLAND_DISPLAY="$WL" CYLINDER_LAYOUT_SOCK="$STATE/layout.sock" STARDUST_INSTANCE="$inst" \
    stardust-xr-wayland-service -p "$PREFIX/libexec/stardust_cylinder_shell" "$WL" \
      >"$HV_LOG/wayland.log" 2>&1 &
  echo $! > "$STATE/pids/wayland.pid"
  sleep 0.5
  if ! kill -0 "$(cat "$STATE/pids/wayland.pid")" 2>/dev/null; then
    tail -30 "$HV_LOG/wayland.log" || true
    die "wayland-service failed"
  fi
  write_env
  echo "headless-vr UP"
  echo "  STATE=$STATE"
  echo "  server=$SBIN"
  echo "  DISPLAY=:$DISPLAY_NUM instance=$inst wayland=$WL VK_MODE=$VK_MODE"
  echo "  shot via: $0 screenshot <label>"
}

cmd_doctor() {
  ensure_bins
  local ok=0 bad=0
  check() { if "$@"; then echo "OK  $*"; ok=$((ok+1)); else echo "BAD $*"; bad=$((bad+1)); fi; }
  check test -x "$(pick_server)"
  check test -x "$PREFIX/bin/cylinder-layout"
  if [[ "$(pick_server)" == *stardust-xr-server-verify ]]; then
    echo "OK  using verify server (in-engine screenshots)"
    ok=$((ok+1))
  else
    echo "NOTE stock server — install stardust-xr-server-verify for in-engine shots (see staging/stardust-server-verify/)"
  fi
  if [[ -f "$STATE/env.sh" ]]; then
    load_env
    check pid_alive "$STATE/pids/server.pid"
    check pid_alive "$STATE/pids/layout.pid"
    check pid_alive "$STATE/pids/wayland.pid"
    check test -S "$STATE/layout.sock"
    if [[ "$HV_XDG" == /run/user/* ]]; then echo "BAD private xdg collides with live"; bad=$((bad+1)); else echo "OK  private XDG=$HV_XDG"; ok=$((ok+1)); fi
  else
    echo "NOTE not started yet"
  fi
  [[ -S /run/user/$(id -u)/stardust-server/stardust-0 ]] && echo "NOTE live stardust-0 present — harness isolated"
  echo "doctor: $ok ok, $bad bad"
  [[ "$bad" -eq 0 ]]
}

cmd_launch() {
  load_env
  local APP="${1:-kitty}"; shift || true
  command -v "$APP" >/dev/null || die "app not found: $APP"
  local WL; WL=$(cat "$STATE/wayland-display")
  export CYLINDER_WIDTH_PX="${CYLINDER_WIDTH_PX:-960}"
  export CYLINDER_HEIGHT_PX="${CYLINDER_HEIGHT_PX:-540}"
  export CYLINDER_DENSITY="${CYLINDER_DENSITY:-3000}"
  if [[ "$(basename "$APP")" == ghostty ]]; then set -- --gtk-single-instance=false "$@"; fi
  env -u DISPLAY WAYLAND_DISPLAY="$WL" \
    CYLINDER_LAYOUT_SOCK="$STATE/layout.sock" \
    STARDUST_INSTANCE="$(cat "$STATE/instance")" \
    XDG_RUNTIME_DIR="$HV_XDG" \
    CYLINDER_WIDTH_PX="$CYLINDER_WIDTH_PX" CYLINDER_HEIGHT_PX="$CYLINDER_HEIGHT_PX" CYLINDER_DENSITY="$CYLINDER_DENSITY" \
    "$APP" "$@" >"$HV_LOG/app-${APP##*/}.log" 2>&1 &
  echo $! > "$STATE/pids/app.pid"
  echo "launched $APP pid=$(cat "$STATE/pids/app.pid") on $WL"
  for _ in $(seq 1 40); do
    rg -q 'panel registered' "$HV_LOG/layout.log" 2>/dev/null && break
    sleep 0.25
  done
  sleep 1
}

cmd_pose() {
  load_env
  local action="${1:-nudge-forward}"
  local WID; WID=$(DISPLAY=":$DISPLAY_NUM" xdotool search --name 'Stardust' | head -1) || die "no window"
  case "$action" in
    nudge-forward|forward) DISPLAY=":$DISPLAY_NUM" xdotool key --window "$WID" --delay 40 w w w w ;;
    nudge-back|back) DISPLAY=":$DISPLAY_NUM" xdotool key --window "$WID" --delay 40 s s s s ;;
    look-left)
      DISPLAY=":$DISPLAY_NUM" xdotool keydown --window "$WID" Shift_L
      DISPLAY=":$DISPLAY_NUM" xdotool mousedown --window "$WID" 3
      DISPLAY=":$DISPLAY_NUM" xdotool mousemove_relative -- -80 0
      DISPLAY=":$DISPLAY_NUM" xdotool mouseup --window "$WID" 3
      DISPLAY=":$DISPLAY_NUM" xdotool keyup --window "$WID" Shift_L ;;
    look-right)
      DISPLAY=":$DISPLAY_NUM" xdotool keydown --window "$WID" Shift_L
      DISPLAY=":$DISPLAY_NUM" xdotool mousedown --window "$WID" 3
      DISPLAY=":$DISPLAY_NUM" xdotool mousemove_relative -- 80 0
      DISPLAY=":$DISPLAY_NUM" xdotool mouseup --window "$WID" 3
      DISPLAY=":$DISPLAY_NUM" xdotool keyup --window "$WID" Shift_L ;;
    *) die "pose: nudge-forward|nudge-back|look-left|look-right" ;;
  esac
  echo "pose $action"
}

cmd_click() {
  load_env
  local x="${1:-640}" y="${2:-360}"
  local WID; WID=$(DISPLAY=":$DISPLAY_NUM" xdotool search --name 'Stardust' | head -1) || die "no window"
  DISPLAY=":$DISPLAY_NUM" xdotool mousemove --window "$WID" "$x" "$y"
  DISPLAY=":$DISPLAY_NUM" xdotool click --window "$WID" 1
  echo "click $x $y"
}

cmd_pinch() {
  load_env
  local phase="${1:-tap}" x="${2:-640}" y="${3:-360}" dx="${4:-0}" dy="${5:-0}"
  local WID; WID=$(DISPLAY=":$DISPLAY_NUM" xdotool search --name 'Stardust' | head -1) || die "no window"
  DISPLAY=":$DISPLAY_NUM" xdotool mousemove --window "$WID" "$x" "$y"
  case "$phase" in
    down) DISPLAY=":$DISPLAY_NUM" xdotool mousedown --window "$WID" 3 ;;
    up) DISPLAY=":$DISPLAY_NUM" xdotool mouseup --window "$WID" 3 ;;
    tap) DISPLAY=":$DISPLAY_NUM" xdotool click --window "$WID" 3 ;;
    drag)
      DISPLAY=":$DISPLAY_NUM" xdotool mousedown --window "$WID" 3
      sleep 0.05
      DISPLAY=":$DISPLAY_NUM" xdotool mousemove_relative -- "$dx" "$dy"
      sleep 0.05
      DISPLAY=":$DISPLAY_NUM" xdotool mouseup --window "$WID" 3 ;;
    *) die "pinch: down|up|tap|drag" ;;
  esac
  echo "pinch $phase $x,$y"
}

cmd_key() {
  load_env
  local WID; WID=$(DISPLAY=":$DISPLAY_NUM" xdotool search --name 'Stardust' | head -1) || die "no window"
  shift || true
  DISPLAY=":$DISPLAY_NUM" xdotool key --window "$WID" "$@"
  echo "key $*"
}

cmd_screenshot() {
  load_env
  local label="${1:-frame}"
  local ts; ts=$(date +%Y%m%d-%H%M%S)
  local dir="$EVID_ROOT/headless-vr-$ts"
  mkdir -p "$dir" "$STATE/evidence" "$ROOT/docs/verify-screenshots" "$STATE/shots"
  local dest="$dir/${label}.png"
  local base="$CYLVERIFY_SHOT_REQUEST"
  local use_engine=0
  if [[ -x "$(cat "$STATE/server_bin" 2>/dev/null || true)" ]] && \
     [[ "$(cat "$STATE/server_bin")" == *verify* ]]; then
    use_engine=1
  fi
  if [[ "$use_engine" == 1 ]]; then
    rm -f "${base}.done" "$base" "${base}.request"
    # Point request at dest via symlink base? Server writes to CYLVERIFY_SHOT_REQUEST path.
    # So set request on the env base, then copy.
    touch "${base}.request"
    local i
    for i in $(seq 1 80); do
      [[ -f "${base}.done" ]] && break
      sleep 0.1
    done
    [[ -f "$base" ]] || die "in-engine screenshot missing; see ${base}.done and $HV_LOG/server.log"
    cp -f "$base" "$dest"
  else
    echo "WARN falling back to ffmpeg x11grab (often black on NVIDIA+Xvfb)" >&2
    ffmpeg -y -f x11grab -video_size 1280x720 -i ":$DISPLAY_NUM.0" -frames:v 1 "$dest" \
      >"$HV_LOG/ffmpeg.log" 2>&1 || die "ffmpeg failed"
  fi
  cp -f "$dest" "$STATE/evidence/${label}.png"
  cp -f "$dest" "$ROOT/docs/verify-screenshots/${label}.png"
  python3 - "$dest" "$dir/${label}.meta.json" <<'PY'
import json,sys
from pathlib import Path
from PIL import Image
p=Path(sys.argv[1]); im=Image.open(p)
small=im.resize((64,36))
try:
  data=list(small.get_flattened_data())
except Exception:
  data=list(small.getdata())
cols=len(set(data))
meta={"path":str(p),"size":list(im.size),"mode":im.mode,
      "unique_colors_approx":cols,
      "center_rgb":list(im.getpixel((im.size[0]//2,im.size[1]//2)))[:3],
      "corner_tl":list(im.getpixel((0,0)))[:3],
      "pixels_gt20": sum(1 for px in im.getdata() if (px[0] if isinstance(px,tuple) else px)>20)}
Path(sys.argv[2]).write_text(json.dumps(meta,indent=2)+"\n")
print(json.dumps(meta))
if meta["unique_colors_approx"] < 3 and meta["pixels_gt20"] < 100:
  print("WARN nearly blank", file=sys.stderr); sys.exit(2)
PY
  echo "screenshot $dest"
}

cmd_state() {
  load_env
  local ts; ts=$(date +%Y%m%d-%H%M%S)
  local dir="$EVID_ROOT/headless-vr-state-$ts"
  mkdir -p "$dir"
  python3 - "$STATE/layout.sock" "$dir/layout-state.json" <<'PY'
import json,socket,sys,time
sock_path,out=sys.argv[1],sys.argv[2]
s=socket.socket(socket.AF_UNIX,socket.SOCK_STREAM); s.settimeout(2.0); s.connect(sock_path)
s.sendall(b'{"Hello":{"metrics":{"size_px":[320,200],"density_px_per_m":3000.0,"app_id":"verify-state-probe","title":"state"}}}\n')
buf=b''
while b'\n' not in buf: buf+=s.recv(65536)
ev=json.loads(buf.split(b'\n',1)[0])
slot=ev.get('Assigned',{}).get('slot',{})
s.sendall((json.dumps({'Bye':{'id':slot.get('id','')}})+'\n').encode()); s.close()
doc={'probe_assigned':ev,'layout_sock':sock_path,'ts':time.time()}
open(out,'w').write(json.dumps(doc,indent=2)+'\n'); print(json.dumps(doc,indent=2))
PY
  echo "state -> $dir/layout-state.json"
}

cmd_scenario() {
  local which="${1:-smoke}"
  load_env
  local ts; ts=$(date +%Y%m%d-%H%M%S)
  local dir="$EVID_ROOT/headless-vr-scenario-$which-$ts"
  mkdir -p "$dir"
  local rc=0
  case "$which" in
    smoke|all)
      cmd_doctor | tee "$dir/doctor.txt" || rc=1
      cmd_screenshot empty-or-aabb 2>"$dir/shot0.err" | tee "$dir/shot0.txt" || true
      if command -v kitty >/dev/null; then cmd_launch kitty || rc=1
      elif command -v ghostty >/dev/null; then cmd_launch ghostty || rc=1
      else echo "SKIP no kitty/ghostty" | tee -a "$dir/notes.txt"; fi
      sleep 2
      cmd_screenshot with-panel 2>"$dir/shot1.err" | tee "$dir/shot1.txt" || rc=1
      cmd_state | tee "$dir/state.txt" || rc=1
      cmd_click 700 360 || true
      sleep 0.2
      cmd_pinch drag 700 360 50 0 || true
      sleep 0.3
      cmd_screenshot after-input 2>"$dir/shot2.err" | tee "$dir/shot2.txt" || true
      # Separate IPC suite (own temp sock)
      "$ROOT/scripts/verify/ipc-scenarios.sh" >"$dir/ipc.txt" 2>&1 || rc=1
      ;;
    *) die "scenarios: smoke|all" ;;
  esac
  python3 - "$dir" "$rc" <<'PY'
import json,sys
from pathlib import Path
d=Path(sys.argv[1]); rc=int(sys.argv[2])
summary={"dir":str(d),"rc":rc,"files":sorted(p.name for p in d.iterdir())}
(d/"summary.json").write_text(json.dumps(summary,indent=2)+"\n")
print(json.dumps(summary,indent=2))
PY
  echo "scenario $which rc=$rc evidence=$dir"
  return "$rc"
}

usage() {
  cat <<EOF
Usage: $0 <command> [args]
  start | stop | status | doctor
  launch [app] | pose <action> | click [x y] | pinch <phase> ... | key ...
  screenshot [label] | state | scenario smoke|all

Env: CYLVERIFY_STATE CYLVERIFY_VK=nvidia|lavapipe CYLVERIFY_SERVER_BIN
     CYLVERIFY_CAM_YAW_DEG CYLVERIFY_AABB_DEBUG=1
Isolation: private XDG under \$STATE/xdg — never live /run/user/\$UID.
EOF
}

main() {
  local cmd="${1:-}"; shift || true
  case "$cmd" in
    start) cmd_start "$@" ;; stop) cmd_stop "$@" ;; status) cmd_status "$@" ;;
    doctor) cmd_doctor "$@" ;; launch) cmd_launch "$@" ;; pose) cmd_pose "$@" ;;
    click) cmd_click "$@" ;; pinch) cmd_pinch "$@" ;; key) cmd_key "$@" ;;
    screenshot) cmd_screenshot "$@" ;; state) cmd_state "$@" ;; scenario) cmd_scenario "$@" ;;
    -h|--help|help|"") usage ;;
    *) die "unknown: $cmd" ;;
  esac
}
main "$@"
