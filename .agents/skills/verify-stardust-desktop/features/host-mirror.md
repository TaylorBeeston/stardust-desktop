# Host mirror

`host-mirror` captures one host monitor via xdg-desktop-portal ScreenCast and shows it as a Wayland toplevel for Stardust.

## Sub-features

- `label-token` — per-label restore token under `$XDG_STATE_HOME/host-mirror/`.
- `dual-flat` — `stardust-dual-mirror-flat` launches Meta-0 + eDP-1.
- `cylinder-target` — `STARDUST_WAYLAND_DISPLAY` selects the cylinder compositor sock.

## How to get to it (user POV)

- Autostart after stardust-session.
- Manual: `STARDUST_WAYLAND_DISPLAY=wayland-N stardust-dual-mirror-flat`.

## Driving it with verify.sh

Preconditions:

- Workspace builds.

- **Help/CLI.** `target/release/host-mirror --help` exits 0.
- **Unit.** `cargo test -p host-mirror`.
- **Full portal path** is headset/session-adjacent — see headset-only.

## Gotchas

- Capture is **whole output**, not an arbitrary host window (portal window capture is future work).
- Token files are secrets-adjacent — never commit or print them.
