# Cylinder-shell headset test plan (prepared 2026-11-05)

Tip: monorepo `roadie/cyl-headset-test` (off `01dd391`). Live session stays Flatland.
WaylandService **staged2** (`94099ab` / sha `551662f7…`) must stay.

## Restart needed?

**No** for the first headset check. Use the **side path**: a second
`stardust-xr-wayland-service -p stardust_cylinder_shell` joins the live
`STARDUST_INSTANCE=stardust-0` server on a free Wayland display. Live Flatland
panels on `wayland-1` (host-mirror eDP-1 / Meta-0) are untouched.

**Yes** (stardust-session restart, or at least replacing live `-p` + new spawns)
only when swapping production `-p` from Flatland → cylinder after side validation.
`-p` is fixed in the running wayland-service cmdline / `startup.sh`; a clean full
swap needs a session restart. A libexec hot-replace affects **new** toplevels only.

## Go-live (side — preferred, no restart)

```sh
cd ~/dev/open-source/stardust-desktop
# bins already: just install-cylinder-shell
scripts/cyl-headset-side-start
scripts/cyl-headset-side-launch-app          # ghostty default
# optional second app:
# scripts/cyl-headset-side-launch-app kitty
```

Stop (Flatland untouched):

```sh
scripts/cyl-headset-side-stop
```

## Go-live (full -p swap — only after side checks pass)

```sh
scripts/cyl-headset-swap-prepare   # writes candidates + GO_LIVE/ROLLBACK under roadie-backups
# then follow that backup's GO_LIVE.txt (needs explicit Taylor OK + session restart for clean path)
```

## Rollback

- Side: `scripts/cyl-headset-side-stop`
- Swap: `scripts/cyl-headset-swap-rollback ~/.local/share/roadie-backups/cyl-headset-swap-<ts>`
  (add `--restart` only if startup.sh was swapped and session must reload)

Also: config snapshots at
`~/.local/share/roadie-backups/cyl-headset-test-20261105-103203/`.

## What to check in headset (ordered)

1. **Curve / concave face** — does the panel bend the right way (screen readable from inside)?
2. **Pointer UV** — cursor / click lands on the texel the eye sees (not mirrored, not slid).
3. **Multi-app pack** — second app joins the same cylinder with gap ahead; no `app_id` config.
4. **Group move** — visual bead is **not shipped yet** (STATUS gap 3). Use
   `scripts/cyl-headset-nudge-group grab 0.15 0` and confirm all panels yaw together.
5. Do **not** promote to live `-p` until 1–2 pass.

## Blockers / risks

- **No visual group-grab bead** — IPC `GroupGrab` works (unit-tested + nudge script); bead UI is follow-up.
- Touch / child surfaces / popups not routed on cylinder.
- Field is chord-AABB (capture gate only); landing is arc UV.
- Side wayland-service shares the live server — extra clients use GPU/CPU; stop when done.
- Never replace WaylandService binary (keep staged2). Never restart wivrn/wayvr/GNOME for this test.
- Protostar stripped-env: side wrapper bakes `PREFIX` + bootstrap share paths.
