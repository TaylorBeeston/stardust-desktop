//! Curved pointer/touch hit-test. Owned by the panel worker that owns the mesh.
//!
//! Flatland's lab curve draws a bent `SurfaceModel` but keeps flat `PointerPlane`s
//! (see Flatland `curve.rs` header and `surface_input.rs`). Product must map rays
//! onto the cylinder slice, not a flat proxy.

/// Map a world-space ray onto panel surface coordinates.
pub trait CurvedHitTest {
    /// Returns normalized UV on the panel (u toward −X / concave-correct, v down) if the ray hits the front face.
    fn ray_to_surface_uv(&self, origin: [f32; 3], direction: [f32; 3]) -> Option<(f32, f32)>;

    fn uv_to_panel_px(&self, uv: (f32, f32), size_px: (u32, u32)) -> (f32, f32) {
        (uv.0 * size_px.0 as f32, uv.1 * size_px.1 as f32)
    }
}

/// Hit-test for a cylinder-slice panel centred at `yaw_rad` on radius `radius_m`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CylinderSliceHitTest {
    pub radius_m: f32,
    pub yaw_rad: f32,
    pub width_m: f32,
    pub height_m: f32,
}

/// Frame: cylinder-local. Axis is +Y through the origin (the user), forward is -Z,
/// yaw + = right (+X). A point at azimuth `phi` is `(r sin phi, y, -r cos phi)`.
/// The panel belt is centred on y = 0 (callers subtract group height first).
/// The front face looks inward (toward the axis); rays must hit it from inside.
impl CurvedHitTest for CylinderSliceHitTest {
    fn ray_to_surface_uv(&self, origin: [f32; 3], direction: [f32; 3]) -> Option<(f32, f32)> {
        let [ox, oy, oz] = origin;
        let [dx, dy, dz] = direction;
        let r = self.radius_m;
        let a = dx * dx + dz * dz;
        if a < 1e-12 || r <= 0.0 {
            return None; // parallel to axis
        }
        let b = 2.0 * (ox * dx + oz * dz);
        let c = ox * ox + oz * oz - r * r;
        let disc = b * b - 4.0 * a * c;
        if disc < 0.0 {
            return None;
        }
        let sq = disc.sqrt();
        let mut ts = [(-b - sq) / (2.0 * a), (-b + sq) / (2.0 * a)];
        ts.sort_by(|x, y| x.partial_cmp(y).unwrap());
        for t in ts {
            if t <= 0.0 {
                continue;
            }
            let (x, y, z) = (ox + t * dx, oy + t * dy, oz + t * dz);
            // Front face: inward normal (-x, 0, -z)/r must oppose the ray.
            if dx * x + dz * z <= 0.0 {
                continue;
            }
            let phi = x.atan2(-z);
            let mut d = phi - self.yaw_rad;
            d = (d + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
            let u = 0.5 - d * r / self.width_m;
            let v = 0.5 - y / self.height_m;
            if (0.0..=1.0).contains(&u) && (0.0..=1.0).contains(&v) {
                return Some((u, v));
            }
        }
        None
    }
}

impl CylinderSliceHitTest {
    /// A point on the rendered slice, offset inward toward the viewer axis.
    /// Texture U increases toward -X; logical cursor pixels are converted separately.
    pub fn surface_point(&self, u: f32, v: f32, inset_m: f32) -> [f32; 3] {
        let phi = (0.5 - u) * self.width_m / self.radius_m;
        let (s, c) = phi.sin_cos();
        let r = self.radius_m - inset_m;
        [r * s, (0.5 - v) * self.height_m, self.radius_m - r * c]
    }

    pub fn surface_rotation(&self, u: f32) -> glam::Quat {
        let phi = (0.5 - u) * self.width_m / self.radius_m;
        glam::Quat::from_rotation_y(-phi)
    }

    /// Project a point in **mesh-local** frame (panel centre at origin, edges toward +Z,
    /// axis at `(0, 0, +radius)`) onto panel UV. Used when a Field hover lands in the
    /// panel spatial. Field segments route input; this projection returns exact UV.
    /// `yaw_rad` is ignored (the mesh is always centred on its own yaw).
    pub fn mesh_local_point_to_uv(&self, point: [f32; 3]) -> Option<(f32, f32)> {
        let [x, y, z] = point;
        let r = self.radius_m;
        if r <= 0.0 || self.width_m <= 0.0 || self.height_m <= 0.0 {
            return None;
        }
        // The placement faces +Z toward the cylinder axis / viewer.
        let phi = x.atan2(r - z);
        let half = self.width_m * 0.5 / r;
        if phi.abs() > half * 1.0001 {
            return None;
        }
        let u = 0.5 - phi * r / self.width_m;
        let v = 0.5 - y / self.height_m;
        ((0.0..=1.0).contains(&u) && (0.0..=1.0).contains(&v)).then_some((u, v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panel(yaw: f32) -> CylinderSliceHitTest {
        CylinderSliceHitTest { radius_m: 0.7, yaw_rad: yaw, width_m: 0.6, height_m: 0.4 }
    }
    fn dir(yaw: f32, pitch: f32) -> [f32; 3] {
        [yaw.sin() * pitch.cos(), pitch.sin(), -yaw.cos() * pitch.cos()]
    }
    fn close(a: (f32, f32), b: (f32, f32)) -> bool {
        (a.0 - b.0).abs() < 1e-4 && (a.1 - b.1).abs() < 1e-4
    }

    #[test]
    fn centre_ray_hits_centre() {
        for yaw in [0.0, 0.8, -2.5, 3.1] {
            let uv = panel(yaw).ray_to_surface_uv([0.0; 3], dir(yaw, 0.0)).unwrap();
            assert!(close(uv, (0.5, 0.5)), "{yaw} {uv:?}");
        }
    }

    #[test]
    fn edges_and_corners_from_axis() {
        let p = panel(0.3);
        let half = p.width_m / 2.0 / p.radius_m; // arc half-angle
        // right edge, top: direction toward (yaw+half, y=+h/2) at radius r
        let target_y = p.height_m / 2.0 * 0.999;
        let az = p.yaw_rad + half * 0.999;
        let d = [p.radius_m * az.sin(), target_y, -p.radius_m * az.cos()];
        let uv = p.ray_to_surface_uv([0.0; 3], d).unwrap();
        assert!(uv.0 < 0.002 && uv.1 < 0.002, "{uv:?}");
        // a quarter in from the left (−yaw), bottom quarter → u = 0.75 under 1−t convention
        let az = p.yaw_rad - half / 2.0;
        let d = [p.radius_m * az.sin(), -p.height_m / 4.0, -p.radius_m * az.cos()];
        assert!(close(p.ray_to_surface_uv([0.0; 3], d).unwrap(), (0.75, 0.75)));
    }

    #[test]
    fn misses_outside_arc_height_and_behind() {
        let p = panel(0.0);
        assert!(p.ray_to_surface_uv([0.0; 3], dir(0.6, 0.0)).is_none()); // past arc edge
        assert!(p.ray_to_surface_uv([0.0; 3], dir(0.0, 0.5)).is_none()); // above
        assert!(p.ray_to_surface_uv([0.0; 3], dir(std::f32::consts::PI, 0.0)).is_none()); // behind
        assert!(p.ray_to_surface_uv([0.0; 3], [0.0, 1.0, 0.0]).is_none()); // along axis
    }

    #[test]
    fn off_axis_origin_and_back_face() {
        let p = panel(0.0);
        // head offset 0.2 m right, aiming at panel centre (0,0,-0.7)
        let o = [0.2, 0.0, 0.0];
        let uv = p.ray_to_surface_uv(o, [-0.2, 0.0, -0.7]).unwrap();
        assert!(close(uv, (0.5, 0.5)));
        // from outside, looking back at the panel's rear: rejected
        let behind = p.ray_to_surface_uv([0.0, 0.0, -2.0], [0.0, 0.0, 1.0]);
        // ray enters at z=-0.7 (back face, rejected) then exits at z=+0.7 (yaw=pi, outside arc)
        assert!(behind.is_none());
    }

    #[test]
    fn uv_to_px() {
        assert_eq!(panel(0.0).uv_to_panel_px((0.5, 0.25), (1280, 720)), (640.0, 180.0));
    }
}
