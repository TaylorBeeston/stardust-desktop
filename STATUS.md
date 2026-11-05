# stardust-desktop — status (2026-11-04 night, PT)

Morning note for Taylor. No headset session tonight. Live `-p` still Flatland.

## 1. Where things live

- Repo: https://github.com/TaylorBeeston/stardust-desktop (`main`)
- Workspace crates:
  - `crates/host-mirror` — ScreenCast → Stardust panel
  - `crates/desk-toys` — `wrist-menu`, `newtons-cradle`, `spin-sign`
  - `crates/cylinder-shell` — `cylinder-shell` worker + `cylinder-layout` daemon
- Design: `docs/cylinder-shell-design.md`
- Audit log: `.audit/productize.tsv`
- Install: `just install` (Flatland panel shell wrapper); `just install-cylinder-shell` (binaries only, does not swap `-p`)

## 2. Done tonight (measured)

- Monorepo stood up and published public; **14** commits on `main` (`da6ee18`…`931c3d6`).
- `cargo build --release --workspace` — ok.
- `cargo test --workspace` — ok: **cylinder-shell 29**, **desk-toys 9**, **host-mirror 1**.
- `scripts/verify.sh` — ALL OK (build, tests, bash -n, `--help`, temp-prefix install, panels.conf parse).
- Cylinder-shell headless: layout daemon + Unix-socket IPC, arc packing (gap ahead), curved GLB bake, chord-AABB Field, XR preflight (fail-fast without sockets), Asteroids accept path (`PanelItemAcceptor` → `PanelShell` → curved `SurfaceModel`), curved pointer UV wired (`CurvedPointerPlane`), mesh TEXCOORD_0 unified with hit-test (`u = t`, +X → u=1).
- Desk-toys: wrist-menu **place-here** (right-strip → kill+respawn with fresh head-relative `STARDUST_STARTUP_TOKEN`).
- Live Stardust session untouched: still `stardust_panel_shell` → Flatland; no WiVRn restart.

## 3. Architecture decision

**Candidate B** — per-window `PanelShell` worker (`wayland-service -p` oneshot) + `cylinder-layout` daemon owning shared cylinder state.

Not a Flatland fork. Temporary Flatland `curve_*` / `FLATLAND_CURVE_*` patches are a lab dead-end; product is drop-in windows with no `app_id` special cases. Hit-test stays with the worker that owns the curved mesh. Escape hatch toward Candidate A documented if Nova adds multi-item accept later.

## 4. Desk toys / wrist-menu (place-here)

Toys still have no relocate IPC. Place-here = stop then start so the toy reappears at current head pose via a new startup token. Right strip of a wrist-menu row is the place target when the toy is up (or starting); left/body still toggles. Kind tags optional in `items.txt`. Unit-covered (`place_here_*`, right-strip target).

## 5. Do NOT do yet

- Do **not** point live `stardust-xr-wayland-service -p` at `cylinder-shell`.
- Do **not** treat code UV unification as proof the headset image is correct — still need a mirrored-image check on-device.
- Do **not** assume concave face vs `SlotPlacement` is right until seen in headset (ported GLB bends toward −Z; placement rotates +Z toward axis).

## 6. First headset checklist (ordered)

1. Install cylinder bins (`just install-cylinder-shell`); keep Flatland on live `-p`.
2. Side-prefix / isolated smoke: one protected panel into `cylinder-shell` (not production `-p`).
3. Confirm which mesh face reads concave; flip placement or bake if wrong.
4. Confirm pointer cursor lands on the texel the eye sees (not mirrored; u convention holds).
5. Multi-app drop-in: Ghostty + browser + host-mirror on one cylinder, no `app_id` config.
6. Only after 3–5: consider swapping live `-p` to cylinder-shell; keep Flatland rollback path.
7. Then: group bead / radius under WiVRn latency, gap-ahead on resize, daemon death with live workers, Protostar stripped-env launch.

## 7. Open follow-ups

- **Group-grab from XR:** layout IPC has `GroupGrab` / `GroupChanged`, but the worker drops the layout socket before `asteroids::run` — no live XR→daemon channel yet. Needs persistent layout client task + re-pose on `GroupChanged`.
- **Touch:** `touch_*` not routed (hands drive pointer + pinch buttons only).
- **Popups / child surfaces:** Flatland `ChildState` path not ported to curved parent.
- **battery-widget:** still only under `stardust-bootstrap/org.stardustxr.Battery` — not in this monorepo.
- **Bootstrap DeskToys unsynced:** `stardust-bootstrap/org.stardustxr.DeskToys` differs from `crates/desk-toys` (wrist-menu place-here not copied back). Bootstrap install path may still run the old menu until synced or pointed at monorepo install.
- Field remains chord-AABB Box (Fusion has no cylinder-slice SDF) — capture gate only; landing is arc UV.
