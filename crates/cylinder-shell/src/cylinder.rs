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
    /// Layout / mesh width (metres). May be scaled down by [`CylinderState::relayout`]
    /// when the side would wrap past ±π; restored when radius grows again.
    pub width_m: f32,
    pub height_m: f32,
    /// Unscaled width from the worker's metrics. Packing always starts from this so a
    /// later larger radius can undo a previous overflow shrink. Not sent over IPC.
    #[serde(skip)]
    natural_width_m: f32,
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
        self.slots.push(PanelSlot {
            id,
            yaw_rad: 0.0,
            width_m: w,
            height_m: h,
            natural_width_m: w,
        });
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
        s.natural_width_m = w;
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

    /// Swap two panels' positions in registration order, then re-pack.
    pub fn swap_slots(&mut self, a: SlotId, b: SlotId) -> bool {
        let ia = self.slots.iter().position(|s| s.id == a);
        let ib = self.slots.iter().position(|s| s.id == b);
        match (ia, ib) {
            (Some(i), Some(j)) if i != j => {
                self.slots.swap(i, j);
                self.relayout();
                true
            }
            _ => false,
        }
    }

    /// Move `id` to `index` in registration order (clamped), then re-pack.
    pub fn reorder_slot(&mut self, id: SlotId, index: usize) -> bool {
        let Some(from) = self.slots.iter().position(|s| s.id == id) else {
            return false;
        };
        let slot = self.slots.remove(from);
        let to = index.min(self.slots.len());
        self.slots.insert(to, slot);
        self.relayout();
        true
    }

    /// Index of a slot in registration order.
    pub fn slot_index(&self, id: SlotId) -> Option<usize> {
        self.slots.iter().position(|s| s.id == id)
    }

    /// Move `id` by `delta` slots in registration order (clamped).
    pub fn shift_slot(&mut self, id: SlotId, delta: i32) -> bool {
        let Some(from) = self.slot_index(id) else {
            return false;
        };
        if delta == 0 {
            return false;
        }
        let dest = (from as i32 + delta).clamp(0, self.slots.len().saturating_sub(1) as i32) as usize;
        if dest == from {
            return false;
        }
        self.reorder_slot(id, dest)
    }

    /// Place `id` at an absolute yaw while dragging: reorder by yaw, re-pack
    /// neighbours so a landing gap opens, then keep the floating panel at the
    /// drag yaw (hand follow). Settle snaps it into the packed seat.
    pub fn slide_slot(&mut self, id: SlotId, yaw_rad: f32) -> Option<PanelSlot> {
        if !yaw_rad.is_finite() {
            return self.slot(id).cloned();
        }
        let from = self.slot_index(id)?;
        self.slots[from].yaw_rad = yaw_rad;
        let dest = self
            .slots
            .iter()
            .filter(|s| s.id != id && s.yaw_rad < yaw_rad)
            .count();
        if from != dest {
            let slot = self.slots.remove(from);
            let insert_at = dest.min(self.slots.len());
            self.slots.insert(insert_at, slot);
        }
        self.relayout();
        // Neighbours keep packed seats (gap at insertion index); floater follows hand.
        if let Some(s) = self.slots.iter_mut().find(|s| s.id == id) {
            s.yaw_rad = yaw_rad;
        }
        self.slot(id).cloned()
    }

    /// After a slide: reorder so `id` sits among neighbours by current yaw, then re-pack.
    pub fn settle_slot_by_yaw(&mut self, id: SlotId) -> bool {
        let Some(yaw) = self.slot(id).map(|s| s.yaw_rad) else {
            return false;
        };
        let idx = self
            .slots
            .iter()
            .filter(|s| s.id != id && s.yaw_rad < yaw)
            .count();
        self.reorder_slot(id, idx)
    }

    /// Adjacent panel in registration order.
    pub fn neighbour(&self, id: SlotId, toward_higher_index: bool) -> Option<SlotId> {
        let i = self.slot_index(id)?;
        let j = if toward_higher_index {
            i.checked_add(1)?
        } else {
            i.checked_sub(1)?
        };
        self.slots.get(j).map(|s| s.id)
    }

    pub fn set_radius(&mut self, radius_m: f32) {
        if radius_m.is_finite() {
            self.radius_m = radius_m.clamp(MIN_RADIUS_M, MAX_RADIUS_M);
            self.relayout();
        }
    }

    /// Arc-length packing -> yaw. Arc widths are radius-independent, so angles scale 1/r.
    ///
    /// If a side's panels would wrap past the back (±π from the gap), that side's widths
    /// are uniformly scaled down so the outer edge lands on the back seam. Panels are
    /// never dropped. Scales are computed from [`PanelSlot::natural_width_m`] so raising
    /// the radius later restores full size.
    fn relayout(&mut self) {
        use std::f32::consts::PI;
        let r = self.radius_m.max(1e-4);
        // Available arc length per side: gap edge → back midline.
        let max_side_arc = (PI * r - self.gap_m * 0.5).max(0.0);

        let side_extent = |idxs: &[usize]| -> f32 {
            if idxs.is_empty() {
                return 0.0;
            }
            let mut sum = 0.0;
            for (n, &i) in idxs.iter().enumerate() {
                let w = self.slots[i].natural_width_m.max(self.slots[i].width_m);
                sum += w;
                if n + 1 < idxs.len() {
                    sum += PANEL_SPACING_M;
                }
            }
            sum
        };
        let right_idxs: Vec<usize> = (0..self.slots.len()).filter(|i| i % 2 == 0).collect();
        let left_idxs: Vec<usize> = (0..self.slots.len()).filter(|i| i % 2 == 1).collect();
        let re = side_extent(&right_idxs);
        let le = side_extent(&left_idxs);
        let right_scale = if re > max_side_arc && re > 1e-6 {
            max_side_arc / re
        } else {
            1.0
        };
        let left_scale = if le > max_side_arc && le > 1e-6 {
            max_side_arc / le
        } else {
            1.0
        };

        let (mut right, mut left) = (self.gap_m / 2.0, self.gap_m / 2.0);
        for (i, s) in self.slots.iter_mut().enumerate() {
            if s.natural_width_m <= 0.0 {
                s.natural_width_m = s.width_m;
            }
            let scale = if i % 2 == 0 { right_scale } else { left_scale };
            let w = s.natural_width_m * scale;
            s.width_m = w;
            let edge = if i % 2 == 0 { &mut right } else { &mut left };
            let centre = *edge + w / 2.0;
            *edge += w + PANEL_SPACING_M;
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
    fn slide_opens_gap_for_neighbours() {
        let mut c = CylinderState::default();
        let a = c.assign_slot(&m(1000, 500));
        let b = c.assign_slot(&m(1000, 500));
        let d = c.assign_slot(&m(1000, 500));
        let b_yaw0 = c.slot(b.id).unwrap().yaw_rad;
        // Drag `a` past `b` toward the far left (more negative than d).
        let target = c.slot(d.id).unwrap().yaw_rad - 0.5;
        c.slide_slot(a.id, target);
        assert!((c.slot(a.id).unwrap().yaw_rad - target).abs() < 1e-5);
        // Neighbours re-packed: b should have moved from its original seat.
        let b_yaw1 = c.slot(b.id).unwrap().yaw_rad;
        assert!(
            (b_yaw1 - b_yaw0).abs() > 1e-4,
            "expected neighbour to slide for gap, b {b_yaw0} → {b_yaw1}"
        );
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

    #[test]
    fn overflow_scales_width_to_fit_half_cylinder_never_drops() {
        // Flatland Meta-0 alone at r=0.45 needs ~1.66 m of arc; half-circ ≈ 1.41 m → must shrink.
        let mut c = CylinderState::default();
        c.gap_m = 0.055;
        c.set_radius(0.45);
        // 3440/2070 ≈ 1.662 m wide (Flatland Meta density), one panel on the right.
        let meta = PanelMetrics {
            size_px: (3440, 1440),
            density_px_per_m: 2070.0,
            app_id: Some("host-mirror-meta-0".into()),
            title: None,
        };
        let edp = PanelMetrics {
            size_px: (2560, 1600),
            density_px_per_m: 2300.0,
            app_id: Some("host-mirror-edp-1".into()),
            title: None,
        };
        let a_id = c.assign_slot(&meta).id; // right
        let b_id = c.assign_slot(&edp).id; // left
        assert_eq!(c.slots.len(), 2);
        let a = c.slot(a_id).unwrap().clone();
        let b = c.slot(b_id).unwrap().clone();
        assert!(a.yaw_rad > 0.0 && b.yaw_rad < 0.0);
        // Outer edges must stay within ±π (back seam).
        assert!(a.yaw_rad + half_arc_rad(&a, c.radius_m) <= std::f32::consts::PI + 1e-3);
        assert!(b.yaw_rad.abs() + half_arc_rad(&b, c.radius_m) <= std::f32::consts::PI + 1e-3);
        assert!(!overlaps(&a, &b, c.radius_m));
        // Shrunk relative to natural Flatland metres.
        assert!(a.width_m < 1.662 - 1e-3, "meta width {}", a.width_m);
        assert!(a.natural_width_m > a.width_m);
        // Raising radius to Flatland's 0.70 restores full size.
        c.set_radius(0.70);
        let a = c.slot(a_id).unwrap().clone();
        let b = c.slot(b_id).unwrap().clone();
        assert!((a.width_m - 3440.0 / 2070.0).abs() < 1e-3, "meta restored {}", a.width_m);
        assert!((b.width_m - 2560.0 / 2300.0).abs() < 1e-3, "edp restored {}", b.width_m);
        assert!((a.yaw_rad - 1.226).abs() < 0.02, "meta yaw {}", a.yaw_rad); // ~70.3 deg
        assert!((b.yaw_rad + 0.834).abs() < 0.02, "edp yaw {}", b.yaw_rad); // ~-47.8 deg
    }

    #[test]
    fn swap_and_reorder_change_sides() {
        let mut c = CylinderState::default();
        let a = c.assign_slot(&m(1000, 500));
        let b = c.assign_slot(&m(1000, 500));
        assert!(c.slot(a.id).unwrap().yaw_rad > 0.0);
        assert!(c.slot(b.id).unwrap().yaw_rad < 0.0);
        assert!(c.swap_slots(a.id, b.id));
        assert!(c.slot(a.id).unwrap().yaw_rad < 0.0);
        assert!(c.slot(b.id).unwrap().yaw_rad > 0.0);
        assert!(c.reorder_slot(a.id, 0));
        assert!(c.slot(a.id).unwrap().yaw_rad > 0.0);
    }

    #[test]
    fn neighbour_follows_registration_order() {
        let mut c = CylinderState::default();
        let a = c.assign_slot(&m(1000, 500));
        let b = c.assign_slot(&m(1000, 500));
        assert_eq!(c.neighbour(a.id, true), Some(b.id));
        assert_eq!(c.neighbour(b.id, false), Some(a.id));
        assert_eq!(c.neighbour(a.id, false), None);
    }

    #[test]
    fn slide_then_settle_orders_by_yaw() {
        let mut c = CylinderState::default();
        let a = c.assign_slot(&PanelMetrics {
            size_px: (1000, 500),
            density_px_per_m: 2000.0,
            app_id: Some("a".into()),
            title: None,
        });
        let b = c.assign_slot(&PanelMetrics {
            size_px: (1000, 500),
            density_px_per_m: 2000.0,
            app_id: Some("b".into()),
            title: None,
        });
        // a is RIGHT (+), b is LEFT (-) in registration order
        assert!(a.yaw_rad > 0.0 && b.yaw_rad < 0.0, "pack a+ b- got {} {}", a.yaw_rad, b.yaw_rad);
        // Slide a past b to the far left
        c.slide_slot(a.id, -1.5).unwrap();
        assert!(c.settle_slot_by_yaw(a.id));
        // After settle+relayout, a should be first in leftward sense: registration order by yaw
        let ia = c.slot_index(a.id).unwrap();
        let ib = c.slot_index(b.id).unwrap();
        // a had more negative yaw at settle → lower index among sorted-by-yaw insert
        assert_eq!(ia, 0, "a should land at index 0 after settling left of b");
        assert_eq!(ib, 1);
        // Pack still alternates from gap — just check both have finite disjoint yaw
        let ya = c.slot(a.id).unwrap().yaw_rad;
        let yb = c.slot(b.id).unwrap().yaw_rad;
        assert!(ya.is_finite() && yb.is_finite());
        assert!((ya - yb).abs() > 0.05, "packed yaw should differ");
    }

    #[test]
    fn palm_openness_hysteresis_helpers() {
        // Mirrors curved_pointer::palm_openness thresholds.
        fn classify(openness: f32, was_fist: bool) -> &'static str {
            const OPEN: f32 = 0.055;
            const FIST: f32 = 0.032;
            if was_fist {
                if openness >= OPEN { "open" } else { "fist" }
            } else if openness <= FIST {
                "fist"
            } else {
                "open"
            }
        }
        assert_eq!(classify(0.08, false), "open");
        assert_eq!(classify(0.02, false), "fist");
        // hysteresis: mid-band keeps prior
        assert_eq!(classify(0.04, true), "fist");
        assert_eq!(classify(0.04, false), "open");
        assert_eq!(classify(0.06, true), "open");
    }
}
