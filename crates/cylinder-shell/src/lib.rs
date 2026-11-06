//! Cylinder panel shell for Stardust XR.
//!
//! Design: `docs/cylinder-shell-design.md` (Candidate B: per-window PanelShell worker
//! + `cylinder-layout` daemon).

pub mod cylinder;
pub mod field_shape;
pub mod hit_test;
pub mod layout_ipc;
pub mod curved_pointer;
pub mod mesh;
pub mod mods;
pub mod outline;
pub mod panel;
pub mod pointer_uv;
pub mod pointer_button;
pub mod preflight;
pub mod xr;

pub use cylinder::{CylinderAnchor, CylinderState, PanelMetrics, PanelSlot, SlotId, MIN_RADIUS_M, MAX_RADIUS_M};
pub use field_shape::{field_shape_for_slice, slice_aabb_extents};
pub use hit_test::{CurvedHitTest, CylinderSliceHitTest};
pub use layout_ipc::{
    apply_layout_event, default_socket_path, LayoutClient, LayoutDaemon, LayoutEvent,
    LayoutRequest, LayoutRequestSender, LayoutSession, LayoutSessionClosed, SocketLayoutClient,
};
pub use mesh::{bake_curved_panel_glb, bake_curved_panel_glb_in};
pub use mods::ModTracker;
pub use pointer_uv::{cursor_mesh_transform, mesh_local_point_hit, mesh_local_ray_hit, uv_to_mesh_local, uv_to_surface_px, SliceHit};
pub use panel::{PanelWorker, SlotPlacement};
pub use preflight::{
    parse_protected_panel_env, preflight_xr, provider_socket_path, server_socket_path, PreflightError,
};
pub use xr::{install_layout_session, install_pending, CylinderShell, PendingLaunch};

/// Scaffold-era error type. Narrow once IPC and XR paths exist.
#[derive(Debug, thiserror::Error)]
pub enum CylinderError {
    #[error("not implemented: {0}")]
    NotImplemented(&'static str),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, CylinderError>;
