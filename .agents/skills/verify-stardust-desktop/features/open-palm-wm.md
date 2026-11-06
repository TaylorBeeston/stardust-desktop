# Open-palm window management

## Sub-features

- `highlight` — open palm ray hits panel → outline/highlight
- `fist-slide` — curl to fist → panel follows along cylinder (Slide IPC)
- `settle` — open hand → nearest slot (Settle → Reorder+repack)
- `pinch-is-click` — pinch does not trigger palm WM (hysteresis)

## How to get to it (user POV)

In headset: open palm toward a panel, fist to grab, move, open to drop.

## Driving it with harness

- Unit: `cargo test -p cylinder-shell --lib palm_wm`
- IPC: `scripts/verify/ipc-scenarios.sh` includes `slide_settle`
- Pinch-drag swap removed (collided with click)

## Gotchas

- Real hand tracking feel remains headset-only; flatscreen has no articulated hands.
