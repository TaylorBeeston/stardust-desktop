# stardust-desktop

A small desktop kit for [Stardust XR](https://stardustxr.org): mirror your real monitors into
VR as Flatland panels (flat or on a curved cylinder, WayVR-style), plus a few desk toys and a
wrist menu to toggle them.

| path | what |
|---|---|
| `crates/host-mirror` | Wayland client that captures a monitor via the xdg-desktop-portal ScreenCast (wlx-capture/PipeWire) and shows it as a window inside Stardust; injects mouse/keyboard back via uinput. |
| `crates/desk-toys` | Stardust clients: `wrist-menu`, `newtons-cradle`, `spin-sign`. |
| `scripts/` | `stardust-dual-mirror-flat` (launch one mirror per monitor), `stardust-dual-mirror-recenter`, `stardust-recenter-all`, and `stardust_panel_shell.in` (per-window Flatland settings). |
| `examples/` | `panels.conf`, `items.txt`. |

The desk toys don't depend on host-mirror; they share a repo for organization only.

## Requirements

- Linux with a Wayland desktop that exposes the **ScreenCast portal** (tested: GNOME 50 / Mutter) and PipeWire.
- A Stardust XR stack: `stardust-xr-server`, `stardust-xr-wayland-service` (needs `-p` panel-shell support), and **Flatland**. Panels are shown flat for now (see Cylinder WM).
- An OpenXR runtime (tested: WiVRn → Quest 3).
- Rust (stable, edition 2024 for desk-toys), `just`, `python3` with PyGObject (only for the multi-monitor pointer box).
- Write access to `/dev/uinput` for input injection (e.g. `input` group / udev rule), or run with `HOST_MIRROR_NO_INPUT=1`.

## Install

```sh
git clone https://github.com/TaylorBeeston/stardust-desktop
cd stardust-desktop
just build            # cargo build --release --workspace
just test
just install          # PREFIX defaults to ~/.local; `PREFIX=/opt/stardust just install` for elsewhere
just install-config   # copies examples/ into ~/.config without overwriting
just verify           # headless self-check: build, tests, script syntax, temp-prefix install
```

`just install` puts:

- binaries and scripts in `$PREFIX/bin`
- desk-toy models in `$PREFIX/share/org.stardustxr.DeskToys`: add `$PREFIX/share` to `STARDUST_RES_PREFIXES` for the Stardust session
- the panel shell in `$PREFIX/libexec/stardust_panel_shell`, with `$PREFIX` baked in (protostar can launch it with a stripped environment)

Point the wayland service at the panel shell in your Stardust startup script:

```sh
stardust-xr-wayland-service -p "$PREFIX/libexec/stardust_panel_shell" &
```

Panel-shell env: `STARDUST_PREFIX` (overrides the baked prefix), `FLATLAND_BIN`,
`STARDUST_PANELS_CONF` (default `~/.config/stardust-desktop/panels.conf`). Any `FLATLAND_*`
var you export yourself takes priority over panels.conf. It logs to `$XDG_RUNTIME_DIR/stardust-panel-shell.log`.

## Dual monitors

1. Start Stardust (server + your startup script). These scripts never start or restart it.
2. `stardust-dual-mirror-flat`, which starts one `host-mirror --label <L>` per label in
   `STARDUST_MIRROR_OUTPUTS` (default `"eDP-1 Meta-0"`). The labels are just names. The first
   run opens one portal dialog per mirror: pick the matching monitor. The restore token is saved
   to `~/.local/state/host-mirror/<label>.token`; delete it to pick again. **Don't commit or
   share token files.**
3. Each mirror's Wayland app_id is `host-mirror-<label lowercased>`. Add one line per app_id to
   `panels.conf` for its density (px/m) and fixed resolution (the curve columns are experimental, see Cylinder WM). See
   `examples/panels.conf` (a 2560×1600 laptop panel and a 3440×1440 ultrawide on a 0.70 m cylinder).
4. (Experimental curved panels only) look straight ahead and run `stardust-dual-mirror-recenter` to re-place the curved panels.
   `--yaw +10`, `--height -0.1`, `--radius 0.9`, `--show`, `--reset` nudge the whole set.
   `stardust-recenter-all` (good on a hotkey) also runs a classic scene recenter through
   `stardust-shell recenter` if it's installed (`STARDUST_SHELL_BIN`).

Useful env: `HOST_MIRROR_BIN`, `HOST_MIRROR_NO_INPUT=1`, `HOST_MIRROR_REPLACE=1`,
`STARDUST_WAYLAND_DISPLAY=wayland-N`, `HOST_MIRROR_DESKTOP=x,y,w,h` (absolute pointer box; auto-detected on GNOME).
Stop: `pkill -f 'host-mirror[^ ]* --label'`. To turn curving off for every panel: `touch ~/.local/state/host-mirror/flat`.

## Wrist menu config

`wrist-menu` puts a watch on the back of your left wrist. Tap the face to open it. Tap the
**left/center** of a row to toggle that item on or off. When it is on, tap the right **"here"**
strip to **place it in front of you** — that kills the process and respawns it with a fresh
head-relative `STARDUST_STARTUP_TOKEN` (yaw-only upright pose at the configured offset). Desk toys
do not speak a relocate IPC yet, so place = respawn-at-head; a future toychest can stash vs activate
without changing this menu contract.

Items live in `~/.config/org.stardustxr.WristMenu/items.txt` (written with defaults on first run,
honours `XDG_CONFIG_HOME`):

```
# label | command (args ok) | spawn offset from your head: right,up,forward (forward is -) [| kind]
# kind is optional: toy (default) or widget — reserved for a future toychest; behaviour is the same today.
Battery | battery-widget | 0.18,-0.2,-0.45 | widget
Spin sign | spin-sign | -0.22,-0.3,-0.45 | toy
Newton's cradle | newtons-cradle | 0.0,-0.32,-0.4 | toy
```

Lines starting with `#` and lines with no command are skipped. A missing or bad offset falls back
to `0,-0.25,-0.45`. An unknown `kind` is ignored (treated as `toy`), so older three-column files
still load. Which items are on is stored in `~/.local/state/wrist-menu/enabled`. Widgets that are
already running get adopted.

Black-hole swallow (stash into the wrist) is a separate client and is not wired into this menu yet.

## Cylinder WM (planned panel shell)

The curved, tiling-WM-style layout is planned as a **dedicated Stardust panel shell**: a
`stardust-xr-wayland-service -p` panel shell that arranges *any* Wayland surface on a shared
cylinder around you (tiling-WM feel, drop-in windows, group grab/recenter). It is not built yet,
and it will land in this repo (e.g. `crates/cylinder-shell`) as its own crate.

**Today** the panels are plain Flatland. `stardust_panel_shell` runs whatever `flatland` it
finds (`FLATLAND_BIN` overrides) and passes per-window density and fixed resolution from
`panels.conf`. Stock Flatland shows the mirrors **flat**.

The `curve_r`/`curve_yaw` columns in `panels.conf` and the `FLATLAND_CURVE_*` env are left over
from a **temporary experiment**: curve patches on a personal Flatland branch. They aren't the
product, they aren't supported, and they will be replaced by the cylinder panel shell. Stock
Flatland ignores them, and `stardust-dual-mirror-recenter` only does anything with those
experimental panels.

## License

MIT (see `LICENSE`). host-mirror links **wlx-capture (GPL-3.0)**, so host-mirror binaries you
distribute are under GPL-3.0 terms.
