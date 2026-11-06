# Cylinder cutover

How the live session runs **cylinder-shell** instead of Flatland, and how to roll back.

## What runs

1. `cylinder-layout` with `CYLINDER_RADIUS_M=0.70` (sock
   `$XDG_RUNTIME_DIR/stardust-cylinder/layout.sock`).
2. `stardust-xr-wayland-service -p $PREFIX/libexec/stardust_cylinder_shell`.
3. `stardust-mirrors-autostart` → `stardust-dual-mirror-flat` with
   `STARDUST_MIRROR_OUTPUTS=Meta-0 eDP-1`.

Reference startup: `scripts/startup.sh.cylinder-cutover`.

Startup clears a **stale** layout sock when the pidfile process is dead, then starts layout
with radius set **before** any panel Hello. Autostart refuses to launch mirrors if the layout
sock exists but does not accept a connect (that failure mode put both panels at the same yaw).

## Panel size and order

- Hello metrics for host-mirror (from `stardust_cylinder_shell`):
  - eDP-1: 2560×1600 @ density **2300** → **1.113 m**
  - Meta-0: 3440×1440 @ density **2070** → **1.662 m**
- Packing: first registrant RIGHT. Launch Meta-0 first so eDP-1 is LEFT.
- Overflow: a side that would wrap past ±π scales widths from `natural_width_m`; raising
  radius restores full size. Panels are never dropped.
- Radius default **0.70** (Flatland).

## Side stack (no session restart)

```sh
scripts/cyl-headset-side-start
scripts/cyl-headset-side-launch-app
scripts/cyl-headset-side-launch-mirror   # label cyl-eDP-1; tokens seeded from eDP-1
scripts/stardust-cylinder-nudge radius 0.70
scripts/cyl-headset-side-stop
```

## Session lifetime (layout + mirrors)

`startup.sh` (cutover) always stops the previous `cylinder-layout` via its pid file,
clears `layout.sock`, and starts a fresh daemon with `CYLINDER_RADIUS_M=0.70` before
Hello. It also SIGTERMs leftover `stardust-xr-wayland-service -p …/stardust_cylinder_shell`
workers so a SEGV'd server cannot leave acceptors bound to a dead session.

`stardust-mirrors-autostart` defaults `HOST_MIRROR_REPLACE=1` and prefers the newest
cylinder wayland-service when choosing `WAYLAND_DISPLAY`, so Meta/eDP are relaunched
into the new session instead of skipping setsid orphans.

Monitor-dashboard Stop/Start/Restart Stardust also stop the layout daemon (pid file)
and clear the sock on stop; start/restart leave startup.sh to create a fresh one.

## Rollback

Backups: `~/.local/share/roadie-backups/cylinder-cutover-*/`

```sh
~/.local/share/roadie-backups/cylinder-cutover-<ts>/rollback.sh
```

Restores Flatland `stardust_panel_shell`, prior startup and mirrors scripts, then restarts
**only** `stardust-session` (never WiVRn / WayVR / GNOME).

Manifold / simular stay disabled in the cutover startup unless you uncomment them on purpose.
