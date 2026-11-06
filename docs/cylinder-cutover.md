# Cylinder cutover (2026-11-05)

Live `-p` is **cylinder-shell** (not Flatland). Radius default nudged to **0.70**.
Both host mirrors (eDP-1, Meta-0) launch into the cylinder compositor via
`stardust-mirrors-autostart` → `stardust-dual-mirror-flat` with
`STARDUST_WAYLAND_DISPLAY` pinned to the cylinder socket.

## Installed paths

- `~/.local/libexec/stardust_cylinder_shell` — wayland-service `-p` worker wrapper
- `~/dev/open-source/stardust-bootstrap/startup.sh` — starts cylinder-layout + cylinder `-p`
- `~/.local/libexec/stardust-mirrors-autostart` — ExecStartPost; waits for cylinder sock
- `~/.local/bin/stardust-dual-mirror-flat` — launches eDP-1 + Meta-0
- `~/.local/bin/stardust-cylinder-nudge` — SetRadius / GroupGrab / Reanchor IPC
- Side-stack helpers still at `~/.local/bin/cyl-headset-side-*` (optional)

## Backup / rollback

Backup: see newest `~/.local/share/roadie-backups/cylinder-cutover-*/`

```bash
~/.local/share/roadie-backups/cylinder-cutover-20261105-180024/rollback.sh
```

Restores Flatland panel shell + prior startup/mirrors scripts and restarts
**only** `stardust-session` (never WiVRn / WayVR / GNOME).

## Manifold

Still disabled in startup.sh (Taylor request). Do not relaunch without OK.

## Panel size + order (post-cutover fix)

- Hello metrics for host-mirror match Flatland WayVR sizes via `stardust_cylinder_shell`:
  - eDP-1: 2560×1600 @ density **2300** → **1.113 m**
  - Meta-0: 3440×1440 @ density **2070** → **1.662 m**
- Packing alternates RIGHT, LEFT by registration order. Autostart launches
  `STARDUST_MIRROR_OUTPUTS=Meta-0 eDP-1` so Meta is RIGHT and eDP is LEFT.
- Radius default **0.70** (Flatland).

## Layout daemon liveness (post-overlap incident)

Root cause of same-yaw overlap after a session restart: startup saw a **stale**
`layout.sock` from the previous session (`-S` true) and skipped starting
`cylinder-layout`. Workers then Hello-failed and both panels sat at the default
pose. Startup now checks the pidfile / clears a dead sock, and starts layout with
`CYLINDER_RADIUS_M=0.70` so radius is set **before** any panel registers.
Mirrors-autostart refuses to launch if the layout sock is not connectable.
Host-mirror Flatland metrics are always applied from `SDXR_WL_APP_ID` (inherited
`CYLINDER_DENSITY=3000` no longer wins).
