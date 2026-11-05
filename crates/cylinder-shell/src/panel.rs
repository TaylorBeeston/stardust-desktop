//! Per-window panel worker state (one `PanelItem` per process — Candidate B).

use crate::cylinder::{CylinderState, PanelMetrics, PanelSlot};
use crate::hit_test::CylinderSliceHitTest;
use glam::{Quat, Vec3};
use gluon_ipc::Node;
use stardust_xr_panels_asteroids::panel_shell::PanelShellHandler;

/// Where a worker should put its panel, in the cylinder-local frame
/// (axis +Y through the anchor, forward -Z, yaw + = right).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlotPlacement {
    /// Panel centre.
    pub position: Vec3,
    /// Rotates the panel's +Z (front normal) to point back at the axis.
    pub rotation: Quat,
    /// Hit-test in the same frame, with belt height already subtracted by `ray_local`.
    pub hit_test: CylinderSliceHitTest,
    /// Absolute centre azimuth (group yaw + slot yaw).
    pub azimuth_rad: f32,
}

impl SlotPlacement {
    pub fn compute(slot: &PanelSlot, group: &CylinderState) -> Self {
        let az = group.yaw_rad + slot.yaw_rad;
        let r = group.radius_m;
        Self {
            position: Vec3::new(r * az.sin(), group.height_m, -r * az.cos()),
            rotation: Quat::from_rotation_y(-az),
            hit_test: CylinderSliceHitTest { radius_m: r, yaw_rad: az, width_m: slot.width_m, height_m: slot.height_m },
            azimuth_rad: az,
        }
    }

    /// Convert a cylinder-frame ray origin into the hit-test's frame (belt at y = 0).
    pub fn ray_local(&self, origin: Vec3, group: &CylinderState) -> [f32; 3] {
        (origin - Vec3::Y * group.height_m).to_array()
    }
}

/// Worker-side bookkeeping. The Asteroids `ClientState` in `main` holds the live
/// `PanelShell` / `PanelItem`; this struct tracks layout membership.
#[derive(Debug, Clone)]
pub struct PanelWorker {
    pub slot: Option<PanelSlot>,
    pub metrics: PanelMetrics,
    pub placement: Option<SlotPlacement>,
    pub accepted: bool,
}

impl PanelWorker {
    pub fn new(metrics: PanelMetrics) -> Self {
        Self { slot: None, metrics, placement: None, accepted: false }
    }

    /// Called when `PanelItemAcceptor` delivers a shell (via `SDXR_PROTECTED_PANEL`).
    /// Live callback wiring (resolution, app_id, children) needs a running server and
    /// lives in the binary; see design.md "Implementation status".
    pub fn on_accept_panel(&mut self, _shell: &Node<PanelShellHandler>) {
        self.accepted = true;
    }

    /// Apply a daemon-assigned slot and current group transform. Returns the placement
    /// the XR side should pose the Spatial / mesh / Field at.
    pub fn apply_slot(&mut self, slot: PanelSlot, group: &CylinderState) -> SlotPlacement {
        let p = SlotPlacement::compute(&slot, group);
        self.slot = Some(slot);
        self.placement = Some(p);
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hit_test::CurvedHitTest;

    #[test]
    fn placement_sits_on_cylinder_faces_axis_and_hit_tests_centre() {
        let mut g = CylinderState::default();
        g.apply_group_grab(0.2, 0.15);
        let m = PanelMetrics { size_px: (1280, 720), density_px_per_m: 2000.0, app_id: None, title: None };
        let slot = g.assign_slot(&m);
        let mut w = PanelWorker::new(m);
        let p = w.apply_slot(slot, &g);
        assert!((Vec3::new(p.position.x, 0.0, p.position.z).length() - g.radius_m).abs() < 1e-5);
        assert!((p.position.y - 0.15).abs() < 1e-6);
        let front = p.rotation * Vec3::Z;
        let to_axis = -Vec3::new(p.position.x, 0.0, p.position.z).normalize();
        assert!(front.dot(to_axis) > 0.9999);
        // head at the anchor at belt height looking at the panel centre
        let head = Vec3::new(0.0, 0.15, 0.0);
        let uv = p.hit_test.ray_to_surface_uv(p.ray_local(head, &g), (p.position - head).to_array()).unwrap();
        assert!((uv.0 - 0.5).abs() < 1e-4 && (uv.1 - 0.5).abs() < 1e-4);
    }
}
