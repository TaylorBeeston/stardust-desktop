#!/usr/bin/env bash
# Palm-reach regressions (no live session / no headset).
# Proves: open-palm suppresses mouse, fist grabs distant bead (ray), panel slide path.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"
EVID="${VERIFY_EVIDENCE_DIR:-$ROOT/.verify-evidence}/palm-reach-$(date +%Y%m%d-%H%M%S)"
mkdir -p "$EVID"
echo "evidence=$EVID"

set +e
cargo test -p cylinder-shell --lib palm_wm 2>&1 | tee "$EVID/palm_wm-tests.txt"
rc1=${PIPESTATUS[0]}
cargo test -p cylinder-shell --lib sim_hands 2>&1 | tee "$EVID/sim_hands-tests.txt"
rc2=${PIPESTATUS[0]}
cargo test -p cylinder-shell --lib 2>&1 | tee "$EVID/cylinder-shell-all.txt"
rc3=${PIPESTATUS[0]}
set -e

{
  echo "palm_wm rc=$rc1"
  echo "sim_hands rc=$rc2"
  echo "all lib rc=$rc3"
  echo "scenarios:"
  echo "  1. open palm over panel → suppress hand→mouse (open_palm_over_panel_*)"
  echo "  2. fist ray grabs distant bead (fist_ray_grabs_distant_bead)"
  echo "  3. fist grab → slide yaw → open settle (fist_grabs_then_open_settles_*)"
  echo "  4. pinch clears reach, click path free (pinch_clears_reach_*)"
} | tee "$EVID/summary.txt"

test "$rc1" -eq 0 && test "$rc2" -eq 0 && test "$rc3" -eq 0
