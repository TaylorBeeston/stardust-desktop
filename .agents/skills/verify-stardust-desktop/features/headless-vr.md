# Headless VR desktop

Isolated flatscreen Stardust that mimics the VR desktop for agents.

## Sub-features

- `start-stop` — private XDG server + layout + wayland -p
- `launch-panel` — kitty/ghostty → cylinder panel
- `screenshot` — in-engine PNG from head pose
- `pointer-click-pinch` — LMB select / RMB grab
- `layout-state` — Assigned dump

## How to get to it (user POV)

An agent starts `scripts/verify/headless-vr.sh start` instead of putting on a headset.

## Driving it with headless-vr.sh

```bash
scripts/verify/headless-vr.sh start
scripts/verify/headless-vr.sh launch kitty
scripts/verify/headless-vr.sh screenshot with-panel
scripts/verify/headless-vr.sh click 700 360
scripts/verify/headless-vr.sh pinch drag 700 360 40 0
scripts/verify/headless-vr.sh stop
```

Proof: `docs/verify-screenshots/with-panel.png` (or latest evidence PNG) shows non-trivial pixels (`pixels_gt20` ≫ 0); layout log has `panel registered`; stop leaves live session untouched.

## Gotchas

- Needs `stardust-xr-server-verify` for usable screenshots on NVIDIA+Xvfb.
- Do not set harness `XDG_RUNTIME_DIR` to `/run/user/$UID`.
- `CYLVERIFY_VK=lavapipe` is empty-scene only.
