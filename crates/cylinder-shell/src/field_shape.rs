//! Segmented thin field following the same rendered arc as the panel mesh.
//! Fusion has no cylinder-shell primitive, but supports unions of transformed boxes.
//! Tangent segments avoid the old chord box missing the curved edges and claiming empty space.

use stardust_xr_fusion::fields::Shape;

/// Full extents `(x, y, z)` metres of the rendered slice AABB.
/// The actual curved field below is not this box; this remains useful for bounds.
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

/// Full field thickness. Segment sag is below 0.1 mm at normal headset radii.
pub const FIELD_THICKNESS_M: f32 = 0.006;

pub fn field_shape_for_slice(width_m: f32, height_m: f32, radius_m: f32) -> Shape {
    let h = crate::mesh::rendered_slice(width_m, height_m, radius_m);
    let count = crate::mesh::segments_for(h.width_m / h.radius_m);
    let angle = h.width_m / h.radius_m / count as f32;
    let chord = 2.0 * h.radius_m * (angle * 0.5).sin();
    let sag = h.radius_m * (1.0 - (angle * 0.5).cos());
    let shapes = (0..count).map(|i| {
        let u = (i as f32 + 0.5) / count as f32;
        // The chord midpoint is sag metres inward from the true arc.
        let centre = glam::Vec3::from(h.surface_point(u, 0.5, sag));
        let transform = glam::Mat4::from_rotation_translation(h.surface_rotation(u), centre);
        Shape::Transform {
            transform: transform.into(),
            shape: Box::new(Shape::Box { size: [chord + 0.0002, h.height_m, FIELD_THICKNESS_M].into() }),
        }
    }).collect();
    Shape::Union { shapes }
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
        let _ = field_shape_for_slice(0.64, 0.36, 0.70); // type-checks the composed shape
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
        let z = h.radius_m * (1.0 - c);
        // inset so exact-edge float noise cannot push u/v outside [0,1]
        let uv = h.mesh_local_point_to_uv([x * 0.999, h.height_m / 2.0 * 0.999, z]).unwrap();
        assert!(uv.0 < 0.01 && uv.1 < 0.01, "{uv:?}");
        assert!(h.mesh_local_point_to_uv([0.0, h.height_m, 0.0]).is_none());
    }

    fn close(a: (f32, f32), b: (f32, f32)) -> bool {
        (a.0 - b.0).abs() < 1e-4 && (a.1 - b.1).abs() < 1e-4
    }

    #[test]
    fn field_follows_rendered_edges_without_filling_the_chord() {
        let h = crate::mesh::rendered_slice(0.42666668, 0.24, 0.45);
        let field = field_shape_for_slice(h.width_m, h.height_m, h.radius_m);
        for u in [0.001, 0.05, 0.25, 0.5, 0.75, 0.95, 0.999] {
            let p = h.surface_point(u, 0.5, 0.0);
            assert!(field.sample(p).distance <= 0.0, "arc edge {u} must be inside capture field");
            let front = h.surface_point(u, 0.5, 0.02);
            assert!(field.sample(front).distance > 0.01, "field must not claim space ahead of arc");
        }
        let edge_z = h.surface_point(0.0, 0.5, 0.0)[2];
        assert!(field.sample([0.0, 0.0, edge_z * 0.5]).distance > 0.01,
            "chord interior must not masquerade as the surface");
    }

}
