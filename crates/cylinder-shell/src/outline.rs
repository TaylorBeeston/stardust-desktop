//! Curved panel focus outline (thin border along the slice edge).

use crate::hit_test::CylinderSliceHitTest;
use crate::pointer_uv::OVERLAY_INSET_M;
use stardust_xr_fusion::drawable::Line;
use stardust_xr_fusion::types::rgba_linear;
use stardust_xr_molecules::lines::{line_from_points, LineExt as _};
use std::time::Duration;

/// Hide the outline this long after the last keyboard/mouse handler event.
pub const FOCUS_OUTLINE_TIMEOUT: Duration = Duration::from_secs(2);

/// Line thickness in metres (Flatland debug outline uses ~line_thickness; keep readable).
pub const FOCUS_OUTLINE_THICKNESS: f32 = 0.003;
/// Slightly in front of the visible +Z face so the border is not z-fought inside the mesh.
pub const FOCUS_OUTLINE_OFFSET_M: f32 = OVERLAY_INSET_M;

/// Closed polyline following the curved panel's UV rectangle (top → right → bottom → left).
pub fn curved_panel_border(width_m: f32, height_m: f32, radius_m: f32) -> Line {
    curved_panel_border_segs(width_m, height_m, radius_m, 24)
}

pub fn curved_panel_border_segs(width_m: f32, height_m: f32, radius_m: f32, segs: usize) -> Line {
    let h = CylinderSliceHitTest {
        radius_m,
        yaw_rad: 0.0,
        width_m,
        height_m,
    };
    let segs = segs.max(4);
    let mut pts: Vec<[f32; 3]> = Vec::with_capacity(segs * 4);
    // top: u 0→1, v=0
    for i in 0..=segs {
        let u = i as f32 / segs as f32;
        pts.push(outline_point(&h, u, 0.0));
    }
    // right: u=1, v 0→1
    for i in 1..=segs {
        let v = i as f32 / segs as f32;
        pts.push(outline_point(&h, 1.0, v));
    }
    // bottom: u 1→0, v=1
    for i in 1..=segs {
        let u = 1.0 - i as f32 / segs as f32;
        pts.push(outline_point(&h, u, 1.0));
    }
    // left: u=0, v 1→0
    for i in 1..=segs {
        let v = 1.0 - i as f32 / segs as f32;
        pts.push(outline_point(&h, 0.0, v));
    }
    let mut line = line_from_points(pts);
    line.cyclic = true;
    line.thickness(FOCUS_OUTLINE_THICKNESS)
        .color(rgba_linear!(0.55, 0.85, 1.0, 0.95))
}

/// Surface point offset toward the user (+Z-facing side) so the line sits
/// in front of the mesh rather than inside it.
fn outline_point(h: &CylinderSliceHitTest, u: f32, v: f32) -> [f32; 3] {
    h.surface_point(u, v, FOCUS_OUTLINE_OFFSET_M)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn border_is_cyclic_and_nonempty() {
        let line = curved_panel_border(0.6, 0.4, 0.7);
        assert!(line.cyclic);
        assert!(line.points.len() > 16);
        // Centre of top edge ≈ (0, +h/2, small +Z offset)
        let top_mid = &line.points[12]; // segs=24, top has 25 pts, mid ≈ index 12
        assert!(
            top_mid.point.y > 0.15,
            "top mid y should be near +height/2, got {}",
            top_mid.point.y
        );
        assert!(
            (top_mid.thickness - FOCUS_OUTLINE_THICKNESS).abs() < 1e-6,
            "thickness applied, got {}",
            top_mid.thickness
        );
        // Offset toward user at u=0.5 → +Z
        assert!(
            top_mid.point.z > 0.0005,
            "expected toward-user +Z offset, got z={}",
            top_mid.point.z
        );
    }

    #[test]
    fn border_is_curved_and_in_front_of_the_surface_across_the_arc() {
        let line = curved_panel_border(0.6, 0.4, 0.45);
        let top = &line.points[..25];
        assert!(top[0].point.z - top[12].point.z > 0.09, "top edge must follow the arc, not a chord");
        for point in &line.points {
            let p = point.point;
            let radial_distance = (p.x * p.x + (p.z - 0.45).powi(2)).sqrt();
            assert!((radial_distance - (0.45 - FOCUS_OUTLINE_OFFSET_M)).abs() < 1e-5,
                "border must sit on the user-facing side, got {p:?}");
        }
    }

}
