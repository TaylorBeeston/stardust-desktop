# Hexagon launcher → cylinder panels

## Desktop entries (monorepo)

`share/applications/stardust-host-mirror-*.desktop` call `stardust-cylinder-launch host-mirror --label …`.
After `just install`, copy/symlink into a data dir hexagon scans (`~/.local/share/applications` or `$PREFIX/share/applications` on `XDG_DATA_DIRS`).

## Protostar fork (TaylorBeeston/protostar @ roadie/cylinder-launch)

- Release near the cylinder belt (xz distance ≈ `CYLINDER_RADIUS_M`, default 0.70 ± 0.25 m) sets `STARDUST_LAUNCH_TARGET=cylinder`.
- `Application::launch` then uses `STARDUST_CYLINDER_WAYLAND_DISPLAY` instead of the default `WAYLAND_DISPLAY`.

Startup should export (staged cutover snippet):

```bash
# Discover cylinder wayland sock (same helper as mirrors-autostart)
export STARDUST_CYLINDER_WAYLAND_DISPLAY=wayland-N
export CYLINDER_RADIUS_M=0.70
```

Build/install the forked `hexagon_launcher` over bootstrap’s only after Taylor OK (may need session restart to pick up the binary).
