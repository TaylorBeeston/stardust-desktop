# Headset-only checks

Do **not** mark these verified from headless-vr or CI. Everything else in the feature map should be driven via `scripts/verify/headless-vr.sh` or IPC scripts first.

## Sub-features

- `quest-lens-feel` — optical distortion / IPD comfort (not a PNG property).
- `wivrn-latency` — bead grab and pointer lag under WiVRn to Quest.
- `true-hand-pinch` — controller/hand tracking pinch precision vs flatscreen RMB grab.
- `hexagon-controller-drop` — grab hexagon item with controller and release on cylinder belt.
- `passthrough-hands` — transparent hands over real world (`--transparent-hands`).

## How to get to it (user POV)

Wear Quest with WiVRn; live stardust-session on cylinder-shell.

## Driving it with headset

Taylor available; walk the list; record pass/fail with measured notes.

## Gotchas

- Headless-vr flatscreen RMB grab is a stand-in, not proof of hand tracking.
- In-engine PNGs prove geometry/layout/visibility, not WiVRn codec or comfort.
