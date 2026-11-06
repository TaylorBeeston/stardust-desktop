# Verify screenshots

Sample frames from `scripts/verify/headless-vr.sh` on FrenchFry (in-engine capture via
`stardust-xr-server-verify`). Agents can `Read` these PNGs.

| File | What it shows (measured) |
|---|---|
| `with-panel.png` | Kitty on cylinder; wire/aabb; ~22k pixels lit |
| `after-input.png` | After click + pinch-drag |
| `empty-or-aabb.png` | Pre-client frame (often near-black until content) |
| `headless-looking-at-panel-boosted.png` | Contrast-boosted view of wire + panel + bead |

Regenerate: `scripts/verify/headless-vr.sh scenario smoke` then copy from `.verify-evidence/`.
