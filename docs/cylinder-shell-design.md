# Cylinder shell design

Status: scaffold sketch (2026-11-04). Product is a **proper Stardust panel shell**, not a Flatland fork.

## Overview

`cylinder-shell` is the planned `stardust-xr-wayland-service -p` panel shell that tiles *any* Wayland panel item onto a shared cylinder around the user (tiling-WM feel: drop-in windows, radius/yaw/height group controls, gap ahead, bead/group grab). It replaces today's temporary Flatland curve experiment (`FLATLAND_CURVE_*`, host-mirror app_id special-casing in the live wrapper).

Goals:

- Accept Ghostty, browsers, host-mirror, or anything wayland-service spawns — via the PanelShell / ProtectedPanel protocol only.
- Curved mesh **and** correct pointer hit-test (Flatland's flat input planes are a known gap).
- Shared cylinder state so panels never overlap and recenter as one group.
- No app_id hacks in the product path.

Non-goals for this sketch: restarting WiVRn/session, installing over the live Flatland prefix, shipping a working headset build tonight.

## Key Concepts

| Term | Meaning |
|------|---------|
| **Panel shell** | Process path passed to wayland-service `-p`. Launched once per new toplevel; connects via `SDXR_PROTECTED_PANEL` and presents a `PanelItemAcceptor`. |
| **PanelItem** | Hot-swappable 2D surface item from `stardust-xr-panels`. Shell draws it and forwards input. |
| **Cylinder layout** | Shared geometry: radius (m), group yaw/height, gap ahead (m of arc), panel slots along the arc. |
| **Bead / group grab** | Small handle (typically under the gap) that moves the whole cylinder rigidly. |
| **Hit-test ownership** | Who maps a 3D pointer ray onto panel UV/px. Must match the curved mesh, not a flat proxy plane. |
| **Layout daemon** | Long-lived process owning cylinder group state; panel workers are ephemeral per toplevel. |

## How PanelShell works (cited)

Measured on FrenchFry trees under `~/dev/open-source/stardust-bootstrap/`.

### Spawn: one shell process per toplevel

`org.stardustxr.WaylandService/src/default_panel_shell.rs` — `spawn_default_panel_shell`:

1. Builds a unique `SDXR_PROTECTED_PANEL=path:{name}:{secret}` and optional `SDXR_WL_APP_ID` / `SDXR_WL_TITLE`.
2. Binds a `ProtectedPanelProvider` under `$XDG_RUNTIME_DIR/stardust-protected-panel-provider/{name}`.
3. `protostar_launcher::launch(path, vars)` — **one new process per call**.
4. Waits up to 15s for the shell to call `get_panel` with a matching secret; the oneshot is then consumed (`AlreadyConsumed` on reuse).

Call site: `protocols/xdg/surface.rs` maps a new xdg toplevel and calls `spawn_default_panel_shell` (fallback: in-process `PanelItemUi` if spawn fails).

### Client accept: Flatland pattern

`org.stardustxr.Flatland/src/main.rs`:

- Asteroids client with `panel_item: Option<PanelItem>` — **one item per process**.
- `PanelItemAcceptor` component calls `protected_panel_provider::get_env_var_panel` (see `stardust-xr-panels-asteroids` `panel_item_acceptor.rs`): the bool flag is whether the item arrived via `SDXR_PROTECTED_PANEL`.
- `PanelShell::new(&shell, on_destroy)` wires resolution / app_id / children / cursor callbacks.

### Curve lab (not the product)

`Flatland/src/curve.rs`: env-driven cylinder slice mesh (`FLATLAND_CURVE_RADIUS` / `YAW` / `ANCHOR`). Comment at top: *"Visual only: the toplevel is drawn on a curved mesh; input planes stay flat (phase 2)."* Shared group state today is files under `$XDG_RUNTIME_DIR/flatland-curve/` (`adjust`, recenter, head anchor) polled by every process — a temporary multi-process coordination hack.

`Flatland/src/surfaces.rs`: when curved, `SurfaceModel` uses a baked GLB; pointer/touch still use flat `PointerPlane` / `TouchPlane` in `surface_input.rs`.

### Live wrapper anti-pattern

`~/.local/libexec/stardust_panel_shell` (and the monorepo `scripts/stardust_panel_shell.in` panels.conf path) special-cases `host-mirror-*` app_ids for density and curve yaw. **Product cylinder-shell must not copy app_id branching**; layout comes from the shared cylinder + panel metrics, not mirror labels.

### Crates Flatland uses

From Flatland `Cargo.toml`: `stardust-xr-fusion`, `stardust-xr-asteroids`, `stardust-xr-molecules`, `stardust-xr-panels` + `stardust-xr-panels-asteroids` (git `StardustXR/panel-item`), `gluon-ipc`, `glam`, `tokio`.

## Candidate A — single multi-panel process

**Shape:** One long-lived `cylinder-shell` Asteroids client holds `HashMap<PanelId, PanelSlot>`. The `-p` path is a thin launcher that either (1) starts the daemon if missing, or (2) forwards the fresh `SDXR_PROTECTED_PANEL` binding to the running process over a local socket, then exits.

**Pros**

- True single owner of cylinder mesh, gap, bead, and recenter — no IPC for layout.
- Hit-test and group grab live in one scene graph; no cross-process Field sync.
- Natural tiling-WM mental model (one compositor-like shell).

**Cons**

- Fights today's spawn model: wayland-service always `launch`es a new process and expects *that* process's acceptor to consume the oneshot (`AlreadyConsumed` if a second client tries). Forwarding requires either a custom handoff protocol or wayland-service changes.
- Process death takes down every panel.
- Cold-start latency of the first panel includes full XR client bring-up for the shared shell; subsequent handoffs need a race-free attach API that does not exist yet.

## Candidate B — per-window PanelShell + layout daemon

**Shape:** Match Flatland's process model. Each `-p` launch is a **panel worker** (`cylinder-shell`) that accepts exactly one `PanelItem` via `PanelItemAcceptor` / `SDXR_PROTECTED_PANEL`. A separate **`cylinder-layout`** daemon owns `CylinderState` (radius, yaw, height, gap, bead pose, slot assignments). Workers register size/app metadata, receive a slot pose, bake their own curved mesh, and apply group transforms from the daemon.

**Pros**

- Matches measured spawn model (`default_panel_shell.rs`): one process, one secret, one `get_panel`, Flatland-identical acceptor path.
- No wayland-service changes; drop-in `-p` binary.
- Panel crash isolation; worker lifecycle follows toplevel death (`PanelShell` on_destroy → process exit with `-d` / derez-with-item).
- Hit-test ownership is local: each worker owns its curved Field + mesh (closes Flatland's flat-plane gap without centralizing rays).

**Cons**

- Layout IPC required (replace file-polling `flatland-curve/adjust` with an explicit protocol).
- Bead/group grab must update daemon state; workers subscribe — extra failure modes if daemon dies (workers freeze poses or fall back flat).
- Duplicate XR client connections per window (same cost Flatland pays today).

## Synthesis decision

**Choose Candidate B for v1**, with a documented escape hatch toward A.

Why:

1. **Spawn model fit.** wayland-service `-p` spawns one shell per toplevel and consumes the ProtectedPanel oneshot in that process (`default_panel_shell.rs`). B is that model. A needs new attach semantics.
2. **Hit-test.** Product requires curved mesh + correct hit-test. Owning both on the worker that already owns `SurfaceModel` and pointer callbacks is the smallest change from Flatland's structure while fixing the known gap (curved Field / mesh raycast in-process, not a flat proxy).
3. **Shared cylinder without app_id hacks.** Daemon assigns slots from panel pixel size + density + gap policy — Ghostty and host-mirror look the same. No `SDXR_WL_APP_ID` branching.
4. **Escape hatch.** If Nova later adds multi-item accept or wayland-service learns to attach to a running shell, the daemon + worker IPC boundary collapses into Candidate A's in-process map; types (`CylinderState`, `PanelSlot`, `HitTest`) stay.

Rejected for v1: pure A (handoff wrapper) — too much protocol invention before first curved panel works; pure Flatland-fork — explicitly ruled out by product correction.

## Module map + type sketch

```
crates/cylinder-shell/
  Cargo.toml
  src/
    lib.rs              // modules + shared error type
    main.rs             // panel worker binary (wayland-service -p target)
    bin/cylinder-layout.rs  // layout daemon binary
    cylinder.rs         // CylinderState, SlotId, group controls
    panel.rs            // PanelWorker state (one PanelItem)
    mesh.rs             // curved GLB bake (port concepts from Flatland curve.rs lab)
    hit_test.rs         // curved pointer/touch mapping ownership
    layout_ipc.rs       // worker ↔ daemon protocol
```

### Types (signatures; bodies `todo!()` / not-implemented in scaffold)

```rust
/// Shared cylinder group state. Owned by `cylinder-layout`; workers hold a snapshot.
pub struct CylinderState {
    pub radius_m: f32,
    pub yaw_rad: f32,      // group rotation about vertical
    pub height_m: f32,     // vertical offset of the belt
    pub gap_m: f32,        // empty arc ahead of the user
    pub anchor: CylinderAnchor,
}

pub enum CylinderAnchor {
    HmdAtGrab,
    Stage { x: f32, y: f32, z: f32 },
}

pub struct SlotId(pub u64);

/// One panel's place on the cylinder, assigned by the daemon.
pub struct PanelSlot {
    pub id: SlotId,
    pub yaw_rad: f32,      // centre angle on the cylinder (+ = right)
    pub width_m: f32,
    pub height_m: f32,
}

pub struct PanelMetrics {
    pub size_px: (u32, u32),
    pub density_px_per_m: f32,
    pub app_id: Option<String>,
    pub title: Option<String>,
}

impl CylinderState {
    pub fn assign_slot(&mut self, metrics: &PanelMetrics) -> PanelSlot { todo!() }
    pub fn release_slot(&mut self, id: SlotId) { todo!() }
    pub fn apply_group_grab(&mut self, delta_yaw: f32, delta_height: f32) { todo!() }
    pub fn set_radius(&mut self, radius_m: f32) { todo!() }
}

/// Worker-side: one PanelItem, curved draw + hit-test.
pub struct PanelWorker {
    pub slot: Option<PanelSlot>,
    pub metrics: PanelMetrics,
    // panel_item / shell held by Asteroids state in the binary
}

impl PanelWorker {
    pub fn on_accept_panel(/* shell: Node<PanelShellHandler> */) { todo!() }
    pub fn apply_slot(&mut self, slot: PanelSlot, group: &CylinderState) { todo!() }
}

/// Hit-test ownership: the panel worker that owns the curved mesh also owns
/// mapping pointer rays → surface UV/px. Daemon never routes pointers.
pub trait CurvedHitTest {
    fn ray_to_surface_uv(&self, origin: [f32; 3], direction: [f32; 3]) -> Option<(f32, f32)>;
    fn uv_to_panel_px(&self, uv: (f32, f32), size_px: (u32, u32)) -> (f32, f32);
}

pub struct CylinderSliceHitTest {
    pub radius_m: f32,
    pub yaw_rad: f32,
    pub width_m: f32,
    pub height_m: f32,
}

impl CurvedHitTest for CylinderSliceHitTest { /* todo */ }

pub fn bake_curved_panel_glb(width_m: f32, height_m: f32, radius_m: f32) -> std::path::PathBuf {
    todo!()
}

/// IPC: worker registers, daemon replies with slot + CylinderState snapshots.
pub enum LayoutRequest {
    Hello { metrics: PanelMetrics },
    Bye { id: SlotId },
    MetricsChanged { id: SlotId, metrics: PanelMetrics },
}

pub enum LayoutEvent {
    Assigned { slot: PanelSlot, group: CylinderState },
    GroupChanged { group: CylinderState, slot: PanelSlot },
    Shutdown,
}

pub trait LayoutClient {
    fn connect() -> Result<Self, std::io::Error> where Self: Sized;
    fn send(&mut self, req: LayoutRequest) -> Result<(), std::io::Error>;
    fn try_recv(&mut self) -> Result<Option<LayoutEvent>, std::io::Error>;
}
```

Binaries:

- `cylinder-shell` — Asteroids client; `PanelItemAcceptor` → one panel; connect `LayoutClient`; curved `SurfaceModel` + `CylinderSliceHitTest`.
- `cylinder-layout` — listens on `$XDG_RUNTIME_DIR/stardust-cylinder/layout.sock` (path TBD); owns `CylinderState`; serves bead/recenter later.

Install stub: `just install` may install both under `$PREFIX/bin` and point a future `stardust_cylinder_shell` libexec wrapper at `cylinder-shell` (does **not** replace today's Flatland-backed `stardust_panel_shell` until ready).

## Implementation status (2026-11-04, headless fill-in)

Implemented and unit-tested without a headset (`cargo test -p cylinder-shell`, 16 tests):

- **Layout** (`cylinder.rs`): `assign_slot` keeps an empty `gap_m` arc straight ahead and packs
  panels alternately right/left outward in registration order, `PANEL_SPACING_M` (2 cm) apart,
  using arc length (so yaw scales 1/r). `release_slot` / `update_metrics` / `set_radius`
  (clamped 0.2–5 m) re-pack everyone; `apply_group_grab` wraps yaw to (-π, π]; `reanchor`.
- **IPC** (`layout_ipc.rs`, `cylinder-layout` bin): newline-delimited JSON over a Unix socket at
  `$CYLINDER_LAYOUT_SOCK` or `$XDG_RUNTIME_DIR/stardust-cylinder/layout.sock`. Daemon replies
  `Assigned` to the newcomer and pushes `GroupChanged` to every other worker on any change;
  a dropped connection is treated as `Bye`. `bind` refuses a live socket and replaces a stale one.
- **Hit-test** (`hit_test.rs`): ray ↔ cylinder in the cylinder-local frame (axis +Y, forward −Z,
  yaw + = right, belt at y = 0); front face only (ray from inside); u right, v down.
- **Mesh** (`mesh.rs`): Flatland `curve.rs` GLB bake ported (pure std, headless), cached under
  `$XDG_RUNTIME_DIR/cylinder-shell/`.
- **Worker** (`panel.rs`, `main.rs`): `SlotPlacement` (pose + hit-test from slot+group);
  binary registers with the daemon, bakes its mesh, and without `SDXR_PROTECTED_PANEL` runs a
  `NoServer` dry run (exit 0). With `SDXR_PROTECTED_PANEL` set it exits 2: the Asteroids
  `PanelItemAcceptor` client is **not** written yet (needs a live server to verify).

### Deviations from the sketch above

1. `CylinderState` carries `slots` + `next_id` (`#[serde(skip)]`) so the daemon's layout can
   live on the type the sketch put the methods on. Snapshots on the wire omit them.
2. `LayoutRequest` gained `GroupGrab`, `SetRadius`, `Reanchor` (bead/recenter controls).
3. `bake_curved_panel_glb` returns `io::Result<PathBuf>`; `bake_curved_panel_glb_in(dir, ..)` added.
4. `PanelWorker::apply_slot` returns `SlotPlacement`; `on_accept_panel` only marks accepted.
5. Re-pack on removal is order-based, so a neighbour can switch sides (tiling feel, not sticky).

### Open (headset)

Placement orientation vs the mesh's concave side: `SlotPlacement` rotates +Z to face the axis
while the ported GLB bends toward −Z (Flatland's *measured* concave convention). Confirm in
headset which face reads concave and flip one side if needed. Plus gaps 1–8 above.

## Headset-only verification gaps

Cannot close without a live WiVRn / Quest session (do **not** restart tonight):

1. Curved Field / ray hit-test vs flat proxy (the Flatland phase-2 gap) — confirm pointer lands on the mesh the eye sees.
2. Multi-app drop-in: Ghostty + browser + host-mirror on one cylinder without app_id config.
3. Group bead grab and radius drag under motion reprojection / WiVRn latency.
4. Gap-ahead stability when panels resize (density / fixed-res changes).
5. Daemon death / restart while workers live.
6. Protostar stripped-env launch of `cylinder-shell` (PATH / `STARDUST_RES_PREFIXES` baked like today's libexec wrapper).
7. Touch path on curved mesh (mirror of pointer hit-test).
8. Child surfaces / popups (Flatland `ChildState`) on a curved parent.

## References

- Monorepo README § "Cylinder WM (planned panel shell)"
- WaylandService `src/default_panel_shell.rs`, `src/panel_item_ui.rs`, `protocols/xdg/surface.rs` (`spawn_default_panel_shell`), `protocols/xdg/toplevel.rs` (`switch_panel_shell`)
- Flatland `src/main.rs` (PanelShell / Acceptor), `src/curve.rs` (lab), `src/surfaces.rs`, `src/surface_input.rs`
- Live `~/.local/libexec/stardust_panel_shell` (anti-pattern: app_id special cases)
- `stardust-xr-panels` / `stardust-xr-panels-asteroids` (git StardustXR/panel-item)
