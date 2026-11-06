# stardust-desktop

Desktop kit for [Stardust XR](https://stardustxr.org): mirror host monitors into VR as
curved panels on a shared cylinder, plus desk toys and a wrist menu.

| Path | Role |
|---|---|
| `crates/cylinder-shell` | Live panel shell: per-window `PanelShell` worker + `cylinder-layout` daemon. |
| `crates/host-mirror` | ScreenCast → Stardust Wayland window; mouse/keyboard back via uinput. |
| `crates/desk-toys` | `wrist-menu`, `newtons-cradle`, `spin-sign`. |
| `scripts/` | Dual-mirror launcher, cylinder cutover helpers, side-stack test scripts. |
| `examples/` | `panels.conf` (Flatland-era), `items.txt`. |
| `docs/` | Design, cutover, and headset-test notes. |

Owned clients use the `io.github.TaylorBeeston.*` app-id / resource namespace.
Upstream Stardust clients such as Comet stay `org.stardustxr.Comet`.

## Requirements

- Linux Wayland desktop with the **ScreenCast** portal and PipeWire (tested: GNOME / Mutter).
- Stardust XR: `stardust-xr-server`, `stardust-xr-wayland-service` with `-p` panel-shell support.
- OpenXR runtime (tested: WiVRn → Quest 3).
- Rust (stable), `just`, `python3` + PyGObject (multi-monitor pointer box).
- Write access to `/dev/uinput` for input injection, or `HOST_MIRROR_NO_INPUT=1`.

## Install

```sh
git clone https://github.com/TaylorBeeston/stardust-desktop
cd stardust-desktop
just build
just test
just install                 # PREFIX defaults to ~/.local
just install-cylinder-shell  # cylinder-shell, cylinder-layout, stardust_cylinder_shell
just install-config          # copies examples/ without overwriting
just verify
```

`just install` puts binaries and scripts in `$PREFIX/bin`, desk-toy models in
`$PREFIX/share/io.github.TaylorBeeston.DeskToys`, and the legacy Flatland wrapper in
`$PREFIX/libexec/stardust_panel_shell`.

`just install-cylinder-shell` installs:

- `$PREFIX/bin/cylinder-shell`, `$PREFIX/bin/cylinder-layout`
- `$PREFIX/libexec/stardust_cylinder_shell` (baked `$PREFIX`)

Add `$PREFIX/share` to `STARDUST_RES_PREFIXES` for desk-toy models.

## Agent verification

Skill: `.agents/skills/verify-stardust-desktop/` (also linked from `.cursor/skills/`).
Pointers in `AGENTS.md` / `CLAUDE.md`. Headless path:

```sh
./scripts/verify/doctor.sh
./scripts/verify.sh                  # build, test, clippy, temp install
./scripts/verify/ipc-scenarios.sh    # Hello/Bye/Metrics/GroupGrab/SetRadius/Reanchor/Swap/Shift
# optional: ./scripts/verify/side-harness.sh   # separate stardust server under /tmp/cyltest
```

Never point these at the live `$XDG_RUNTIME_DIR/stardust-cylinder/layout.sock`.
Headset-only checks live in the skill's `features/headset-only.md`.

Cylinder helpers (after install): `stardust-cylinder-nudge`, `stardust-cylinder-launch`,
`stardust-cylinder-toggle-mirror`. Hexagon desktop entries for Meta-0 / eDP-1 mirrors
are in `share/applications/`. Drop-near-belt launch needs the protostar fork
(`TaylorBeeston/protostar@roadie/cylinder-launch`).

## Live panel path (cylinder-shell)

Production `-p` is **cylinder-shell**, not Flatland. Shape:

1. `cylinder-layout` owns shared cylinder state (radius, gap, slots).
2. `stardust-xr-wayland-service -p …/stardust_cylinder_shell` starts one worker per toplevel.
3. Each worker Hellos to the layout socket, receives a slot, and draws a curved mesh.

Reference cutover script: `scripts/startup.sh.cylinder-cutover`.

```sh
# Layout sock: $XDG_RUNTIME_DIR/stardust-cylinder/layout.sock
# Clear a dead sock left by a previous session, then start the daemon with radius set
# BEFORE any panel Hello (do not rely on a post-hoc nudge alone).
CYLINDER_RADIUS_M=0.70 cylinder-layout &

WAYLAND_DISPLAY=$(display-socket-finder wayland)
stardust-xr-wayland-service -p "$PREFIX/libexec/stardust_cylinder_shell" "$WAYLAND_DISPLAY" &
```

Important env:

| Variable | Meaning |
|---|---|
| `CYLINDER_RADIUS_M` | Initial layout radius (metres). Default in cutover scripts: **0.70**. |
| `CYLINDER_LAYOUT_SOCK` | Override layout socket path. |
| `CYLINDER_DENSITY` / `CYLINDER_WIDTH_PX` / `CYLINDER_HEIGHT_PX` | Hello metrics. For host-mirror app_ids the wrapper always sets Flatland WayVR sizes (eDP-1 2560@2300 → 1.113 m, Meta-0 3440@2070 → 1.662 m) unless `CYLINDER_METRICS_LOCKED=1`. |

Packing alternates **RIGHT, LEFT, …** by registration order with a gap ahead.
Default mirror launch order is `STARDUST_MIRROR_OUTPUTS=Meta-0 eDP-1` so Meta is RIGHT and
eDP is LEFT. If a side would wrap past ±π, that side’s widths scale down from
`natural_width_m` to fit; raising the radius restores full size. Panels are never dropped.

Session helper: `scripts/stardust-mirrors-autostart` waits for a **connectable** layout sock
(refuses a stale `-S` sock), then launches the mirrors. Details: `docs/cylinder-cutover.md`.

### Side stack (no session restart)

Test cylinder beside a live Flatland session:

```sh
scripts/cyl-headset-side-start
scripts/cyl-headset-side-launch-app          # ghostty by default
scripts/cyl-headset-side-launch-mirror       # distinct cyl-* label; Flatland untouched
scripts/stardust-cylinder-nudge radius 0.70
scripts/cyl-headset-side-stop
```

### Rollback

Cutover backups live under `~/.local/share/roadie-backups/cylinder-cutover-*/` with
`rollback.sh` (restores Flatland panel shell + prior startup/mirrors scripts, restarts
**only** `stardust-session`). Side stack: `scripts/cyl-headset-side-stop`.

## Dual host mirrors

1. Start Stardust with cylinder `-p` and a live `cylinder-layout`.
2. Run `stardust-dual-mirror-flat` (or let `stardust-mirrors-autostart` do it).
   Default labels: **Meta-0 then eDP-1**. First run: portal dialog per label; tokens go to
   `~/.local/state/host-mirror/<label>.token`. Do not commit token files.
3. Stop: `pkill -f 'host-mirror[^ ]* --label'`.

Useful env: `HOST_MIRROR_BIN` (prefers `host-mirror-dmabuf` on PATH), `HOST_MIRROR_NO_INPUT=1`,
`HOST_MIRROR_REPLACE=1`, `STARDUST_WAYLAND_DISPLAY=wayland-N`,
`HOST_MIRROR_DESKTOP=x,y,w,h` (absolute pointer box; auto-detected on GNOME),
`HOST_MIRROR_DMABUF=0` (shm fallback).

`host-mirror` presents via **DMA-BUF** by default, injects mouse/keyboard through uinput,
listens for Stardust key sinks, and releases held mouse buttons on focus loss / cursor leave /
watchdog (`HOST_MIRROR_BUTTON_HOLD_MS`, default 10000). See `crates/host-mirror/README.md`.

## Desk toys and wrist menu

`wrist-menu` sits on the back of the left wrist. Tap the face to open it. Left/center of a
row toggles the item. When it is on, the right **"here"** strip places it in front of you
(kill + respawn with a fresh head-relative `STARDUST_STARTUP_TOKEN`).

Config: `~/.config/io.github.TaylorBeeston.WristMenu/items.txt` (migrates from
`org.stardustxr.WristMenu` on first run). Enabled set: `~/.local/state/wrist-menu/enabled`.

```
# label | command | offset right,up,forward (forward is -) [| kind]
Battery | battery-widget | 0.18,-0.2,-0.45 | widget
Spin sign | spin-sign | -0.22,-0.3,-0.45 | toy
Newton's cradle | newtons-cradle | 0.0,-0.32,-0.4 | toy
```

Physics:

- **Newton's cradle** — fixed physics ticks, frame-delta integration, balls pose-predicted to
  display time (`crates/desk-toys/src/inertia.rs`).
- **Spin sign** — same frame-delta + predicted board angle.

Models load from `io.github.TaylorBeeston.DeskToys` under `STARDUST_RES_PREFIXES`.

## Known gaps

- **Group-grab visual bead** — layout IPC `GroupGrab` / `SetRadius` / `Reanchor` work
  (`stardust-cylinder-nudge`); the Spatial bead UI is not shipped.
- **Keyboard into cylinder host-mirrors** — not validated in headset after cutover.
- **Manifold / simular** — can misroute presses across host-mirror panels (upstream gaze
  retarget). Keep them off unless you are debugging input.
- Touch / child surfaces / popups on cylinder are incomplete.
- Field capture uses a chord-AABB; landing uses arc UV.

## Flatland (legacy)

`scripts/stardust_panel_shell.in` and `FLATLAND_*` / `panels.conf` curve columns are the old
path. Stock Flatland ignores curve columns. Do not treat Flatland forks as the product;
cylinder-shell replaces that experiment. See `docs/cylinder-shell-design.md`.

## License

MIT (see `LICENSE`). `host-mirror` links **wlx-capture (GPL-3.0)**, so distributed
host-mirror binaries fall under GPL-3.0.
