//! Curved panel focus outline (thin border along the slice edge).

use crate::hit_test::CylinderSliceHitTest;
use crate::pointer_uv::uv_to_mesh_local;
use stardust_xr_fusion::drawable::Line;
use stardust_xr_molecules::lines::line_from_points;
use std::time::Duration;

/// Hide the outline this long after the last keyboard/mouse handler event.
pub const FOCUS_OUTLINE_TIMEOUT: Duration = Duration::from_secs(2);

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
        pts.push(uv_to_mesh_local(&h, u, 0.0));
    }
    // right: u=1, v 0→1
    for i in 1..=segs {
        let v = i as f32 / segs as f32;
        pts.push(uv_to_mesh_local(&h, 1.0, v));
    }
    // bottom: u 1→0, v=1
    for i in 1..=segs {
        let u = 1.0 - i as f32 / segs as f32;
        pts.push(uv_to_mesh_local(&h, u, 1.0));
    }
    // left: u=0, v 1→0
    for i in 1..=segs {
        let v = 1.0 - i as f32 / segs as f32;
        pts.push(uv_to_mesh_local(&h, 0.0, v));
    }
    let mut line = line_from_points(pts);
    line.cyclic = true;
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn border_is_cyclic_and_nonempty() {
        let line = curved_panel_border(0.6, 0.4, 0.7);
        assert!(line.cyclic);
        assert!(line.points.len() > 16);
        // Centre of top edge ≈ (0, +h/2, 0)
        let top_mid = &line.points[12]; // segs=24, top has 25 pts, mid ≈ index 12
        assert!(
            top_mid.point.y > 0.15,
            "top mid y should be near +height/2, got {}",
            top_mid.point.y
        );
    }
}
