//! Asteroids `ClientState` for the per-window panel worker (Candidate B).
//!
//! Protocol mirror of Flatland's acceptor / PanelShell / SurfaceModel path — without
//! host-mirror `app_id` branches or Flatland `FLATLAND_CURVE_*` env.
//!
//! Field: Fusion has no cylinder-slice SDF; we attach the chord-AABB `Shape::Box` from
//! [`crate::field_shape`]. Curved pointer UV remapping (`mesh_local_point_to_uv`) is
//! implemented in hit-test but not yet wired into a PointerPlane (headset check).

use crate::field_shape::field_shape_for_slice;
use crate::panel::{PanelWorker, SlotPlacement};
use crate::PanelMetrics;
use glam::{Quat, Vec3};
use gluon_ipc::Node;
use serde::{Deserialize, Serialize};
use stardust_xr_asteroids::{
    ClientState, Context, CustomElement as _, Element, Entity, Migrate, Reify, Tasker, Transformable as _,
    components::Derezzable,
};
use stardust_xr_fusion::types::{Posef, Resource, Size2};
use stardust_xr_panels::panel_item::SurfaceUpdateTarget;
use stardust_xr_panels_asteroids::{
    panel_item_acceptor::PanelItemAcceptor,
    panel_shell::{PanelShell, PanelShellHandler},
    surface_model::SurfaceModel,
};
use std::path::PathBuf;
use std::sync::OnceLock;

/// Values `main` stashes before `run` so `initial_state_update` can pose the panel.
#[derive(Debug, Clone)]
pub struct PendingLaunch {
    pub metrics: PanelMetrics,
    pub placement: SlotPlacement,
    pub mesh_path: PathBuf,
    pub radius_m: f32,
}

static PENDING: OnceLock<PendingLaunch> = OnceLock::new();

/// Install launch context for the next `run::<CylinderShell>`. Panics if called twice.
pub fn install_pending(pending: PendingLaunch) {
    PENDING.set(pending).expect("PendingLaunch already installed");
}

fn take_pending() -> Option<&'static PendingLaunch> {
    PENDING.get()
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
            .build()
            .maybe_child(self.panel_shell(context))
            .maybe_child(self.surface_model())
    }
}

impl CylinderShell {
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
}

/// Re-export placement helpers used when building [`PendingLaunch`].
pub fn placement_pose_parts(position: Vec3, rotation: Quat) -> Posef {
    Posef {
        position: position.into(),
        orientation: rotation.into(),
    }
}
