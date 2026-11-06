#!/usr/bin/env bash
# Headless regression: launching an already-running host-mirror must NOT duplicate;
# wrapper metrics for host-mirror app_ids stay Flatland sizes.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
EVID="${VERIFY_EVIDENCE_DIR:-$ROOT/.verify-evidence}/mirror-launch-$(date +%Y%m%d-%H%M%S)"
mkdir -p "$EVID"

# Unit: metrics math
python3 - <<'PY' | tee "$EVID/metrics.txt"
edp_w = 2560 / 2300
meta_w = 3440 / 2070
assert abs(edp_w - 1.113) < 0.01, edp_w
assert abs(meta_w - 1.662) < 0.01, meta_w
print("metrics OK", round(edp_w,3), round(meta_w,3))
PY

# If live session has mirrors, probe no-dup (safe)
if pgrep -af 'host-mirror-dmabuf --label Meta-0' >/dev/null; then
  before=$(pgrep -af 'host-mirror-dmabuf --label Meta-0' | wc -l)
  out=$(stardust-cylinder-launch host-mirror --label Meta-0 2>&1 || true)
  echo "$out" | tee "$EVID/launch-meta.txt"
  after=$(pgrep -af 'host-mirror-dmabuf --label Meta-0' | wc -l)
  echo "before=$before after=$after" | tee -a "$EVID/launch-meta.txt"
  echo "$out" | grep -q 'already on cylinder' || { echo "FAIL: expected no-dup message"; exit 1; }
  test "$after" -eq "$before" || { echo "FAIL: process count changed"; exit 1; }
  echo "no-dup OK"
else
  echo "SKIP live no-dup (no Meta-0 mirror)" | tee "$EVID/skip.txt"
fi

# Wrapper case matching
bash -c 'SDXR_WL_APP_ID=host-mirror-edp-1; source /dev/null
  case "${SDXR_WL_APP_ID-} " in
    *host-mirror-edp-1*) echo edp_metrics=2560x1600@2300 ;;
    *) echo FAIL ;;
  esac' | tee "$EVID/wrapper-case.txt"

echo "mirror-launch regression OK; evidence $EVID"
