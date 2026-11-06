# Overnight stage rollback — /home/taylor/.local/share/roadie-backups/overnight-20261105-204323

Created: Thu, 05 Nov 2026 20:43:55 -0800 (America/Los_Angeles)

## What was staged (binaries on disk; live layout/session NOT restarted)

Restored into ~/.local from this backup:

- bin/cylinder-shell
- bin/cylinder-layout
- bin/stardust-cylinder-nudge
- libexec/stardust_cylinder_shell

Newly added (remove to undo):

- bin/stardust-cylinder-launch
- bin/stardust-cylinder-toggle-mirror
- share/applications/stardust-host-mirror-meta-0.desktop
- share/applications/stardust-host-mirror-edp-1.desktop

## Rollback commands

```sh
BK='/home/taylor/.local/share/roadie-backups/overnight-20261105-204323'
PREFIX="$HOME/.local"
for f in cylinder-shell cylinder-layout stardust-cylinder-nudge; do
  [[ -e "$BK/bin/$f" ]] && cp -a "$BK/bin/$f" "$PREFIX/bin/$f"
done
[[ -e "$BK/libexec/stardust_cylinder_shell" ]] &&   cp -a "$BK/libexec/stardust_cylinder_shell" "$PREFIX/libexec/stardust_cylinder_shell"
rm -f "$PREFIX/bin/stardust-cylinder-launch" "$PREFIX/bin/stardust-cylinder-toggle-mirror"
rm -f "$PREFIX/share/applications/stardust-host-mirror-meta-0.desktop" \
      "$PREFIX/share/applications/stardust-host-mirror-edp-1.desktop"
```

After restoring binaries, a **session restart** (or at least cylinder-layout + wayland -p workers) is required for the old code to be the running process again — overnight did not restart anything.

## Needs session restart to take effect in VR

- New cylinder-layout (Swap/Reorder/Shift, MetricsChanged packing)
- New cylinder-shell workers (pinch-drag shift, MetricsChanged send, optional bead reanchor)
- Protostar drop-near-belt (separate install from TaylorBeeston/protostar@roadie/cylinder-launch @ 0e2b430) — NOT staged tonight

## Safe now without restart

- Scripts: stardust-cylinder-nudge (swap/reorder/shift cmds will fail until layout restart)
- stardust-cylinder-launch / toggle-mirror
- Desktop entries (hexagon will pick them up after protostar/desktop refresh)
- verify skill (headless)

main tip: 60cd67c (TaylorBeeston/stardust-desktop)
