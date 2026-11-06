# stardust-desktop — status (2026-11-05)

`main` tip tracks cylinder cutover, `io.github.TaylorBeeston.*` rename, and desk-toy inertia.

## Live product

- **Panel shell:** `cylinder-shell` + `cylinder-layout` (not Flatland).
- **Radius:** `CYLINDER_RADIUS_M=0.70` at layout start; cutover scripts also nudge 0.70.
- **Mirrors:** `STARDUST_MIRROR_OUTPUTS=Meta-0 eDP-1` (Meta RIGHT, eDP LEFT); Flatland WayVR
  panel metres via `stardust_cylinder_shell` (2300 / 2070 density).
- **Names:** owned clients `io.github.TaylorBeeston.*`. Upstream Comet remains
  `org.stardustxr.Comet`.

## Tests (workspace)

`cargo test --workspace` on current `main`: cylinder-shell 57, desk-toys lib 5,
newtons_cradle 9, spin_sign 7, wrist_menu 9, host-mirror 1 (88 total in the inertia tip).

## Docs map

| Doc | Mode |
|---|---|
| `README.md` | How to install / run |
| `docs/cylinder-cutover.md` | How the live cutover is wired + rollback |
| `docs/cyl-headset-test.md` | Side-stack headset checklist |
| `docs/cylinder-shell-design.md` | Why Candidate B (explanation) |
| `crates/host-mirror/README.md` | host-mirror reference |

## Open follow-ups

1. Visual group-grab bead (IPC already works).
2. Headset validation of keyboard into cylinder-hosted host-mirrors.
3. Manifold/simular press/release target latch (upstream).
4. Touch + child surfaces on cylinder.
5. Keep bootstrap DeskToys / wrist-menu install in sync with this monorepo when deploying.

## Overnight generalize (2026-11-05 night)

- Verification skill: `.agents/skills/verify-stardust-desktop/` (+ `.cursor/skills` symlink). Run `scripts/verify.sh`.
- Tiling IPC: `MetricsChanged` from surface resize, `Swap` / `Reorder` / `Shift`, pinch-drag panel shift, optional `CYLINDER_BEAD_REANCHOR=1`.
- Hexagon: desktop entries + `stardust-cylinder-launch`; protostar fork `TaylorBeeston/protostar` branch `roadie/cylinder-launch`.
- Host keybind: monitor toggle only (`stardust-cylinder-toggle-mirror`); per-window capture documented as future in `docs/host-window-keybind.md`.

## Overnight 2026-11-05 stage

Staged release binaries + helpers under `~/.local` with backup `~/.local/share/roadie-backups/overnight-*` + ROLLBACK.md. Live layout/session **not** restarted — tiling IPC / pinch-drag need morning session recycle. Protostar fork pushed (`roadie/cylinder-launch` @ 0e2b430) but not installed over live hexagon.

## Headless VR verify (2026-11-05 night)

`scripts/verify/headless-vr.sh` + `stardust-xr-server-verify` (in-engine screenshots).
Sample frames in `docs/verify-screenshots/`. Live session untouched. See skill + `staging/stardust-server-verify/`.

## Headset fixes 2026-11-06

- Mirror launch: no-dup if already running; Flatland metrics via wrapper (eDP 2560@2300, Meta 3440@2070).
- Toggle: clean in/out state machine (no foreground exec).
- Open-palm WM replaces pinch-drag swap (Slide/Settle IPC); pinch stays click.
- Staged under `~/.local` with `roadie-backups/headset-fixes-*`. **Needs stardust-session restart** for layout Slide/Settle + palm in workers.

## Palm reach (2026-11-06)

- **Root cause (measured):** Hand XR input reached `cylinder-shell`; open palm still drove `on_motion` → host-mirror mouse. Palm WM ray rarely hit; bead grab was near-field only.
- **Fix:** Open palm / fist-grab suppresses Hand→mouse; visible reach ray; fist grabs panel (Slide/Settle) or distant bead (GroupGrab). Pinch still clicks.
- **Verify:** `scripts/verify/palm-reach-regression.sh` (sim_hands + palm_wm). Needs session restart to pick up staged `cylinder-shell`.
