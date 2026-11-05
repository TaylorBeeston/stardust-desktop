//! Shared cylinder group state. Owned by `cylinder-layout`; workers hold snapshots.

use serde::{Deserialize, Serialize};

/// Opaque slot handle assigned by the layout daemon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SlotId(pub u64);

/// Where the cylinder axis is anchored in stage space.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum CylinderAnchor {
    /// Snap upright to HMD when the first panel (or a recenter) grabs.
    HmdAtGrab,
    /// Fixed stage-space point (x, y, z metres).
    Stage { x: f32, y: f32, z: f32 },
}

/// Shared cylinder group state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CylinderState {
    /// Cylinder radius in metres.
    pub radius_m: f32,
    /// Group yaw about vertical (radians, + = right).
    pub yaw_rad: f32,
    /// Vertical offset of the panel belt (metres).
    pub height_m: f32,
    /// Empty arc ahead of the user (metres of arc at `radius_m`).
    pub gap_m: f32,
    pub anchor: CylinderAnchor,
}

impl Default for CylinderState {
    fn default() -> Self {
        Self {
            radius_m: 0.70,
            yaw_rad: 0.0,
            height_m: 0.0,
            gap_m: 0.055,
            anchor: CylinderAnchor::HmdAtGrab,
        }
    }
}

/// One panel's place on the cylinder.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PanelSlot {
    pub id: SlotId,
    /// Centre angle on the cylinder (radians, + = right), relative to group yaw.
    pub yaw_rad: f32,
    pub width_m: f32,
    pub height_m: f32,
}

/// Metrics a worker reports when registering or on resize.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PanelMetrics {
    pub size_px: (u32, u32),
    pub density_px_per_m: f32,
    pub app_id: Option<String>,
    pub title: Option<String>,
}

impl PanelMetrics {
    pub fn size_m(&self) -> (f32, f32) {
        let d = self.density_px_per_m.max(1.0);
        (self.size_px.0 as f32 / d, self.size_px.1 as f32 / d)
    }
}

impl CylinderState {
    /// Place a new panel into the tiling layout. Daemon-owned.
    pub fn assign_slot(&mut self, metrics: &PanelMetrics) -> PanelSlot {
        let _ = metrics;
        // Layout algorithm (gap-ahead, left-to-right pack) lands in fill-in.
        todo!("CylinderState::assign_slot")
    }

    pub fn release_slot(&mut self, id: SlotId) {
        let _ = id;
        todo!("CylinderState::release_slot")
    }

    pub fn apply_group_grab(&mut self, delta_yaw: f32, delta_height: f32) {
        let _ = (delta_yaw, delta_height);
        todo!("CylinderState::apply_group_grab")
    }

    pub fn set_radius(&mut self, radius_m: f32) {
        let _ = radius_m;
        todo!("CylinderState::set_radius")
    }
}
