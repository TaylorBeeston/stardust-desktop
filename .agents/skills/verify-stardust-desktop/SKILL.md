---
name: verify-stardust-desktop
description: Verify the stardust-desktop monorepo (cylinder-shell, host-mirror, desk-toys) like a user — including an isolated headless Stardust desktop with in-engine screenshots. Use whenever changing cylinder layout, panel shell, mirrors, desk toys, or the verify harness. Never drive the live stardust-session.
---

# Verify stardust-desktop

Project-local verification for [TaylorBeeston/stardust-desktop](https://github.com/TaylorBeeston/stardust-desktop). Read this cold mid-task. The feature map under `features/` is the maintained source of user-facing paths.

## Surface

| Surface | What a user touches | Drive without headset? |
|---|---|---|
| **Headless flatscreen desktop** | Full stack as if in VR (panels, wire/bead, pointer) | Yes — `scripts/verify/headless-vr.sh` |
| **cylinder-layout IPC** | Packing / grab / radius / swap | Yes — throwaway Unix sock |
| **cylinder-shell worker** | Curved panel mesh | Yes via headless-vr; unit tests too |
| **host-mirror** | Monitor as a panel | CLI + unit tests; portal still needs session |
| **desk-toys** | Wrist menu / cradle / spin-sign | Unit tests; XR feel still headset |

**Never** point verification at the live `$XDG_RUNTIME_DIR/stardust-cylinder/layout.sock` or live `/run/user/$UID/stardust-server/stardust-0`. Headless-vr uses a **private** `XDG_RUNTIME_DIR` under `$XDG_STATE_HOME/cylinder-headless-vr/xdg`.

## Launch (headless VR — preferred for visual proof)

```bash
scripts/verify/doctor.sh
scripts/verify/headless-vr.sh start          # Xvfb + flatscreen server + layout + wayland -p
scripts/verify/headless-vr.sh launch kitty   # real Wayland client → cylinder panel
scripts/verify/headless-vr.sh screenshot with-panel
scripts/verify/headless-vr.sh state
scripts/verify/headless-vr.sh scenario smoke
scripts/verify/headless-vr.sh stop             # only kills recorded pids
```

Requirements (measured on FrenchFry):

- `~/.local/bin/stardust-xr-server-verify` (patched server with in-engine screenshots — see `staging/stardust-server-verify/`). Stock server works for IPC/input but X11 grabs of NVIDIA+Xvfb are black.
- `Xvfb`, `ffmpeg`, `xdotool`, `python3`+Pillow, lavapipe optional.
- Default `CYLVERIFY_VK=nvidia` for full stack. `lavapipe` can capture empty scenes but **crashes** when wayland clients attach (dmatex).

Ready means: doctor 0 bad, headless-vr status shows server/layout/wayland UP, screenshot meta has `unique_colors_approx > 10` or `pixels_gt20 > 1000`, panel registered in layout log.

Also always run:

```bash
cargo test --workspace
scripts/verify/ipc-scenarios.sh
scripts/verify.sh
```

## Doctor

`scripts/verify/doctor.sh` plus `scripts/verify/headless-vr.sh doctor`.

## Drive

| Goal | Command |
|---|---|
| Visual frame from head pose | `headless-vr.sh screenshot <label>` → PNG under `.verify-evidence/` and `docs/verify-screenshots/` |
| Head pose | `pose look-left\|look-right\|nudge-forward` or env `CYLVERIFY_CAM_YAW_DEG` at start |
| Pointer select | `click [x y]` (LMB) |
| Grab / pinch | `pinch tap\|drag [x y [dx dy]]` (RMB = grab in flatscreen) |
| Layout state | `state` → Assigned JSON |
| IPC packing | `ipc-scenarios.sh` (own temp sock) |

## Evidence

- `.verify-evidence/headless-vr-<ts>/` (gitignored) — PNGs + meta.json + scenario dumps
- `docs/verify-screenshots/*.png` — sample frames committed for humans/agents to Read
- Proof = action + resulting frame/state, not unit-test green alone

## Cleanup

`headless-vr.sh stop` kills only pids under `$STATE/pids/`. Evidence directories are kept. Never `pkill stardust`, never kill WiVRn / WayVR / GNOME / grok-bot / live cylinder-layout.

## Helpers

| Script | Role |
|---|---|
| `scripts/verify/headless-vr.sh` | Isolated flatscreen desktop + screenshots |
| `scripts/verify/doctor.sh` | Worth-driving? |
| `scripts/verify/ipc-scenarios.sh` | Layout IPC scenarios |
| `scripts/verify/side-harness.sh` | Optional side stack on **live** server (legacy) |
| `scripts/verify.sh` | Build/test/clippy/temp install |
| `staging/stardust-server-verify/` | Patch + README for verify server binary |

## Alternatives tried (measured)

| Approach | Result |
|---|---|
| `stardust-xr-server -f` under Xvfb + NVIDIA | Stack runs; `ffmpeg x11grab` / ImageMagick mostly **black** (NVIDIA doesn't present to Xvfb pixmap) |
| Same + lavapipe ICD | Empty flatscreen **capturable**; attaching wayland client → server panic in `dmatex.rs` |
| `-s --spectator` | Needs OpenXR session — not used (would touch WiVRn/Monado) |
| Monado simulated runtime | Not installed as a standalone user binary; WiVRn's monado must not be used for verify |
| In-engine `Screenshot::primary_window` via `CYLVERIFY_SHOT_REQUEST` | **Works** — real frames with panels/wire/bead |

## Headset-only (cannot simulate tonight)

See `features/headset-only.md` — Quest lens distortion feel, WiVRn latency, true hand tracking pinch precision, hexagon grab-and-drop with controllers, passthrough.

## Maintenance

After behavior changes, update `features/*.md`, refresh `docs/verify-screenshots/`, re-run `headless-vr.sh scenario smoke` + ipc-scenarios. Use `/maintain-verification-skill` for honesty passes.
