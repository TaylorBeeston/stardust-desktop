//! Curved pointer/touch hit-test. Owned by the panel worker that owns the mesh.
//!
//! Flatland's lab curve draws a bent `SurfaceModel` but keeps flat `PointerPlane`s
//! (see Flatland `curve.rs` header and `surface_input.rs`). Product must map rays
//! onto the cylinder slice, not a flat proxy.

/// Map a world-space ray onto panel surface coordinates.
pub trait CurvedHitTest {
    /// Returns normalized UV on the panel (u right, v down) if the ray hits the front face.
    fn ray_to_surface_uv(&self, origin: [f32; 3], direction: [f32; 3]) -> Option<(f32, f32)>;

    fn uv_to_panel_px(&self, uv: (f32, f32), size_px: (u32, u32)) -> (f32, f32) {
        (uv.0 * size_px.0 as f32, uv.1 * size_px.1 as f32)
    }
}

/// Hit-test for a cylinder-slice panel centred at `yaw_rad` on radius `radius_m`.
#[derive(Debug, Clone, Copy)]
pub struct CylinderSliceHitTest {
    pub radius_m: f32,
    pub yaw_rad: f32,
    pub width_m: f32,
    pub height_m: f32,
}

impl CurvedHitTest for CylinderSliceHitTest {
    fn ray_to_surface_uv(&self, origin: [f32; 3], direction: [f32; 3]) -> Option<(f32, f32)> {
        let _ = (origin, direction);
        // Intersect ray with cylinder, reject back-face / outside arc / outside height.
        todo!("CylinderSliceHitTest::ray_to_surface_uv")
    }
}
