---
name: verify-stardust-desktop
description: Verify the stardust-desktop monorepo (cylinder-shell, host-mirror, desk-toys) like a user without a headset where possible — build/test, layout IPC scenarios, optional side-stack harness. Use whenever changing cylinder layout, panel shell, mirrors, or desk toys; never drive the live stardust-session.
---

# Verify stardust-desktop

Project-local verification for [TaylorBeeston/stardust-desktop](https://github.com/TaylorBeeston/stardust-desktop). Read this cold mid-task. The feature map under `features/` is the maintained source of user-facing paths.

## Surface

| Surface | What a user touches | Drive without headset? |
|---|---|---|
| **cylinder-layout IPC** | Shared cylinder packing / group grab / radius | Yes — throwaway Unix sock |
| **cylinder-shell worker** | Curved panel in VR | Partial — unit tests + side harness (needs live `stardust-0` server, still no WiVRn) |
| **host-mirror** | Monitor as a panel | CLI `--help` + unit tests; full path needs portal + Wayland |
| **desk-toys** | Wrist menu / cradle / spin-sign | Unit tests; XR needs headset |
| **scripts** | dual-mirror, nudge, side start/stop | `bash -n` + dry runs |

**Never** point verification at the live `$XDG_RUNTIME_DIR/stardust-cylinder/layout.sock` or replace live `-p`. Side harness uses a dedicated sock under `$XDG_STATE_HOME/cylinder-headset-side/`.

## Launch

From the monorepo root:

```bash
scripts/verify/doctor.sh          # read-only worth-driving check
cargo test --workspace            # unit/integration (no XR)
cargo build --release --workspace
scripts/verify/ipc-scenarios.sh   # real cylinder-layout binary on a temp sock
scripts/verify.sh                 # full headless suite (build/test/syntax/temp install)
```

Optional side stack (live `stardust-session` must already be up; does **not** restart it):

```bash
scripts/verify/side-harness.sh    # start side layout+wayland-service, Hello probe, stop
```

Ready means: doctor exits 0, `cargo test --workspace` exits 0, ipc-scenarios prints `ALL OK` and writes `.verify-evidence/ipc-*/results.json`.

## Doctor

`scripts/verify/doctor.sh` — crates present, scripts executable, cargo available. Notes if a live layout sock exists (reminder: do not use it).

## Drive

- **IPC:** `scripts/verify/ipc-scenarios.sh` (Hello packing, GroupGrab, SetRadius, MetricsChanged, Bye repack, Reanchor).
- **Math/packing:** `cargo test -p cylinder-shell --lib`.
- **Side stack:** `scripts/verify/side-harness.sh` when `stardust-server/stardust-0` exists.
- Prefer feature files under `features/` for the recipe + proof standard.

## Evidence

Default dir: `.verify-evidence/<suite>-<timestamp>/` (gitignored). Survives cleanup. Proof = action + resulting state (e.g. IPC request + `results.json` + layout.log excerpt), not unit-test green alone when the map lists an IPC path.

## Cleanup

- IPC script traps kill of **its** layout pid and removes its temp dir; evidence stays under `.verify-evidence/`.
- Side harness calls `cyl-headset-side-stop` (only pids it started).
- Never `pkill` by name; never kill grok-bot / WiVRn / WayVR / GNOME / stardust-session.

## Helpers

| Script | Role |
|---|---|
| `scripts/verify/doctor.sh` | Worth-driving? |
| `scripts/verify/ipc-scenarios.sh` | Layout IPC scenarios |
| `scripts/verify/side-harness.sh` | Isolated side compositor smoke |
| `scripts/verify.sh` | Full headless suite |
| `scripts/stardust-cylinder-nudge` | Manual GroupGrab/SetRadius/Reanchor (and Reorder/Swap when present) |

## Headset-only checks (do not fake)

Recorded in `features/headset-only.md`. Examples: concave mesh face vs eye, pointer UV on mirrored text, bead grab under WiVRn latency, pinch-drag panel swap, hexagon drop-on-cylinder, manifold keyboard into Ghostty.

## Maintenance

After cylinder/host-mirror/desk-toy behavior changes, update the matching `features/*.md` and re-run doctor + ipc-scenarios + `cargo test --workspace`. Use `/maintain-verification-skill` for periodic honesty passes.
