//! Asteroids `ClientState` for the per-window panel worker (Candidate B).
//!
//! Protocol mirror of Flatland's acceptor / PanelShell / SurfaceModel path — without
//! host-mirror `app_id` branches or Flatland `FLATLAND_CURVE_*` env.
//!
//! Field: Fusion has no cylinder-slice SDF; we attach the chord-AABB `Shape::Box` from
//! [`crate::field_shape`]. Pointer / hand input is wired through
//! [`crate::curved_pointer::CurvedPointerPlane`]: the chord-AABB Field only gates capture;
//! the landing texel comes from the true arc (`pointer_uv` / `mesh_local_point_to_uv`).

use crate::curved_pointer::CurvedPointerPlane;
use crate::field_shape::field_shape_for_slice;
use crate::mods::ModTracker;
use crate::outline::{curved_panel_border, FOCUS_OUTLINE_TIMEOUT};
use crate::hit_test::CylinderSliceHitTest;
use crate::layout_ipc::{apply_layout_event, LayoutEvent, LayoutRequestSender, LayoutSession};
use crate::mesh::bake_curved_panel_glb;
use crate::pointer_uv::uv_to_surface_px;
use crate::panel::{PanelWorker, SlotPlacement};
use crate::PanelMetrics;
use glam::{Quat, Vec3};
use gluon_ipc::Node;
use serde::{Deserialize, Serialize};
use stardust_xr_asteroids::{
    ClientState, Context, CustomElement as _, Element, Entity, Migrate, Reify, Tasker, Transformable as _,
    components::{Derezzable, KeyboardHandler, MouseHandler, ScrollSource as AsteroidsScrollSource},
    elements::Lines,
};
use stardust_xr_fusion::types::{Posef, Resource, Size2};
use stardust_xr_molecules::lines::LineExt as _;
use stardust_xr_panels::panel_item::{ScrollSource, SurfaceId, SurfaceUpdateTarget};
use stardust_xr_panels_asteroids::{
    panel_item_acceptor::PanelItemAcceptor,
    panel_shell::{PanelShell, PanelShellHandler},
    surface_model::SurfaceModel,
};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// Values `main` stashes before `run` so `initial_state_update` can pose the panel.
#[derive(Debug, Clone)]
pub struct PendingLaunch {
    pub metrics: PanelMetrics,
    pub placement: SlotPlacement,
    pub mesh_path: PathBuf,
    pub radius_m: f32,
}

static PENDING: OnceLock<PendingLaunch> = OnceLock::new();
static LAYOUT: Mutex<Option<LayoutSession>> = Mutex::new(None);

/// Install launch context for the next `run::<CylinderShell>`. Panics if called twice.
pub fn install_pending(pending: PendingLaunch) {
    PENDING.set(pending).expect("PendingLaunch already installed");
}

/// Hand the live layout socket to the XR client (kept open for GroupChanged / GroupGrab).
pub fn install_layout_session(session: LayoutSession) {
    *LAYOUT.lock().expect("layout mutex") = Some(session);
}

fn take_pending() -> Option<&'static PendingLaunch> {
    PENDING.get()
}

fn take_layout_session() -> Option<LayoutSession> {
    LAYOUT.lock().expect("layout mutex").take()
}

fn pose_from_placement(p: &SlotPlacement) -> Posef {
    Posef {
        position: p.position.into(),
        orientation: p.rotation.into(),
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CylinderShell {
    pose: Posef,
    width_m: f32,
    height_m: f32,
    radius_m: f32,
    size_px: (u32, u32),
    mesh_path: Option<String>,
    #[serde(skip)]
    shell: Option<Node<PanelShellHandler>>,
    #[serde(skip)]
    worker: PanelWorker,
    /// Sends GroupGrab / SetRadius / Reanchor while the layout thread owns the socket.
    #[serde(skip)]
    layout_tx: Option<LayoutRequestSender>,
    /// Last pointer position (surface px) — relative mouse motion (manifold) accumulates here.
    #[serde(skip)]
    cursor_px: [f32; 2],
    /// Seat-local mods (see [`ModTracker`]) — do not forward manifold's global xkb mask.
    #[serde(skip)]
    mod_tracker: ModTracker,
    /// Last manifold keyboard/mouse event (for focus outline timeout).
    #[serde(skip)]
    last_input_at: Option<Instant>,
}

/// Apply a relative mouse delta to the cursor, clamped to the surface. Pure — unit-tested.
pub fn step_cursor(pos: [f32; 2], delta: [f32; 2], size_px: (u32, u32)) -> [f32; 2] {
    let (w, h) = ((size_px.0 as f32 - 1.0).max(0.0), (size_px.1 as f32 - 1.0).max(0.0));
    [(pos[0] + delta[0]).clamp(0.0, w), (pos[1] + delta[1]).clamp(0.0, h)]
}

fn scroll_source(s: AsteroidsScrollSource) -> ScrollSource {
    match s {
        AsteroidsScrollSource::Wheel => ScrollSource::Wheel,
        AsteroidsScrollSource::Finger => ScrollSource::Touch,
        AsteroidsScrollSource::Continuous => ScrollSource::Continuous,
        AsteroidsScrollSource::WheelTilt => ScrollSource::WheelTilt,
    }
}

impl Default for CylinderShell {
    fn default() -> Self {
        Self {
            pose: Posef::default(),
            width_m: 0.4,
            height_m: 0.25,
            radius_m: 0.7,
            size_px: (1280, 720),
            mesh_path: None,
            shell: None,
            worker: PanelWorker::new(PanelMetrics {
                size_px: (1280, 720),
                density_px_per_m: 3000.0,
                app_id: None,
                title: None,
            }),
            layout_tx: None,
            cursor_px: [0.0, 0.0],
            mod_tracker: ModTracker::new(),
            last_input_at: None,
        }
    }
}

impl Migrate for CylinderShell {
    type Old = Self;
}

impl ClientState for CylinderShell {
    const APP_ID: &'static str = "org.stardustxr.CylinderShell";

    fn initial_state_update(&mut self) {
        let Some(pending) = take_pending() else {
            tracing::warn!("CylinderShell started without PendingLaunch; using defaults");
            return;
        };
        self.pose = pose_from_placement(&pending.placement);
        self.width_m = pending.placement.hit_test.width_m;
        self.height_m = pending.placement.hit_test.height_m;
        self.radius_m = pending.radius_m;
        self.size_px = pending.metrics.size_px;
        self.mesh_path = Some(pending.mesh_path.to_string_lossy().into_owned());
        self.worker = PanelWorker::new(pending.metrics.clone());
        // slot already applied by main; keep placement for hit-test ownership
        self.worker.placement = Some(pending.placement);
        tracing::info!(
            mesh = ?self.mesh_path,
            pos = ?self.pose.position,
            "CylinderShell pending launch applied"
        );
    }

    fn on_start(&mut self, _context: &Context, tasks: impl Tasker<Self>) {
        let Some(session) = take_layout_session() else {
            tracing::info!("CylinderShell: no layout session (daemon absent or NoServer path)");
            return;
        };
        self.layout_tx = Some(session.request_sender());
        tracing::info!(slot = ?session.slot_id(), "layout IPC kept open for XR lifetime");
        Self::pump_layout(tasks, session);
    }
}

impl Reify for CylinderShell {
    fn reify(
        &self,
        context: &Context,
        _tasks: impl Tasker<Self>,
        _props: (),
    ) -> impl Element<Self> {
        let field = field_shape_for_slice(self.width_m, self.height_m, self.radius_m);
        Entity::new(field)
            .pose(self.pose)
            .component(Derezzable::program_stopper(context))
            .component(PanelItemAcceptor::<Self>::new(|state, shell, from_env| {
                tracing::info!(from_env, "PanelItem accepted");
                let size = Size2 {
                    x: state.size_px.0,
                    y: state.size_px.1,
                };
                let _ = shell.item().request_toplevel_resize(size);
                state.worker.on_accept_panel(&shell);
                state.shell = Some(shell);
            }))
            // Keyboard + mouse handlers on the panel Field: this is what manifold (and any
            // Stardust keyboard/mouse input method) targets — same as Flatland's surfaces.rs.
            .component(self.shell.is_some().then(|| self.keyboard_handler()))
            .component(self.shell.is_some().then(|| self.mouse_handler()))
            .build()
            .maybe_child(self.pointer_plane())
            .maybe_child(self.focus_outline())
            .maybe_child(self.panel_shell(context))
            .maybe_child(self.surface_model())
    }
}

impl CylinderShell {
    /// Mesh-local hit-test for this panel (yaw is irrelevant in mesh-local frame).
    pub fn slice_hit_test(&self) -> CylinderSliceHitTest {
        CylinderSliceHitTest { radius_m: self.radius_m, yaw_rad: 0.0, width_m: self.width_m, height_m: self.height_m }
    }

    fn keyboard_handler(&self) -> KeyboardHandler<Self> {
        // Use on_key (frame-polled) so we can update seat-local ModTracker on &mut Self.
        // Forwarding manifold's global depressed/locked mask verbatim made ghostty treat
        // Shift as held whenever that mask was stale (Flatland host-mirror avoids this via key_sink).
        KeyboardHandler::<Self>::new().on_key(move |state, key, timestamp| {
            state.last_input_at = Some(Instant::now());
            let mods = state
                .mod_tracker
                .apply(key.keycode, key.pressed, key.modifiers.layout_group);
            if key.modifiers.depressed != mods.depressed || key.modifiers.locked != mods.locked {
                tracing::info!(
                    keycode = key.keycode,
                    pressed = key.pressed,
                    remote_dep = key.modifiers.depressed,
                    local_dep = mods.depressed,
                    remote_lock = key.modifiers.locked,
                    local_lock = mods.locked,
                    "key mods: seat-local overrides remote mask"
                );
            } else {
                tracing::debug!(keycode = key.keycode, pressed = key.pressed, dep = mods.depressed, "key");
            }
            let Some(shell) = &state.shell else { return };
            let _ = shell.item().key(
                SurfaceId::Toplevel,
                key.keycode,
                key.pressed,
                mods,
                key.keymap,
                timestamp,
            );
        })
    }

    fn mouse_handler(&self) -> MouseHandler<Self> {
        let item = self.shell.as_ref().map(|s| s.item().clone());
        MouseHandler::<Self>::new()
            .on_button_async({
                let item = item.clone();
                move |button, pressed, timestamp| {
                    tracing::debug!(button, pressed, "mouse button");
                    if let Some(item) = &item {
                        let _ = item.pointer_button(SurfaceId::Toplevel, button, pressed, timestamp);
                    }
                }
            })
            .on_motion(|state: &mut Self, motion, timestamp| {
                state.last_input_at = Some(Instant::now());
                let Some(shell) = &state.shell else { return };
                let d = [motion.x, -motion.y];
                state.cursor_px = step_cursor(state.cursor_px, d, state.size_px);
                let _ = shell.item().pointer_motion(SurfaceId::Toplevel, Some(d.into()), state.cursor_px.into(), timestamp);
            })
            .on_scroll_discrete_async({
                let item = item.clone();
                move |v, source, timestamp| {
                    if let Some(item) = &item {
                        let _ = item.pointer_scroll_discrete(SurfaceId::Toplevel, [v.x, -v.y].into(), scroll_source(source), timestamp);
                    }
                }
            })
            .on_scroll_continuous_async(move |v, source, timestamp| {
                if let Some(item) = &item {
                    let _ = item.pointer_scroll_pixels(SurfaceId::Toplevel, [v.x, -v.y].into(), scroll_source(source), timestamp);
                }
            })
    }


    /// Thin curved border while manifold keyboard/mouse is targeting this panel.
    fn focus_outline(&self) -> Option<impl Element<Self> + use<>> {
        let at = self.last_input_at?;
        if at.elapsed() > FOCUS_OUTLINE_TIMEOUT {
            return None;
        }
        let line = curved_panel_border(self.width_m, self.height_m, self.radius_m)
            .thickness(0.0012)
            .color(stardust_xr_fusion::types::rgba_linear!(0.55, 0.75, 1.0, 0.35));
        Some(Lines::new([line]).build())
    }

    /// Curved pointer input; only once a PanelItem is accepted (nothing to send to before).
    fn pointer_plane(&self) -> Option<impl Element<Self> + use<>> {
        self.shell.as_ref()?;
        Some(
            CurvedPointerPlane::new(self.slice_hit_test())
                .on_motion(|state: &mut Self, uv| {
                    let Some(shell) = &state.shell else { return };
                    let [x, y] = uv_to_surface_px(uv, state.size_px);
                    state.cursor_px = [x, y];
                    let _ = shell.item().pointer_motion(SurfaceId::Toplevel, None, [x, y].into(), None);
                })
                .on_button(|state: &mut Self, button, pressed| {
                    let Some(shell) = &state.shell else { return };
                    tracing::info!(button, pressed, "xr pointer button");
                    let _ = shell.item().pointer_button(SurfaceId::Toplevel, button, pressed, None);
                })
                .on_scroll(|state: &mut Self, delta| {
                    let Some(shell) = &state.shell else { return };
                    let _ = shell.item().pointer_scroll_discrete(SurfaceId::Toplevel, delta.into(), ScrollSource::Wheel, None);
                })
                .build(),
        )
    }

    fn panel_shell(&self, context: &Context) -> Option<impl Element<Self> + use<>> {
        let shell = self.shell.as_ref()?;
        let context = context.clone();
        Some(
            PanelShell::new(shell, move |state: &mut Self| {
                tracing::info!("panel shell disconnected");
                state.shell = None;
                context.stop();
            })
            .on_toplevel_resolution_changed(|state, _, size| {
                state.size_px = (size.x, size.y);
                tracing::info!(x = size.x, y = size.y, "toplevel resolution");
            })
            .on_toplevel_app_id_changed(|state, _, app_id| {
                state.worker.metrics.app_id = Some(app_id);
            })
            .on_toplevel_title_changed(|state, _, title| {
                state.worker.metrics.title = Some(title);
            })
            .build(),
        )
    }

    fn surface_model(&self) -> Option<impl Element<Self> + use<>> {
        let shell = self.shell.as_ref()?;
        let path = self.mesh_path.clone()?;
        Some(
            SurfaceModel::new(
                shell,
                SurfaceUpdateTarget::Toplevel,
                Resource::Direct { path },
                "Panel",
            )
            .build(),
        )
    }

    /// Recursively await layout events and re-pose / remesh on `GroupChanged`.
    fn pump_layout(tasks: impl Tasker<Self>, mut session: LayoutSession) {
        tasks.spawn(
            async move {
                let ev = session.recv().await;
                (session, ev)
            },
            |tasks, state, (session, ev)| {
                let Some(ev) = ev else {
                    tracing::info!("layout IPC ended");
                    state.layout_tx = None;
                    return;
                };
                state.apply_layout_event_xr(ev);
                Self::pump_layout(tasks, session);
            },
        );
    }

    /// Apply a daemon event to pose / metrics / mesh. Public for unit tests.
    pub fn apply_layout_event_xr(&mut self, ev: LayoutEvent) {
        let Some(placement) = apply_layout_event(&mut self.worker, &ev) else {
            tracing::info!("layout Shutdown");
            return;
        };
        let (slot, group) = match &ev {
            LayoutEvent::Assigned { slot, group } | LayoutEvent::GroupChanged { slot, group } => {
                (slot, group)
            }
            LayoutEvent::Shutdown => return,
        };
        let old_r = self.radius_m;
        let old_w = self.width_m;
        let old_h = self.height_m;
        self.pose = pose_from_placement(&placement);
        self.width_m = placement.hit_test.width_m;
        self.height_m = placement.hit_test.height_m;
        self.radius_m = group.radius_m;
        let geom_changed = (old_r - self.radius_m).abs() > 1e-4
            || (old_w - self.width_m).abs() > 1e-4
            || (old_h - self.height_m).abs() > 1e-4;
        if geom_changed {
            match bake_curved_panel_glb(self.width_m, self.height_m, self.radius_m) {
                Ok(path) => {
                    tracing::info!(?path, id = slot.id.0, "rebaked mesh after layout update");
                    self.mesh_path = Some(path.to_string_lossy().into_owned());
                }
                Err(e) => tracing::error!(%e, "mesh rebake failed after layout update"),
            }
        }
        tracing::info!(
            id = slot.id.0,
            yaw = group.yaw_rad,
            height = group.height_m,
            radius = group.radius_m,
            pos = ?self.pose.position,
            "applied layout GroupChanged"
        );
    }

    /// XR / bead path: request a group yaw/height delta (no-op if layout session gone).
    pub fn send_group_grab(&self, delta_yaw: f32, delta_height: f32) {
        if let Some(tx) = &self.layout_tx {
            if let Err(e) = tx.group_grab(delta_yaw, delta_height) {
                tracing::warn!(%e, "GroupGrab send failed");
            }
        }
    }

    pub fn send_set_radius(&self, radius_m: f32) {
        if let Some(tx) = &self.layout_tx {
            if let Err(e) = tx.set_radius(radius_m) {
                tracing::warn!(%e, "SetRadius send failed");
            }
        }
    }

    pub fn send_reanchor(&self, hmd_yaw: f32, hmd_height: f32) {
        if let Some(tx) = &self.layout_tx {
            if let Err(e) = tx.reanchor(hmd_yaw, hmd_height) {
                tracing::warn!(%e, "Reanchor send failed");
            }
        }
    }
}

/// Re-export placement helpers used when building [`PendingLaunch`].
pub fn placement_pose_parts(position: Vec3, rotation: Quat) -> Posef {
    Posef {
        position: position.into(),
        orientation: rotation.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cylinder::{CylinderState, PanelMetrics};
    use crate::layout_ipc::LayoutEvent;

    fn metrics() -> PanelMetrics {
        PanelMetrics {
            size_px: (1280, 720),
            density_px_per_m: 2000.0,
            app_id: Some("test".into()),
            title: None,
        }
    }

    #[test]
    fn step_cursor_accumulates_and_clamps() {
        assert_eq!(step_cursor([10.0, 10.0], [5.0, -3.0], (100, 50)), [15.0, 7.0]);
        assert_eq!(step_cursor([98.0, 1.0], [10.0, -10.0], (100, 50)), [99.0, 0.0]);
    }

    #[test]
    fn apply_layout_event_xr_reposes_and_tracks_radius() {
        let mut g = CylinderState::default();
        let slot = g.assign_slot(&metrics());
        let mut shell = CylinderShell::default();
        shell.worker = PanelWorker::new(metrics());
        shell.apply_layout_event_xr(LayoutEvent::Assigned {
            slot: slot.clone(),
            group: g.clone(),
        });
        assert!((shell.radius_m - 0.7).abs() < 1e-6);
        let y0 = f32::from(shell.pose.position.y);

        g.apply_group_grab(0.4, 0.12);
        g.set_radius(1.1);
        let slot2 = g.slot(slot.id).unwrap().clone();
        shell.apply_layout_event_xr(LayoutEvent::GroupChanged {
            slot: slot2,
            group: g.clone(),
        });
        let y1 = f32::from(shell.pose.position.y);
        assert!((y1 - 0.12).abs() < 1e-5);
        assert!((shell.radius_m - 1.1).abs() < 1e-6);
        assert!((y1 - y0 - 0.12).abs() < 1e-4);
        assert!(shell.mesh_path.is_some(), "rebake after radius change");
        assert!(shell.worker.placement.is_some());
    }
}
