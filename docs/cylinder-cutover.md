# Cylinder cutover (2026-11-05)

Live `-p` is **cylinder-shell** (not Flatland). Radius default nudged to **0.45**.
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
- Radius remains **0.45**.
