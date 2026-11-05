//! Closest Fusion [`Shape`] for a cylinder-slice panel Field.
//!
//! Fusion's SDFs include solid `Cylinder` (Y-axis), `Box`, `Torus`, etc., but **no
//! cylinder-shell / curved-quad surface**. Flatland's lab uses a flat `Shape::Box` proxy
//! for input while the mesh bends (`curve.rs` "phase 2"). Product owns the curved
//! ray→UV math in [`crate::CylinderSliceHitTest`]; the Field here is the closest capture
//! volume the server will accept — the axis-aligned box of the chord AABB in mesh-local
//! frame (centre origin, bend toward −Z).

use stardust_xr_fusion::fields::Shape;

/// Full extents `(x, y, z)` metres of the chord AABB for a cylinder-slice panel.
///
/// Mesh verts lie in `x ∈ [-xmax, xmax]`, `y ∈ [-h/2, h/2]`, `z ∈ [-zmax, 0]` with the
/// axis at `(0, 0, -radius)`. The returned box is sized to cover that AABB when centred
/// on the panel spatial (depth `zmax`, so the +Z half sticks slightly past the chord
/// toward the user — acceptable capture padding).
pub fn slice_aabb_extents(width_m: f32, height_m: f32, radius_m: f32) -> (f32, f32, f32) {
    let width = width_m.max(1e-4);
    let height = height_m.max(1e-4);
    let radius = radius_m.max(width / 175f32.to_radians()).max(1e-4);
    let half_arc = (width / radius) * 0.5;
    let xmax = if half_arc >= std::f32::consts::FRAC_PI_2 {
        radius
    } else {
        radius * half_arc.sin()
    };
    let zmax = radius * (1.0 - half_arc.cos());
    (2.0 * xmax, height, zmax.max(0.01))
}

/// Closest Field shape for a curved panel of the given metres size on `radius_m`.
pub fn field_shape_for_slice(width_m: f32, height_m: f32, radius_m: f32) -> Shape {
    let (x, y, z) = slice_aabb_extents(width_m, height_m, radius_m);
    Shape::Box {
        size: [x, y, z].into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aabb_matches_flat_when_radius_huge() {
        // Nearly flat: xmax ≈ width/2, zmax ≈ 0 → clamped to 1 cm
        let (x, y, z) = slice_aabb_extents(0.64, 0.36, 100.0);
        assert!((x - 0.64).abs() < 0.01, "{x}");
        assert!((y - 0.36).abs() < 1e-6);
        assert!((z - 0.01).abs() < 1e-6);
    }

    #[test]
    fn aabb_deepens_as_radius_shrinks() {
        let flat = slice_aabb_extents(0.64, 0.36, 10.0);
        let bent = slice_aabb_extents(0.64, 0.36, 0.50);
        assert!(bent.2 > flat.2);
        assert!(bent.0 < flat.0); // chord shorter than arc width
        let _ = field_shape_for_slice(0.64, 0.36, 0.70); // type-checks Shape::Box
    }

    #[test]
    fn mesh_local_centre_and_edges_uv() {
        use crate::hit_test::CylinderSliceHitTest;
        let h = CylinderSliceHitTest {
            radius_m: 0.70,
            yaw_rad: 0.0,
            width_m: 0.64,
            height_m: 0.36,
        };
        assert!(close(h.mesh_local_point_to_uv([0.0, 0.0, 0.0]).unwrap(), (0.5, 0.5)));
        let half = h.width_m / 2.0 / h.radius_m;
        let (s, c) = half.sin_cos();
        let x = h.radius_m * s;
        let z = -h.radius_m * (1.0 - c);
        let uv = h.mesh_local_point_to_uv([x, h.height_m / 2.0, z]).unwrap();
        assert!(uv.0 > 0.99 && uv.1 < 0.01, "{uv:?}");
        assert!(h.mesh_local_point_to_uv([0.0, h.height_m, 0.0]).is_none());
    }

    fn close(a: (f32, f32), b: (f32, f32)) -> bool {
        (a.0 - b.0).abs() < 1e-4 && (a.1 - b.1).abs() < 1e-4
    }
}
