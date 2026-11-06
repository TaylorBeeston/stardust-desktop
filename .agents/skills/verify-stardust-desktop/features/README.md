# stardust-desktop verification map

Maintained source for verifying user-facing behavior. Read this index, then the matching feature file.

## Baseline preconditions

- Work from a stardust-desktop checkout with `cargo` on PATH.
- Run `scripts/verify/doctor.sh` and require exit 0.
- Prefer throwaway layout socks (`CYLINDER_LAYOUT_SOCK` under a temp dir). **Never** send test Hellos to the live session sock.
- Evidence lands in `.verify-evidence/` and must survive cleanup.
- Live `stardust-session` / WiVRn / WayVR / GNOME are out of bounds unless a feature file explicitly says "side harness" and the user already has the session up.

## Driving conventions

- Start from baseline unless a feature lists other preconditions.
- Treat script paths and JSON IPC field names as literal.
- Report unreachable headset paths with the unmet precondition; do not mark them verified via a unit test alone.

## Features

- [Cylinder layout IPC](./cylinder-layout-ipc.md) — packing, group grab, radius, metrics, bye, reanchor.
- [Cylinder panel shell](./cylinder-panel-shell.md) — worker accept path, wire/bead, side harness.
- [Host mirror](./host-mirror.md) — ScreenCast → Stardust panel.
- [Desk toys / wrist menu](./desk-toys.md) — cradle, spin-sign, wrist place-here.
- [Headset-only checks](./headset-only.md) — must wait for Quest / WiVRn.
- `headless-vr.md` — isolated flatscreen desktop + screenshots for agents
- `open-palm-wm.md` — open palm grab / slide / settle
