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
