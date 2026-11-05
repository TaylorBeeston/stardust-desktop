//! Cylinder panel shell for Stardust XR.
//!
//! Design: `docs/cylinder-shell-design.md` (Candidate B: per-window PanelShell worker
//! + `cylinder-layout` daemon). Types and signatures are the contract; fill-in replaces
//! `todo!()` / `not_implemented` bodies.

pub mod cylinder;
pub mod hit_test;
pub mod layout_ipc;
pub mod mesh;
pub mod panel;

pub use cylinder::{CylinderAnchor, CylinderState, PanelMetrics, PanelSlot, SlotId};
pub use hit_test::{CurvedHitTest, CylinderSliceHitTest};
pub use layout_ipc::{default_socket_path, LayoutClient, LayoutEvent, LayoutRequest, SocketLayoutClient};
pub use mesh::bake_curved_panel_glb;
pub use panel::PanelWorker;

/// Scaffold-era error type. Narrow once IPC and XR paths exist.
#[derive(Debug, thiserror::Error)]
pub enum CylinderError {
    #[error("not implemented: {0}")]
    NotImplemented(&'static str),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, CylinderError>;
