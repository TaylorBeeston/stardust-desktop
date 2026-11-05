//! Per-window panel worker state (one `PanelItem` per process — Candidate B).

use crate::cylinder::{CylinderState, PanelMetrics, PanelSlot};
use gluon_ipc::Node;
use stardust_xr_panels_asteroids::panel_shell::PanelShellHandler;

/// Worker-side bookkeeping. The Asteroids `ClientState` in `main` holds the live
/// `PanelShell` / `PanelItem`; this struct tracks layout membership.
#[derive(Debug, Clone)]
pub struct PanelWorker {
    pub slot: Option<PanelSlot>,
    pub metrics: PanelMetrics,
}

impl PanelWorker {
    pub fn new(metrics: PanelMetrics) -> Self {
        Self {
            slot: None,
            metrics,
        }
    }

    /// Called when `PanelItemAcceptor` delivers a shell (via `SDXR_PROTECTED_PANEL`).
    pub fn on_accept_panel(&mut self, _shell: &Node<PanelShellHandler>) {
        // Wire PanelShell callbacks (resolution, app_id, children) in the binary.
        todo!("PanelWorker::on_accept_panel")
    }

    /// Apply a daemon-assigned slot and current group transform.
    pub fn apply_slot(&mut self, slot: PanelSlot, group: &CylinderState) {
        let _ = group;
        self.slot = Some(slot);
        todo!("PanelWorker::apply_slot — pose Spatial + bake mesh + refresh hit-test")
    }
}
