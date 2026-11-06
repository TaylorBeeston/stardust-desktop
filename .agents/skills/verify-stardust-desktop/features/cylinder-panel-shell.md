# Cylinder panel shell

Per-window `cylinder-shell` worker accepted via `stardust-xr-wayland-service -p`, curved mesh + layout session.

## Sub-features

- `accept` — SDXR_PROTECTED_PANEL accept → SurfaceModel on curved GLB.
- `wire-bead` — one owner draws rings + gap bead; pinch sends GroupGrab.
- `side-harness` — second wayland-service on a free display, shared live server.

## How to get to it (user POV)

- Live: startup `-p stardust_cylinder_shell` + mirrors autostart.
- Side: `scripts/cyl-headset-side-start` then `cyl-headset-side-launch-app`.

## Driving it with side-harness

Preconditions:

- Live `stardust-server/stardust-0` socket exists (session already up).
- User has not asked to avoid side clients (GPU cost).

- **Smoke.** `scripts/verify/side-harness.sh`. Exit 0; evidence shows layout pid, wayland pid, and a successful Hello/Assigned on the side sock.
- **Unit proof (no server).** `cargo test -p cylinder-shell --lib`.

## Gotchas

- Side harness shares the live server — stop when done.
- Never swap live `-p` during verification.
- Wire ownership uses flock; only one worker draws rings.
