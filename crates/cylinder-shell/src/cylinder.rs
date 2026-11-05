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
    /// Daemon-only slot table (deviation from design sketch: layout needs it; not serialized).
    #[serde(skip)]
    pub slots: Vec<PanelSlot>,
    #[serde(skip)]
    next_id: u64,
}

/// Arc spacing between neighbouring panels (metres of arc).
pub const PANEL_SPACING_M: f32 = 0.02;
pub const MIN_RADIUS_M: f32 = 0.2;
pub const MAX_RADIUS_M: f32 = 5.0;

impl Default for CylinderState {
    fn default() -> Self {
        Self {
            radius_m: 0.70,
            yaw_rad: 0.0,
            height_m: 0.0,
            gap_m: 0.055,
            anchor: CylinderAnchor::HmdAtGrab,
            slots: Vec::new(),
            next_id: 1,
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
    /// Policy: an empty `gap_m` arc stays straight ahead; panels alternate right, left,
    /// right, ... outward from the gap in registration order.
    pub fn assign_slot(&mut self, metrics: &PanelMetrics) -> PanelSlot {
        let id = SlotId(self.next_id.max(1));
        self.next_id = id.0 + 1;
        let (w, h) = metrics.size_m();
        self.slots.push(PanelSlot { id, yaw_rad: 0.0, width_m: w, height_m: h });
        self.relayout();
        self.slot(id).cloned().unwrap()
    }

    pub fn release_slot(&mut self, id: SlotId) {
        self.slots.retain(|s| s.id != id);
        self.relayout();
    }

    /// Update a panel's size (resize / density change) and re-pack.
    pub fn update_metrics(&mut self, id: SlotId, metrics: &PanelMetrics) -> Option<PanelSlot> {
        let (w, h) = metrics.size_m();
        let s = self.slots.iter_mut().find(|s| s.id == id)?;
        s.width_m = w;
        s.height_m = h;
        self.relayout();
        self.slot(id).cloned()
    }

    pub fn slot(&self, id: SlotId) -> Option<&PanelSlot> {
        self.slots.iter().find(|s| s.id == id)
    }

    pub fn apply_group_grab(&mut self, delta_yaw: f32, delta_height: f32) {
        use std::f32::consts::{PI, TAU};
        self.yaw_rad = (self.yaw_rad + delta_yaw + PI).rem_euclid(TAU) - PI;
        self.height_m += delta_height;
    }

    /// Recenter: put the gap straight ahead of `hmd_yaw` at `hmd_height`.
    pub fn reanchor(&mut self, hmd_yaw: f32, hmd_height: f32) {
        self.yaw_rad = 0.0;
        self.apply_group_grab(hmd_yaw, 0.0);
        self.height_m = hmd_height;
    }

    pub fn set_radius(&mut self, radius_m: f32) {
        if radius_m.is_finite() {
            self.radius_m = radius_m.clamp(MIN_RADIUS_M, MAX_RADIUS_M);
            self.relayout();
        }
    }

    /// Arc-length packing -> yaw. Arc widths are radius-independent, so angles scale 1/r.
    fn relayout(&mut self) {
        let r = self.radius_m;
        let (mut right, mut left) = (self.gap_m / 2.0, self.gap_m / 2.0);
        for (i, s) in self.slots.iter_mut().enumerate() {
            let edge = if i % 2 == 0 { &mut right } else { &mut left };
            let centre = *edge + s.width_m / 2.0;
            *edge += s.width_m + PANEL_SPACING_M;
            s.yaw_rad = if i % 2 == 0 { centre / r } else { -centre / r };
        }
    }
}

/// Angular half-width of a slot on radius `r`.
pub fn half_arc_rad(slot: &PanelSlot, r: f32) -> f32 {
    slot.width_m / (2.0 * r)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(w: u32, h: u32) -> PanelMetrics {
        PanelMetrics { size_px: (w, h), density_px_per_m: 2000.0, app_id: None, title: None }
    }

    fn overlaps(a: &PanelSlot, b: &PanelSlot, r: f32) -> bool {
        (a.yaw_rad - b.yaw_rad).abs() < half_arc_rad(a, r) + half_arc_rad(b, r) - 1e-5
    }

    #[test]
    fn two_panels_do_not_overlap_and_leave_gap() {
        let mut c = CylinderState::default();
        let a = c.assign_slot(&m(1280, 720));
        let b = c.assign_slot(&m(1920, 1080));
        assert_ne!(a.id, b.id);
        assert!(!overlaps(&a, &b, c.radius_m));
        let gap_half = c.gap_m / 2.0 / c.radius_m;
        assert!(a.yaw_rad - half_arc_rad(&a, c.radius_m) >= gap_half - 1e-5);
        assert!(b.yaw_rad + half_arc_rad(&b, c.radius_m) <= -gap_half + 1e-5);
        assert!((a.width_m - 0.64).abs() < 1e-5);
    }

    #[test]
    fn many_panels_pairwise_disjoint_across_sizes_and_radii() {
        for r in [0.3, 0.7, 1.5] {
            let mut c = CylinderState::default();
            c.set_radius(r);
            let slots: Vec<_> = (0..6).map(|i| c.assign_slot(&m(400 + 300 * i, 600))).collect();
            let live: Vec<_> = slots.iter().map(|s| c.slot(s.id).unwrap().clone()).collect();
            for i in 0..live.len() {
                for j in i + 1..live.len() {
                    assert!(!overlaps(&live[i], &live[j], c.radius_m), "r={r} {i},{j}");
                }
            }
        }
    }

    #[test]
    fn release_repacks_and_ids_are_unique() {
        let mut c = CylinderState::default();
        let a = c.assign_slot(&m(1000, 500));
        let b = c.assign_slot(&m(1000, 500));
        let d = c.assign_slot(&m(1000, 500));
        c.release_slot(a.id);
        assert_eq!(c.slots.len(), 2);
        // b now first -> right of gap
        assert!(c.slot(b.id).unwrap().yaw_rad > 0.0);
        assert!(c.slot(d.id).unwrap().yaw_rad < 0.0);
        let e = c.assign_slot(&m(10, 10));
        assert!(e.id != a.id && e.id != b.id && e.id != d.id);
    }

    #[test]
    fn resize_repacks_neighbours() {
        let mut c = CylinderState::default();
        let a = c.assign_slot(&m(1000, 500));
        let _b = c.assign_slot(&m(1000, 500));
        let c3 = c.assign_slot(&m(1000, 500));
        let before = c.slot(c3.id).unwrap().yaw_rad;
        c.update_metrics(a.id, &m(3000, 500)).unwrap();
        assert!(c.slot(c3.id).unwrap().yaw_rad > before);
    }

    #[test]
    fn grab_wraps_and_reanchor_resets() {
        let mut c = CylinderState::default();
        c.apply_group_grab(3.0, 0.1);
        c.apply_group_grab(1.0, 0.1);
        assert!(c.yaw_rad > -std::f32::consts::PI && c.yaw_rad <= std::f32::consts::PI);
        assert!((c.yaw_rad - (4.0 - std::f32::consts::TAU)).abs() < 1e-5);
        assert!((c.height_m - 0.2).abs() < 1e-6);
        c.reanchor(0.5, -0.1);
        assert!((c.yaw_rad - 0.5).abs() < 1e-6 && (c.height_m + 0.1).abs() < 1e-6);
    }

    #[test]
    fn radius_clamped_and_yaw_scales() {
        let mut c = CylinderState::default();
        let a = c.assign_slot(&m(1000, 500));
        let y0 = c.slot(a.id).unwrap().yaw_rad;
        c.set_radius(1.4);
        assert!((c.slot(a.id).unwrap().yaw_rad - y0 / 2.0).abs() < 1e-5);
        c.set_radius(0.0);
        assert_eq!(c.radius_m, MIN_RADIUS_M);
        c.set_radius(f32::NAN);
        assert_eq!(c.radius_m, MIN_RADIUS_M);
    }
}
