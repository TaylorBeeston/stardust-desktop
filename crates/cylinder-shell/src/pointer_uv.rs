//! Pure glue between XR input (in the panel's **mesh-local** frame) and panel pixels.
//!
//! Mesh-local frame (same as [`crate::mesh`] and the panel `Spatial`): panel centre at the
//! origin, edges wrapping toward −Z around an axis at `(0, 0, −radius)`, +Y up.
//! The Field is still the chord-AABB box (Fusion has no cylinder-slice SDF), so it only
//! decides *whether* input reaches us; *where* it lands is computed here on the true arc.

use crate::hit_test::CylinderSliceHitTest;

/// Where an input lands on the curved slice.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SliceHit {
    /// Normalized panel UV (u toward −X / concave-correct, v down), same as `ray_to_surface_uv`.
    pub uv: (f32, f32),
    /// Ray parameter (pointer) or signed radial offset from the surface (hand/tip; + = in
    /// front of the bulge, toward +Z).
    pub distance: f32,
}

/// Intersect a mesh-local ray with the cylinder slice. Returns the nearest hit with
/// `t > 0` that lies on the slice (either face — the GLB is double-sided).
pub fn mesh_local_ray_hit(h: &CylinderSliceHitTest, origin: [f32; 3], dir: [f32; 3]) -> Option<SliceHit> {
    let r = h.radius_m;
    // Shift so the axis is at the origin.
    let (ox, oy, oz) = (origin[0], origin[1], origin[2] + r);
    let [dx, dy, dz] = dir;
    let a = dx * dx + dz * dz;
    if a < 1e-12 || r <= 0.0 {
        return None;
    }
    let b = 2.0 * (ox * dx + oz * dz);
    let c = ox * ox + oz * oz - r * r;
    let disc = b * b - 4.0 * a * c;
    if disc < 0.0 {
        return None;
    }
    let sq = disc.sqrt();
    let (t0, t1) = ((-b - sq) / (2.0 * a), (-b + sq) / (2.0 * a));
    for t in [t0.min(t1), t0.max(t1)] {
        if t <= 0.0 {
            continue;
        }
        let p = [ox + t * dx, oy + t * dy, oz + t * dz - r];
        if let Some(uv) = h.mesh_local_point_to_uv(p) {
            return Some(SliceHit { uv, distance: t });
        }
    }
    None
}

/// Project a mesh-local point (hand / tip) radially onto the slice.
pub fn mesh_local_point_hit(h: &CylinderSliceHitTest, p: [f32; 3]) -> Option<SliceHit> {
    let uv = h.mesh_local_point_to_uv(p)?;
    let rho = (p[0] * p[0] + (p[2] + h.radius_m).powi(2)).sqrt();
    Some(SliceHit { uv, distance: rho - h.radius_m })
}


/// Inverse of [`CylinderSliceHitTest::mesh_local_point_to_uv`]: UV → point on the arc.
pub fn uv_to_mesh_local(h: &CylinderSliceHitTest, u: f32, v: f32) -> [f32; 3] {
    let phi = (0.5 - u) * h.width_m / h.radius_m.max(1e-6);
    let y = (0.5 - v) * h.height_m;
    let (s, c) = phi.sin_cos();
    [h.radius_m * s, y, -h.radius_m * (1.0 - c)]
}

/// UV → surface pixels, clamped to the surface so edge hits never go out of range.
pub fn uv_to_surface_px(uv: (f32, f32), size_px: (u32, u32)) -> [f32; 2] {
    let (w, h) = (size_px.0 as f32, size_px.1 as f32);
    [(uv.0.clamp(0.0, 1.0) * w).min(w.max(1.0) - 1.0).max(0.0), (uv.1.clamp(0.0, 1.0) * h).min(h.max(1.0) - 1.0).max(0.0)]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h() -> CylinderSliceHitTest {
        CylinderSliceHitTest { radius_m: 0.7, yaw_rad: 0.0, width_m: 0.6, height_m: 0.4 }
    }
    fn close(a: (f32, f32), b: (f32, f32)) -> bool {
        (a.0 - b.0).abs() < 1e-4 && (a.1 - b.1).abs() < 1e-4
    }
    /// Mesh vertex at arc fraction `t` (0 = −X edge) and height `y`, like `mesh.rs`.
    fn vert(h: &CylinderSliceHitTest, t: f32, y: f32) -> [f32; 3] {
        let th = (t - 0.5) * h.width_m / h.radius_m;
        [h.radius_m * th.sin(), y, -h.radius_m * (1.0 - th.cos())]
    }

    #[test]
    fn straight_ray_from_front_hits_centre() {
        let hit = mesh_local_ray_hit(&h(), [0.0, 0.0, 1.0], [0.0, 0.0, -1.0]).unwrap();
        assert!(close(hit.uv, (0.5, 0.5)));
        assert!((hit.distance - 1.0).abs() < 1e-5);
    }

    #[test]
    fn ray_from_axis_side_hits_same_uv_as_front() {
        // Double-sided: aiming from the axis (behind the bulge) at the same vertex.
        let h = h();
        let p = vert(&h, 0.8, 0.1);
        let axis = [0.0, 0.1, -h.radius_m];
        let d = [p[0] - axis[0], 0.0, p[2] - axis[2]];
        let a = mesh_local_ray_hit(&h, axis, d).unwrap();
        let front = [p[0] + d[0], p[1], p[2] + d[2]];
        let b = mesh_local_ray_hit(&h, front, [-d[0], 0.0, -d[2]]).unwrap();
        assert!(close(a.uv, b.uv), "{a:?} {b:?}");
        assert!(close(a.uv, (0.2, 0.25)), "{a:?}");
    }

    #[test]
    fn curved_differs_from_flat_proxy_near_edges() {
        // A ray parallel to −Z at x near the chord edge: the flat proxy would report
        // flat proxy u = 0.5 + x / chord_width; arc uses 1−t so edge u≈0.05.
        let h = h();
        let p = vert(&h, 0.95, 0.0);
        let hit = mesh_local_ray_hit(&h, [p[0], 0.0, 1.0], [0.0, 0.0, -1.0]).unwrap();
        assert!(close(hit.uv, (0.05, 0.5)), "{hit:?}");
        let (chord, _, _) = crate::field_shape::slice_aabb_extents(h.width_m, h.height_m, h.radius_m);
        let flat_u = 0.5 + p[0] / chord;
        // > 3 px off on a 1280-wide surface at r = 0.7 m — grows as radius shrinks.
        assert!((flat_u - 0.05).abs() * 1280.0 > 3.0, "flat {flat_u}");
    }

    #[test]
    fn misses_off_slice() {
        let h = h();
        assert!(mesh_local_ray_hit(&h, [0.0, 0.0, 1.0], [0.0, 0.0, 1.0]).is_none()); // away
        assert!(mesh_local_ray_hit(&h, [0.0, 0.5, 1.0], [0.0, 0.0, -1.0]).is_none()); // above
        assert!(mesh_local_ray_hit(&h, [0.6, 0.0, 1.0], [0.0, 0.0, -1.0]).is_none()); // beside
        assert!(mesh_local_ray_hit(&h, [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]).is_none()); // ∥ axis
    }

    #[test]
    fn point_hit_projects_radially() {
        let h = h();
        let p = vert(&h, 0.3, -0.1);
        let hit = mesh_local_point_hit(&h, p).unwrap();
        assert!(close(hit.uv, (0.7, 0.75)) && hit.distance.abs() < 1e-5, "{hit:?}");
        // 5 cm out along the normal: same UV, +0.05 offset
        let th = (0.3 - 0.5) * h.width_m / h.radius_m;
        let q = [p[0] + 0.05 * th.sin(), p[1], p[2] + 0.05 * th.cos()];
        let hit = mesh_local_point_hit(&h, q).unwrap();
        assert!(close(hit.uv, (0.7, 0.75)) && (hit.distance - 0.05).abs() < 1e-5, "{hit:?}");
    }

    #[test]
    fn uv_round_trips_with_mesh_local_point_to_uv() {
        let h = h();
        for (u, v) in [(0.5, 0.5), (0.1, 0.2), (0.9, 0.8)] {
            let p = uv_to_mesh_local(&h, u, v);
            let back = h.mesh_local_point_to_uv(p).unwrap();
            assert!(close(back, (u, v)), "{back:?} vs {u},{v}");
        }
    }

    #[test]
    fn px_clamped() {
        assert_eq!(uv_to_surface_px((0.5, 0.25), (1280, 720)), [640.0, 180.0]);
        assert_eq!(uv_to_surface_px((1.0, 1.2), (1280, 720)), [1279.0, 719.0]);
        assert_eq!(uv_to_surface_px((-0.1, 0.0), (1280, 720)), [0.0, 0.0]);
    }
}
