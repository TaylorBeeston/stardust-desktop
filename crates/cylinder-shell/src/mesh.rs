//! Bake a cylinder-slice GLB for `SurfaceModel` (concepts from Flatland `curve.rs` lab).

use std::path::PathBuf;

/// Write (or reuse) a GLB holding a "Panel" node: arc `width_m`, height `height_m`,
/// concave toward the user at `radius_m`. Path under `$XDG_RUNTIME_DIR/cylinder-shell/`.
pub fn bake_curved_panel_glb(width_m: f32, height_m: f32, radius_m: f32) -> PathBuf {
    let _ = (width_m, height_m, radius_m);
    todo!("bake_curved_panel_glb")
}

/// Quantize radius so live drags do not rebuild a mesh every frame.
pub fn quantize_radius_m(radius_m: f32) -> f32 {
    ((radius_m * 50.0).round() / 50.0).max(0.02)
}
