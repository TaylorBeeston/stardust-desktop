//! Bake a cylinder-slice GLB for `SurfaceModel` (ported from Flatland `curve.rs` lab).
//!
//! Mesh frame matches SlotPlacement: panel centre at the origin, edges
//! wrapping toward +Z around the viewer axis at (0, 0, +radius). UV convention: **u increases toward
//! −X** (+X edge u=0, −X edge u=1), +Y -> v=0 — Flatland's `curve.rs` `1 − t` bake, which
//! reads correctly on the **concave (inside)** face in the headset. The same convention is
//! used by `CylinderSliceHitTest::mesh_local_point_to_uv` / pointer UV so the texel under a
//! point is the texel the pointer reports. Double-sided. Pure std: runs headless.

use std::io;
use std::path::{Path, PathBuf};

/// Default cache dir: `$XDG_RUNTIME_DIR/cylinder-shell/`.
pub fn cache_dir() -> PathBuf {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").unwrap_or_else(|| "/tmp".into());
    PathBuf::from(runtime).join("cylinder-shell")
}

/// Write (or reuse) a GLB holding a "Panel" node in [`cache_dir`].
/// Deviation from design sketch: returns `io::Result` instead of panicking.
pub fn bake_curved_panel_glb(width_m: f32, height_m: f32, radius_m: f32) -> io::Result<PathBuf> {
    bake_curved_panel_glb_in(&cache_dir(), width_m, height_m, radius_m)
}

/// Quantize radius so live drags do not rebuild a mesh every frame.
pub fn quantize_radius_m(radius_m: f32) -> f32 {
    ((radius_m * 50.0).round() / 50.0).max(0.02)
}

/// Segment count used for an arc of `arc_rad` (1.5 degrees per segment, 8..=600).
pub fn segments_for(arc_rad: f32) -> usize {
    ((arc_rad.to_degrees() / 1.5).ceil() as usize).clamp(8, 600)
}

pub fn bake_curved_panel_glb_in(dir: &Path, width_m: f32, height_m: f32, radius_m: f32) -> io::Result<PathBuf> {
    let h = rendered_slice(width_m, height_m, radius_m);
    write_slice_glb(dir, h.width_m, h.height_m, h.radius_m, "cyl-v3")
}

/// Exactly the dimensions stored in the GLB, also used for overlays and fields.
pub fn rendered_slice(width_m: f32, height_m: f32, radius_m: f32) -> crate::hit_test::CylinderSliceHitTest {
    let q = |v: f32| (v * 100.0).round().max(1.0) / 100.0;
    let (width, height) = (q(width_m), q(height_m));
    let radius = quantize_radius_m(radius_m).max(width / 175f32.to_radians());
    crate::hit_test::CylinderSliceHitTest {width_m: width, height_m: height, radius_m: radius, yaw_rad: 0.0}
}

/// Small curved cursor patch; dimensions must not be rounded to panel-size centimetres.
pub fn bake_cursor_glb(width: f32, height: f32, radius: f32) -> io::Result<PathBuf> {
    write_slice_glb(&cache_dir(), width, height, radius, "cursor-v3")
}

fn write_slice_glb(dir: &Path, width: f32, height: f32, radius: f32, prefix: &str) -> io::Result<PathBuf> {
    let mm = |v: f32| (v * 1_000_000.0).round() as i64;
    let path = dir.join(format!("{prefix}-{}x{}-r{}.glb", mm(width), mm(height), mm(radius)));
    if path.exists() {
        return Ok(path);
    }
    std::fs::create_dir_all(dir)?;
    let arc = width / radius;
    let segs = segments_for(arc);
    let (mut pos, mut uv, mut nrm, mut idx) = (vec![], vec![], vec![], Vec::<u16>::new());
    let h = crate::hit_test::CylinderSliceHitTest {radius_m: radius, width_m: width, height_m: height, yaw_rad: 0.0};
    for i in 0..=segs {
        let t = i as f32 / segs as f32;
        let th = (t - 0.5) * arc;
        let (s, c) = th.sin_cos();
        for v in [0.0f32, 1.0] {
            // Bevy gltf_backport imports [x,y,z] as [-x,y,-z]. Export in
            // glTF space; texture U stays +X-right in the rendered panel.
            let native = h.surface_point(t, v, 0.0);
            pos.extend([-native[0], native[1], -native[2]]);
            uv.extend([1.0 - t, v]);
            nrm.extend([-s, 0.0, -c]);
        }
    }
    for i in 0..segs as u16 {
        let (a, b, c, d) = (2 * i, 2 * i + 1, 2 * i + 2, 2 * i + 3);
        idx.extend([a, b, c, c, b, d]);
        idx.extend([a, c, b, c, d, b]);
    }
    let mut bin: Vec<u8> = vec![];
    let f = |v: &Vec<f32>, bin: &mut Vec<u8>| v.iter().for_each(|x| bin.extend(x.to_le_bytes()));
    f(&pos, &mut bin);
    let uv_off = bin.len();
    f(&uv, &mut bin);
    let n_off = bin.len();
    f(&nrm, &mut bin);
    let i_off = bin.len();
    idx.iter().for_each(|x| bin.extend(x.to_le_bytes()));
    while bin.len() % 4 != 0 {
        bin.push(0);
    }
    let nv = pos.len() / 3;
    let zmax = radius * (1.0 - (arc / 2.0).cos());
    let xmax = if arc / 2.0 >= std::f32::consts::FRAC_PI_2 { radius } else { radius * (arc / 2.0).sin() };
    let json = format!(
        r#"{{"asset":{{"version":"2.0","generator":"cylinder-shell mesh.rs"}},"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0,"name":"Panel"}}],"meshes":[{{"name":"Panel","primitives":[{{"attributes":{{"POSITION":0,"TEXCOORD_0":1,"NORMAL":2}},"indices":3}}]}}],"accessors":[{{"bufferView":0,"componentType":5126,"count":{nv},"type":"VEC3","min":[{nx},{ny},-{nz}],"max":[{xmax},{hy},0]}},{{"bufferView":1,"componentType":5126,"count":{nv},"type":"VEC2"}},{{"bufferView":2,"componentType":5126,"count":{nv},"type":"VEC3"}},{{"bufferView":3,"componentType":5123,"count":{ni},"type":"SCALAR"}}],"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":{uv_off},"target":34962}},{{"buffer":0,"byteOffset":{uv_off},"byteLength":{uvl},"target":34962}},{{"buffer":0,"byteOffset":{n_off},"byteLength":{nl},"target":34962}},{{"buffer":0,"byteOffset":{i_off},"byteLength":{il},"target":34963}}],"buffers":[{{"byteLength":{bl}}}]}}"#,
        nx = -xmax,
        nz = zmax,
        ny = -height / 2.0,
        hy = height / 2.0,
        ni = idx.len(),
        uvl = n_off - uv_off,
        nl = i_off - n_off,
        il = idx.len() * 2,
        bl = bin.len(),
    );
    let mut json = json.into_bytes();
    while json.len() % 4 != 0 {
        json.push(b' ');
    }
    let total = 12 + 8 + json.len() + 8 + bin.len();
    let mut out = Vec::with_capacity(total);
    out.extend(b"glTF");
    out.extend(2u32.to_le_bytes());
    out.extend((total as u32).to_le_bytes());
    out.extend((json.len() as u32).to_le_bytes());
    out.extend(b"JSON");
    out.extend(&json);
    out.extend((bin.len() as u32).to_le_bytes());
    out.extend(b"BIN\0");
    out.extend(&bin);
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    std::fs::write(&tmp, out)?;
    std::fs::rename(&tmp, &path)?;
    tracing::info!(?path, segs, "wrote curved panel mesh");
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bakes_valid_glb_with_expected_vertex_count() {
        let dir = tempfile::tempdir().unwrap();
        let p = bake_curved_panel_glb_in(dir.path(), 0.64, 0.36, 0.70).unwrap();
        let bytes = std::fs::read(&p).unwrap();
        assert_eq!(&bytes[0..4], b"glTF");
        assert_eq!(u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize, bytes.len());
        let jlen = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        let json: serde_json::Value = serde_json::from_slice(&bytes[20..20 + jlen]).unwrap();
        let segs = segments_for(0.64 / 0.70);
        let nv = json["accessors"][0]["count"].as_u64().unwrap() as usize;
        assert_eq!(nv, 2 * (segs + 1));
        assert_eq!(json["accessors"][3]["count"].as_u64().unwrap() as usize, segs * 12);
        assert_eq!(json["nodes"][0]["name"], "Panel");
        // vertices lie on the circle centred (0,0,-r)
        let r = 0.70f32;
        let boff = 20 + jlen + 8;
        for v in 0..nv {
            let rd = |k: usize| f32::from_le_bytes(bytes[boff + (v * 3 + k) * 4..][..4].try_into().unwrap());
            let (x, z) = (-rd(0), -rd(2)); // glTF -> server native
            assert!((x * x + (z - r) * (z - r) - r * r).abs() < 1e-4);
        }
        // cache quantizes inputs: nearby sizes reuse the same path
        assert_eq!(bake_curved_panel_glb_in(dir.path(), 0.641, 0.36, 0.70).unwrap(), p);
    }

    /// Mesh TEXCOORD_0 must equal hit-test UV at every vertex (±X edges included).
    #[test]
    fn mesh_texcoords_match_hit_test_uv() {
        use crate::hit_test::CylinderSliceHitTest;
        let dir = tempfile::tempdir().unwrap();
        let (w, hgt, r) = (0.64f32, 0.36f32, 0.70f32);
        let p = bake_curved_panel_glb_in(dir.path(), w, hgt, r).unwrap();
        let bytes = std::fs::read(&p).unwrap();
        let jlen = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        let json: serde_json::Value = serde_json::from_slice(&bytes[20..20 + jlen]).unwrap();
        let boff = 20 + jlen + 8;
        let nv = json["accessors"][0]["count"].as_u64().unwrap() as usize;
        let uv_off = json["bufferViews"][1]["byteOffset"].as_u64().unwrap() as usize;
        let rd = |o: usize| f32::from_le_bytes(bytes[boff + o..][..4].try_into().unwrap());
        let h = CylinderSliceHitTest { radius_m: r, yaw_rad: 0.0, width_m: w, height_m: hgt };
        let (mut saw_pos_x, mut saw_neg_x) = (false, false);
        for v in 0..nv {
            let pos = [-rd(v * 12), rd(v * 12 + 4), -rd(v * 12 + 8)];
            let mesh_uv = (rd(uv_off + v * 8), rd(uv_off + v * 8 + 4));
            // nudge inside the u/v range so edge vertices are not rejected by the bounds check
            let probe = [pos[0] * 0.999, pos[1] * 0.999, pos[2]];
            let hit = h.mesh_local_point_to_uv(probe).unwrap_or_else(|| panic!("vertex {v} {pos:?} missed"));
            assert!((1.0 - hit.0 - mesh_uv.0).abs() < 1e-3, "u mismatch at {pos:?}: hit {hit:?} mesh {mesh_uv:?}");
            assert!((hit.1 - mesh_uv.1).abs() < 2e-3, "v mismatch at {pos:?}: hit {hit:?} mesh {mesh_uv:?}");
            saw_pos_x |= pos[0] > 0.2;
            saw_neg_x |= pos[0] < -0.2;
        }
        assert!(saw_pos_x && saw_neg_x);
    }

    #[test]
    fn cursor_patch_vertices_stay_in_front_at_both_panel_edges() {
        let dir = tempfile::tempdir().unwrap();
        let h = rendered_slice(0.42666668, 0.24, 0.45);
        let eps = crate::pointer_uv::OVERLAY_INSET_M;
        let radius = h.radius_m - eps;
        let path = write_slice_glb(dir.path(), 0.012, 0.012, radius, "cursor-test").unwrap();
        let bytes = std::fs::read(path).unwrap();
        let json_len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        let json: serde_json::Value = serde_json::from_slice(&bytes[20..20 + json_len]).unwrap();
        let bin = &bytes[28 + json_len..];
        let count = json["accessors"][0]["count"].as_u64().unwrap() as usize;
        for u in [0.0, 0.02, 0.5, 0.98, 1.0] {
            let centre = glam::Vec3::from(h.surface_point(u, 0.5, eps));
            let rot = h.surface_rotation(u);
            for vertex in 0..count {
                let read = |component: usize| f32::from_le_bytes(bin[vertex * 12 + component * 4..vertex * 12 + component * 4 + 4].try_into().unwrap());
                let p = centre + rot * glam::Vec3::new(-read(0), read(1), -read(2));
                let rho = (p.x * p.x + (p.z - h.radius_m).powi(2)).sqrt();
                assert!((rho - radius).abs() < 1e-5, "edge {u}, cursor vertex {vertex}: radius {rho}, expected {radius}");
            }
        }
    }

    #[test]
    fn rounded_glb_and_overlay_share_the_same_surface() {
        let dir = tempfile::tempdir().unwrap();
        let h = rendered_slice(0.42666668, 0.24, 0.45);
        let path = bake_curved_panel_glb_in(dir.path(), 0.42666668, 0.24, 0.45).unwrap();
        let bytes = std::fs::read(path).unwrap();
        let json_len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        let json: serde_json::Value = serde_json::from_slice(&bytes[20..20 + json_len]).unwrap();
        let bin = &bytes[28 + json_len..];
        let read = |offset: usize| f32::from_le_bytes(bin[offset..offset + 4].try_into().unwrap());
        let uv_offset = json["bufferViews"][1]["byteOffset"].as_u64().unwrap() as usize;
        let count = json["accessors"][0]["count"].as_u64().unwrap() as usize;
        for vertex in 0..count {
            let p = glam::Vec3::new(-read(vertex * 12), read(vertex * 12 + 4), -read(vertex * 12 + 8));
            let uv = (read(uv_offset + vertex * 8), read(uv_offset + vertex * 8 + 4));
            let overlay = glam::Vec3::from(h.surface_point(1.0 - uv.0, uv.1, crate::pointer_uv::OVERLAY_INSET_M));
            let normal = glam::Vec3::new(-p.x, 0.0, h.radius_m - p.z).normalize();
            assert!((overlay - (p + normal * crate::pointer_uv::OVERLAY_INSET_M)).length() < 1e-5);
        }
    }

    #[test]
    fn imported_mesh_and_overlay_edges_bend_to_the_same_side() {
        let dir = tempfile::tempdir().unwrap();
        let h = rendered_slice(0.42666668, 0.24, 0.45);
        let bytes = std::fs::read(bake_curved_panel_glb_in(dir.path(), h.width_m, h.height_m, h.radius_m).unwrap()).unwrap();
        let json_len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        let json: serde_json::Value = serde_json::from_slice(&bytes[20..20 + json_len]).unwrap();
        let bin = &bytes[28 + json_len..];
        let count = json["accessors"][0]["count"].as_u64().unwrap() as usize;
        for (vertex, u) in [(0, 1.0), (count - 2, 0.0)] {
            let read = |c: usize| f32::from_le_bytes(bin[vertex*12+c*4..vertex*12+c*4+4].try_into().unwrap());
            // Actual locked Bevy loader conversion, not a raw-file-space comparison.
            let p = [-read(0), read(1), -read(2)];
            let overlay = h.surface_point(1.0-u, 0.0, crate::pointer_uv::OVERLAY_INSET_M);
            assert!(p[2] > 0.04 && overlay[2] > 0.04, "both edges must bend toward native +Z");
            assert!((p[0] > 0.0) == (u > 0.5), "texture U must increase toward screen right");
            assert!((p[2] - overlay[2]).abs() < 0.004);
        }
    }

}
