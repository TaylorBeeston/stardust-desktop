//! Open-palm "extend my reach" — fitted from labeled `hand-pose-recorder` takes (2026-11-06).
//!
//! ## Measured (protocol + labeled JSONL)
//! - Palm **face** normal = quat × **−Y** (`Hand::palm_normal`); agrees with wrist→middle ×
//!   index→little (sign by chirality) at n·jn ≈ 0.97. Palm **back** = +Y (9564d90 bug).
//! - Activation (fixtures + JSONL open takes): Open arms 100%; FaceMe/Back/Relaxed/Point/Pinch <5% after 2026-11-06 margin loosen.
//! - Grab curl: mean tip→palm ≤ 0.062 catches Fist + Loose fist; Relaxed/Pinch/Point stay out
//!   when fist is classified **before** pinch.
//!
//! Env overrides: `CYLINDER_PALM_*` (see [`ReachConfig::from_env`]).

use glam::{Quat, Vec3};
use std::time::{Duration, Instant};

pub const REACH_MAX_M: f32 = 2.5;
pub const BEAD_RAY_HIT_M: f32 = 0.08;
/// Thumb–index gap for click — only if not already fist-curled.
pub const PINCH_M: f32 = 0.022;

#[derive(Debug, Clone, Copy)]
pub struct ReachConfig {
    pub extend_m: f32,
    pub extend_min_m: f32,
    pub fist_m: f32,
    pub fist_open_m: f32,
    pub face_dot: f32,
    /// Reject poses where distal·+Y exceeds this (labeled Back of hand).
    pub max_distal_up: f32,
    pub arm_hold_ms: u64,
    pub distal_blend: f32,
    pub origin_distal_m: f32,
    pub aim_smooth: f32,
    pub curl_freeze_m: f32,
    /// How long aim stays frozen after fist grab (ms). Then live aim resumes.
    pub curl_freeze_ms: u64,
}

impl Default for ReachConfig {
    fn default() -> Self {
        // Fitted 2026-11-06 from right+left labeled takes.
        // Loosened 2026-11-06 from live flake + labeled JSONL margins (still
        // rejects fixture Relaxed/Back/FaceMe/Point at <5%):
        // open tip_mean ≥0.113 / tip_min ≥0.095 / face·(−Z) ≥0.52 / distal.y ≤0.905;
        // back-of-hand distal.y ≥ 0.929 must stay rejected.
        Self {
            extend_m: 0.105,
            extend_min_m: 0.090,
            fist_m: 0.062,
            fist_open_m: 0.090,
            face_dot: 0.45,
            max_distal_up: 0.918,
            arm_hold_ms: 100,
            distal_blend: 0.25,
            origin_distal_m: 0.02,
            // EMA blend for live aim while armed (1 = no smooth).
            aim_smooth: 0.28,
            // Freeze aim once mean tip distance drops below this while armed.
            curl_freeze_m: 0.095,
            curl_freeze_ms: 200,
        }
    }
}

impl ReachConfig {
    pub fn from_env() -> Self {
        let mut c = Self::default();
        let f = |k: &str, slot: &mut f32| {
            if let Ok(v) = std::env::var(k) {
                if let Ok(x) = v.parse() {
                    *slot = x;
                }
            }
        };
        let u = |k: &str, slot: &mut u64| {
            if let Ok(v) = std::env::var(k) {
                if let Ok(x) = v.parse() {
                    *slot = x;
                }
            }
        };
        f("CYLINDER_PALM_EXTEND_M", &mut c.extend_m);
        f("CYLINDER_PALM_EXTEND_MIN_M", &mut c.extend_min_m);
        f("CYLINDER_PALM_FIST_M", &mut c.fist_m);
        f("CYLINDER_PALM_FIST_OPEN_M", &mut c.fist_open_m);
        f("CYLINDER_PALM_FACE_DOT", &mut c.face_dot);
        f("CYLINDER_PALM_MAX_DISTAL_UP", &mut c.max_distal_up);
        u("CYLINDER_PALM_ARM_HOLD_MS", &mut c.arm_hold_ms);
        f("CYLINDER_PALM_DISTAL_BLEND", &mut c.distal_blend);
        f("CYLINDER_PALM_ORIGIN_DISTAL_M", &mut c.origin_distal_m);
        f("CYLINDER_PALM_AIM_SMOOTH", &mut c.aim_smooth);
        f("CYLINDER_PALM_CURL_FREEZE_M", &mut c.curl_freeze_m);
        u("CYLINDER_PALM_CURL_FREEZE_MS", &mut c.curl_freeze_ms);
        c
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PalmShape {
    Open,
    Fist,
}

#[derive(Debug, Clone, Copy)]
pub struct HandAxes {
    pub palm_pos: [f32; 3],
    pub palm_normal: [f32; 3],
    pub palm_back: [f32; 3],
    pub distal: [f32; 3],
    #[allow(dead_code)]
    pub chirality_right: bool,
}

pub fn hand_axes_from_quat(palm_pos: [f32; 3], quat_xyzw: [f32; 4], right_hand: bool) -> HandAxes {
    let q = Quat::from_xyzw(quat_xyzw[0], quat_xyzw[1], quat_xyzw[2], quat_xyzw[3]);
    HandAxes {
        palm_pos,
        palm_normal: (q * -Vec3::Y).normalize_or_zero().to_array(),
        palm_back: (q * Vec3::Y).normalize_or_zero().to_array(),
        distal: (q * -Vec3::Z).normalize_or_zero().to_array(),
        chirality_right: right_hand,
    }
}

#[allow(dead_code)]
pub fn palm_normal_from_joints(
    wrist: [f32; 3],
    middle_meta: [f32; 3],
    index_meta: [f32; 3],
    little_meta: [f32; 3],
    right_hand: bool,
) -> [f32; 3] {
    let along = (Vec3::from(middle_meta) - Vec3::from(wrist)).normalize_or_zero();
    let across = (Vec3::from(little_meta) - Vec3::from(index_meta)).normalize_or_zero();
    // Measured: for a right hand with fingers −Z and index +X / little −X, along×across = −Y (palm face).
    // Left hand flips across relative to along → negate.
    let n = along.cross(across).normalize_or_zero();
    let n = if right_hand { n } else { -n };
    n.to_array()
}

pub fn finger_tip_distances(palm: [f32; 3], tips: [[f32; 3]; 4]) -> [f32; 4] {
    let p = Vec3::from(palm);
    [
        p.distance(Vec3::from(tips[0])),
        p.distance(Vec3::from(tips[1])),
        p.distance(Vec3::from(tips[2])),
        p.distance(Vec3::from(tips[3])),
    ]
}

pub fn mean_tip_distance(dists: [f32; 4]) -> f32 {
    (dists[0] + dists[1] + dists[2] + dists[3]) * 0.25
}

pub fn fingers_extended(dists: [f32; 4], cfg: &ReachConfig) -> bool {
    mean_tip_distance(dists) >= cfg.extend_m
        && dists
            .iter()
            .copied()
            .min_by(|a, b| a.total_cmp(b))
            .unwrap_or(0.0)
            >= cfg.extend_min_m
}

/// Fist before pinch: curled hand with small pinch gap is still a fist (Loose fist takes).
pub fn classify_curl(mean_dist: f32, prev: PalmShape, cfg: &ReachConfig) -> PalmShape {
    match prev {
        PalmShape::Fist => {
            if mean_dist >= cfg.fist_open_m {
                PalmShape::Open
            } else {
                PalmShape::Fist
            }
        }
        PalmShape::Open => {
            if mean_dist <= cfg.fist_m {
                PalmShape::Fist
            } else {
                PalmShape::Open
            }
        }
    }
}

pub fn reach_direction(axes: &HandAxes, cfg: &ReachConfig) -> [f32; 3] {
    let n = Vec3::from(axes.palm_normal);
    let d = Vec3::from(axes.distal);
    let b = cfg.distal_blend.clamp(0.0, 1.0);
    (n * (1.0 - b) + d * b).normalize_or_zero().to_array()
}

#[allow(dead_code)]
pub fn reach_origin(axes: &HandAxes, cfg: &ReachConfig) -> [f32; 3] {
    (Vec3::from(axes.palm_pos) + Vec3::from(axes.distal) * cfg.origin_distal_m).to_array()
}

/// Direction from the palm toward cylinder content (forward −Z).
///
/// Cylinder-local: axis +Y at the user, gap/content forward = −Z. Labeled Open
/// takes were fitted against this (`n·(−Z) ≈ +0.85`). The previous same-azimuth
/// radial `(on_cylinder − palm)` inverted the gate (`n·toward ≈ −0.29` on Open,
/// positive on FaceMe), so the ray only appeared when the palm faced the user.
///
/// For a yawed group, prefer [`toward_cylinder_yaw`]. Mesh-local panels should
/// use [`toward_panel_surface`] instead.
pub fn toward_cylinder_content(palm_pos: [f32; 3], cylinder_radius_m: f32) -> [f32; 3] {
    let _ = (palm_pos, cylinder_radius_m);
    [0.0, 0.0, -1.0]
}

/// Unit forward in cylinder frame at group yaw (gap centre).
pub fn toward_cylinder_yaw(yaw_rad: f32) -> [f32; 3] {
    if yaw_rad.abs() < 1e-6 {
        return toward_cylinder_content([0.0, 0.0, 0.0], 0.7);
    }
    [yaw_rad.sin(), 0.0, -yaw_rad.cos()]
}

/// Mesh-local: panel surface centre minus palm (axis at `(0,0,+radius)`, content near z≈0).
#[allow(dead_code)] // kept for off-axis helpers; live arm gate uses toward_cylinder_content (−Z)
pub fn toward_panel_surface(palm_pos: [f32; 3], surface_point: [f32; 3]) -> [f32; 3] {
    let dir = (Vec3::from(surface_point) - Vec3::from(palm_pos)).normalize_or_zero();
    if dir.length_squared() < 1e-8 {
        [0.0, 0.0, -1.0]
    } else {
        dir.to_array()
    }
}

/// Stable ray origin: palm centre (finger curl must not nudge the tip).
pub fn reach_origin_stable(axes: &HandAxes) -> [f32; 3] {
    axes.palm_pos
}

fn lerp_dir(prev: [f32; 3], next: [f32; 3], alpha: f32) -> [f32; 3] {
    let a = alpha.clamp(0.0, 1.0);
    (Vec3::from(prev) * (1.0 - a) + Vec3::from(next) * a)
        .normalize_or_zero()
        .to_array()
}

pub fn palm_faces_content(axes: &HandAxes, toward_content: [f32; 3], cfg: &ReachConfig) -> bool {
    Vec3::from(axes.palm_normal).dot(Vec3::from(toward_content)) >= cfg.face_dot
}

/// Distal not pointing too straight up (rejects labeled Back of hand).
pub fn distal_not_too_up(axes: &HandAxes, cfg: &ReachConfig) -> bool {
    Vec3::from(axes.distal).y <= cfg.max_distal_up
}

pub fn arm_gate_raw(
    dists: [f32; 4],
    axes: &HandAxes,
    toward_content: [f32; 3],
    pinching: bool,
    cfg: &ReachConfig,
) -> bool {
    arm_gate_break(dists, axes, toward_content, pinching, cfg).ok
}

/// Per-gate breakdown for live diagnosis (extend / face / distal / pinch).
#[derive(Debug, Clone, Copy)]
pub struct GateBreak {
    pub ok: bool,
    pub pinching: bool,
    pub extend: bool,
    pub face: bool,
    pub distal: bool,
    pub mean: f32,
    pub tip_min: f32,
    pub face_dot: f32,
    pub distal_y: f32,
}

pub fn arm_gate_break(
    dists: [f32; 4],
    axes: &HandAxes,
    toward_content: [f32; 3],
    pinching: bool,
    cfg: &ReachConfig,
) -> GateBreak {
    let mean = mean_tip_distance(dists);
    let tip_min = dists
        .iter()
        .copied()
        .min_by(|a, b| a.total_cmp(b))
        .unwrap_or(0.0);
    let face_dot = Vec3::from(axes.palm_normal).dot(Vec3::from(toward_content));
    let distal_y = Vec3::from(axes.distal).y;
    let extend = fingers_extended(dists, cfg);
    let face = palm_faces_content(axes, toward_content, cfg);
    let distal = distal_not_too_up(axes, cfg);
    GateBreak {
        ok: !pinching && extend && face && distal,
        pinching,
        extend,
        face,
        distal,
        mean,
        tip_min,
        face_dot,
        distal_y,
    }
}

#[derive(Debug, Clone)]
pub struct ReachArming {
    pub armed: bool,
    pub shape: PalmShape,
    pub grabbing: bool,
    pub locked_dir: Option<[f32; 3]>,
    pub locked_origin: Option<[f32; 3]>,
    /// True while curl-transition freeze is active.
    pub aim_frozen: bool,
    /// Sticky hand while armed/grabbing so the other hand cannot steal the ray.
    pub active_right: Option<bool>,
    /// Wall time when curl freeze ends (grab aim unlocks after this).
    freeze_until: Option<Instant>,
    gate_since: Option<Instant>,
    cfg: ReachConfig,
}

impl ReachArming {
    pub fn new(cfg: ReachConfig) -> Self {
        Self {
            armed: false,
            shape: PalmShape::Open,
            grabbing: false,
            locked_dir: None,
            locked_origin: None,
            aim_frozen: false,
            active_right: None,
            freeze_until: None,
            gate_since: None,
            cfg,
        }
    }

    pub fn cfg(&self) -> &ReachConfig {
        &self.cfg
    }

    pub fn step(
        &mut self,
        now: Instant,
        dists: [f32; 4],
        axes: &HandAxes,
        toward_content: [f32; 3],
        pinch_gap: f32,
        live_dir: [f32; 3],
        live_origin: [f32; 3],
    ) {
        let mean = mean_tip_distance(dists);
        // Fist classification before pinch so Loose fist (small gap) still grabs.
        self.shape = classify_curl(mean, self.shape, &self.cfg);
        let pinching = pinch_gap < PINCH_M && self.shape != PalmShape::Fist;
        let was_armed = self.armed;

        if pinching {
            self.armed = false;
            self.grabbing = false;
            self.locked_dir = None;
            self.locked_origin = None;
            self.aim_frozen = false;
            self.freeze_until = None;
            self.gate_since = None;
            self.active_right = None;
            self.shape = PalmShape::Open;
            return;
        }

        if self.grabbing {
            if self.shape == PalmShape::Open {
                self.grabbing = false;
                self.locked_dir = None;
                self.locked_origin = None;
                self.aim_frozen = false;
                self.freeze_until = None;
                self.armed = false;
                self.gate_since = None;
            } else {
                // Curl freeze only for a short window; then track live aim so
                // body/head yaw around the cylinder moves the panel.
                let frozen = self
                    .freeze_until
                    .is_some_and(|until| now < until);
                self.aim_frozen = frozen;
                if frozen {
                    self.locked_origin = Some(live_origin);
                } else {
                    let sm = match self.locked_dir {
                        Some(prev) => lerp_dir(prev, live_dir, self.cfg.aim_smooth),
                        None => live_dir,
                    };
                    self.locked_dir = Some(sm);
                    self.locked_origin = Some(live_origin);
                }
            }
            return;
        }

        // Curl into a fist while armed → grab immediately (gate will fail once curled).
        if was_armed && self.shape == PalmShape::Fist {
            self.grabbing = true;
            self.armed = true;
            self.aim_frozen = true;
            self.freeze_until =
                Some(now + Duration::from_millis(self.cfg.curl_freeze_ms.max(1)));
            self.locked_dir = Some(self.locked_dir.unwrap_or(live_dir));
            self.locked_origin = Some(live_origin);
            return;
        }

        let gate = arm_gate_raw(dists, axes, toward_content, false, &self.cfg);
        if gate {
            let since = self.gate_since.get_or_insert(now);
            if now.duration_since(*since) >= Duration::from_millis(self.cfg.arm_hold_ms) {
                self.armed = true;
            }
        } else {
            self.gate_since = None;
            if self.armed {
                let soft_face = Vec3::from(axes.palm_normal).dot(Vec3::from(toward_content))
                    >= self.cfg.face_dot * 0.7;
                let soft_open = mean >= self.cfg.extend_min_m * 0.85;
                let soft_up = Vec3::from(axes.distal).y <= self.cfg.max_distal_up + 0.02;
                if !(soft_face && soft_open && soft_up) {
                    self.armed = false;
                    self.aim_frozen = false;
                }
            }
        }

        if self.armed {
            // Freeze aim as fingers curl so the fist transition can't yank the ray.
            if mean <= self.cfg.curl_freeze_m {
                self.aim_frozen = true;
            }
            if self.aim_frozen {
                if self.locked_dir.is_none() {
                    self.locked_dir = Some(live_dir);
                }
            } else {
                let sm = match self.locked_dir {
                    Some(prev) => lerp_dir(prev, live_dir, self.cfg.aim_smooth),
                    None => live_dir,
                };
                self.locked_dir = Some(sm);
            }
            self.locked_origin = Some(live_origin);
        } else {
            self.locked_dir = None;
            self.locked_origin = None;
            self.aim_frozen = false;
            if !self.grabbing {
                self.active_right = None;
            }
        }
    }

    pub fn suppress_pointer(&self, pinch_gap: f32) -> bool {
        let pinching = pinch_gap < PINCH_M && self.shape != PalmShape::Fist && !self.grabbing;
        if pinching {
            return false;
        }
        self.armed || self.grabbing
    }

    pub fn aim(&self) -> Option<([f32; 3], [f32; 3])> {
        Some((self.locked_origin?, self.locked_dir?))
    }
}


/// Mesh-local full-cylinder azimuth of a ray hit (radians, + = right / +X).
///
/// Frame: axis at `(0, y, +radius)`, panel centre at yaw 0. Unlike a panel UV
/// hit, this still returns a yaw when the ray misses the grabbed panel's arc —
/// so rotating the body left/right can drive slide without visiting the
/// cylinder backside.
pub fn mesh_cylinder_phi_from_ray(
    origin: [f32; 3],
    dir: [f32; 3],
    radius_m: f32,
) -> Option<f32> {
    let r = radius_m.max(0.05);
    let o = Vec3::from(origin);
    let d = Vec3::from(dir).normalize_or_zero();
    if d.length_squared() < 1e-12 {
        return None;
    }
    // Cylinder x² + (z − r)² = r² (axis through (0, *, r)).
    let ox = o.x;
    let oz = o.z - r;
    let dx = d.x;
    let dz = d.z;
    let a = dx * dx + dz * dz;
    if a < 1e-12 {
        return None;
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
    for t_hit in ts {
        if t_hit <= 0.0 || t_hit > REACH_MAX_M {
            continue;
        }
        let hit = o + d * t_hit;
        // Same convention as `CylinderSliceHitTest::mesh_local_point_to_uv`.
        return Some(hit.x.atan2(r - hit.z));
    }
    None
}

#[allow(dead_code)]
pub fn slide_yaw_from_uv_delta(
    yaw0: f32,
    u0: f32,
    u_now: f32,
    width_m: f32,
    radius_m: f32,
) -> f32 {
    let r = radius_m.max(0.05);
    yaw0 + (u0 - u_now) * (width_m / r)
}

pub fn ray_closest_point(
    origin: [f32; 3],
    dir: [f32; 3],
    point: [f32; 3],
) -> Option<([f32; 3], f32)> {
    let o = Vec3::from(origin);
    let d = Vec3::from(dir).normalize_or_zero();
    if d.length_squared() < 1e-12 {
        return None;
    }
    let v = Vec3::from(point) - o;
    let t = v.dot(d).max(0.0).min(REACH_MAX_M);
    let closest = o + d * t;
    Some((closest.to_array(), closest.distance(Vec3::from(point))))
}

pub fn ray_hits_bead(origin: [f32; 3], dir: [f32; 3], bead: [f32; 3], tol_m: f32) -> bool {
    ray_closest_point(origin, dir, bead).is_some_and(|(_, dist)| dist <= tol_m)
}

pub fn palm_debug_enabled() -> bool {
    matches!(
        std::env::var("CYLINDER_PALM_DEBUG").as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reach_origin_still_offsets_along_distal() {
        let a = hand_axes_from_quat([1.0, 2.0, 3.0], [0.0, 0.0, 0.0, 1.0], true);
        let cfg = ReachConfig { origin_distal_m: 0.1, ..ReachConfig::default() };
        let o = Vec3::from(reach_origin(&a, &cfg));
        // Identity quat → distal = −Z
        assert!((o - Vec3::new(1.0, 2.0, 2.9)).length() < 1e-4);
    }

    #[test]
    fn measured_openxr_palm_face_is_neg_y() {
        let a = hand_axes_from_quat([0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0], true);
        assert!((Vec3::from(a.palm_normal) + Vec3::Y).length() < 1e-5);
        assert!((Vec3::from(a.palm_back) - Vec3::Y).length() < 1e-5);
    }

    #[test]
    fn joint_cross_agrees_with_protocol_on_open_fixtures() {
        // Sign convention is validated against labeled Open palm takes (both hands).
        for file in [
            "right_open_palm_aimed_at_panel.json",
            "left_open_palm_aimed_at_panel.json",
        ] {
            let path = format!(
                "{}/tests/fixtures/hand_poses/{file}",
                env!("CARGO_MANIFEST_DIR")
            );
            let v: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            for s in v.as_array().unwrap() {
                let wrist = [
                    s["wrist"]["position"][0].as_f64().unwrap() as f32,
                    s["wrist"]["position"][1].as_f64().unwrap() as f32,
                    s["wrist"]["position"][2].as_f64().unwrap() as f32,
                ];
                let mid = [
                    s["middle"]["metacarpal"]["position"][0].as_f64().unwrap() as f32,
                    s["middle"]["metacarpal"]["position"][1].as_f64().unwrap() as f32,
                    s["middle"]["metacarpal"]["position"][2].as_f64().unwrap() as f32,
                ];
                let idx = [
                    s["index"]["metacarpal"]["position"][0].as_f64().unwrap() as f32,
                    s["index"]["metacarpal"]["position"][1].as_f64().unwrap() as f32,
                    s["index"]["metacarpal"]["position"][2].as_f64().unwrap() as f32,
                ];
                let lit = [
                    s["little"]["metacarpal"]["position"][0].as_f64().unwrap() as f32,
                    s["little"]["metacarpal"]["position"][1].as_f64().unwrap() as f32,
                    s["little"]["metacarpal"]["position"][2].as_f64().unwrap() as f32,
                ];
                let right = s["chirality"].as_str() == Some("right");
                let jn = palm_normal_from_joints(wrist, mid, idx, lit, right);
                let pn = [
                    s["derived"]["palm_normal"][0].as_f64().unwrap() as f32,
                    s["derived"]["palm_normal"][1].as_f64().unwrap() as f32,
                    s["derived"]["palm_normal"][2].as_f64().unwrap() as f32,
                ];
                let dot = Vec3::from(jn).dot(Vec3::from(pn));
                assert!(dot > 0.85, "{file} joint·protocol={dot}");
            }
        }
    }

    #[test]
    fn reach_direction_uses_palm_face_not_back() {
        let a = hand_axes_from_quat([0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0], true);
        let d = Vec3::from(reach_direction(&a, &ReachConfig::default()));
        assert!(d.dot(-Vec3::Y) > 0.7);
    }

    #[test]
    fn mesh_cylinder_phi_tracks_body_yaw() {
        let r = 0.7_f32;
        // Palm near viewer (+Z), ray toward panel centre (−ish Z in mesh → toward z=0).
        let origin = [0.0, 0.0, 0.35];
        let dir = [0.0, 0.0, -1.0];
        let phi0 = mesh_cylinder_phi_from_ray(origin, dir, r).unwrap();
        assert!(phi0.abs() < 0.05, "centre aim phi={phi0}");
        // Aim right (+X).
        let dir_r = [0.4, 0.0, -1.0];
        let phi_r = mesh_cylinder_phi_from_ray(origin, dir_r, r).unwrap();
        assert!(phi_r > 0.1, "right aim should be +phi, got {phi_r}");
        // Aim left.
        let dir_l = [-0.4, 0.0, -1.0];
        let phi_l = mesh_cylinder_phi_from_ray(origin, dir_l, r).unwrap();
        assert!(phi_l < -0.1, "left aim should be -phi, got {phi_l}");
    }

    #[test]
    fn slide_yaw_tracks_uv() {
        let y = slide_yaw_from_uv_delta(0.5, 0.5, 0.7, 1.0, 1.0);
        assert!((y - 0.3).abs() < 1e-4);
    }
}

#[cfg(test)]
mod sim_hands {
    use super::*;
    use serde_json::Value;
    use std::time::{Duration, Instant};

    fn load_fixture(name: &str) -> Value {
        let path = format!(
            "{}/tests/fixtures/hand_poses/{}",
            env!("CARGO_MANIFEST_DIR"),
            name
        );
        let s = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
        serde_json::from_str(&s).expect("json")
    }

    fn sample_to_axes_dists(sample: &Value) -> (HandAxes, [f32; 4], f32) {
        let palm = sample["palm"]["position"].as_array().unwrap();
        let palm_pos = [
            palm[0].as_f64().unwrap() as f32,
            palm[1].as_f64().unwrap() as f32,
            palm[2].as_f64().unwrap() as f32,
        ];
        let o = sample["palm"]["orientation_xyzw"].as_array().unwrap();
        let quat = [
            o[0].as_f64().unwrap() as f32,
            o[1].as_f64().unwrap() as f32,
            o[2].as_f64().unwrap() as f32,
            o[3].as_f64().unwrap() as f32,
        ];
        let right = sample["chirality"].as_str() == Some("right");
        let mut axes = hand_axes_from_quat(palm_pos, quat, right);
        // Prefer recorded derived normal (protocol).
        let n = sample["derived"]["palm_normal"].as_array().unwrap();
        axes.palm_normal = [
            n[0].as_f64().unwrap() as f32,
            n[1].as_f64().unwrap() as f32,
            n[2].as_f64().unwrap() as f32,
        ];
        axes.palm_back = [
            -axes.palm_normal[0],
            -axes.palm_normal[1],
            -axes.palm_normal[2],
        ];
        let d = sample["derived"]["distal_axis"].as_array().unwrap();
        axes.distal = [
            d[0].as_f64().unwrap() as f32,
            d[1].as_f64().unwrap() as f32,
            d[2].as_f64().unwrap() as f32,
        ];
        let tips = sample["derived"]["tip_palm_dists"].as_array().unwrap();
        // Reconstruct tip positions along distal for distance API (only magnitudes matter for curl).
        let dists = [
            tips[0].as_f64().unwrap() as f32,
            tips[1].as_f64().unwrap() as f32,
            tips[2].as_f64().unwrap() as f32,
            tips[3].as_f64().unwrap() as f32,
        ];
        let pinch = sample["derived"]["pinch_gap"].as_f64().unwrap() as f32;
        (axes, dists, pinch)
    }

    /// Content direction used in the recorder (HMD −Z). Matches face_dot fit.
    const CONTENT: [f32; 3] = [0.0, 0.0, -1.0];

    fn arm_rate(file: &str) -> f32 {
        let samples = load_fixture(file);
        let arr = samples.as_array().unwrap();
        let cfg = ReachConfig::default();
        let mut ok = 0;
        for s in arr {
            let (axes, dists, pinch) = sample_to_axes_dists(s);
            let pinching = pinch < PINCH_M
                && classify_curl(mean_tip_distance(dists), PalmShape::Open, &cfg) != PalmShape::Fist;
            if arm_gate_raw(dists, &axes, CONTENT, pinching, &cfg) {
                ok += 1;
            }
        }
        ok as f32 / arr.len() as f32
    }

    fn fist_rate(file: &str) -> f32 {
        let samples = load_fixture(file);
        let arr = samples.as_array().unwrap();
        let cfg = ReachConfig::default();
        let mut ok = 0;
        for s in arr {
            let (_axes, dists, _pinch) = sample_to_axes_dists(s);
            if mean_tip_distance(dists) <= cfg.fist_m {
                ok += 1;
            }
        }
        ok as f32 / arr.len() as f32
    }

    #[test]
    fn fixtures_open_arms_both_hands() {
        assert!(arm_rate("right_open_palm_aimed_at_panel.json") > 0.95);
        assert!(arm_rate("left_open_palm_aimed_at_panel.json") > 0.95);
    }

    #[test]
    fn fixtures_reject_back_faceme_relaxed_point_pinch() {
        for f in [
            "right_back_of_hand.json",
            "left_back_of_hand.json",
            "right_palm_facing_me.json",
            "left_palm_facing_me.json",
            "right_relaxed.json",
            "left_relaxed.json",
            "right_point.json",
            "left_point.json",
            "right_pinch.json",
            "left_pinch.json",
        ] {
            let r = arm_rate(f);
            assert!(r < 0.05, "{f} arm_rate={r}");
        }
    }

    #[test]
    fn fixtures_fist_and_loose_grab_not_relaxed_or_pinch() {
        assert!(fist_rate("right_fist.json") > 0.95);
        assert!(fist_rate("left_fist.json") > 0.95);
        assert!(fist_rate("right_loose_fist.json") > 0.95);
        assert!(fist_rate("left_loose_fist.json") > 0.95);
        assert!(fist_rate("right_relaxed.json") < 0.05);
        assert!(fist_rate("left_relaxed.json") < 0.05);
        assert!(fist_rate("right_pinch.json") < 0.05);
        assert!(fist_rate("left_pinch.json") < 0.05);
    }

    #[test]
    fn open_ray_is_palm_face_not_back() {
        for file in [
            "right_open_palm_aimed_at_panel.json",
            "left_open_palm_aimed_at_panel.json",
        ] {
            let samples = load_fixture(file);
            for s in samples.as_array().unwrap() {
                let (axes, _, _) = sample_to_axes_dists(s);
                let dir = Vec3::from(reach_direction(&axes, &ReachConfig::default()));
                assert!(
                    dir.dot(Vec3::from(axes.palm_normal)) > 0.7,
                    "ray must follow palm face"
                );
                assert!(
                    dir.dot(Vec3::from(axes.palm_back)) < 0.0,
                    "ray must not follow back"
                );
                // Toward content (−Z in recorder frame)
                assert!(dir.dot(Vec3::from(CONTENT)) > 0.3, "ray toward panel");
            }
        }
    }

    #[test]
    fn faceme_normal_opposes_content() {
        for file in ["right_palm_facing_me.json", "left_palm_facing_me.json"] {
            let samples = load_fixture(file);
            for s in samples.as_array().unwrap() {
                let (axes, _, _) = sample_to_axes_dists(s);
                assert!(
                    Vec3::from(axes.palm_normal).dot(Vec3::from(CONTENT)) < 0.0,
                    "FaceMe palm faces user, not panel"
                );
            }
        }
    }

    #[test]
    fn toward_cylinder_content_agrees_with_recorder_forward() {
        // Regression: same-azimuth radial inverted vs labeled Open (n·toward was negative).
        for file in [
            "right_open_palm_aimed_at_panel.json",
            "left_open_palm_aimed_at_panel.json",
        ] {
            let samples = load_fixture(file);
            for s in samples.as_array().unwrap() {
                let (axes, _, _) = sample_to_axes_dists(s);
                let toward = toward_cylinder_content(axes.palm_pos, 0.7);
                let face = Vec3::from(axes.palm_normal).dot(Vec3::from(toward));
                assert!(
                    face > 0.45,
                    "{file} open palm must face toward_cylinder_content, face_dot={face}"
                );
                assert!(
                    Vec3::from(toward).dot(Vec3::from(CONTENT)) > 0.5,
                    "{file} toward must agree with recorder −Z"
                );
            }
        }
        for file in ["right_palm_facing_me.json", "left_palm_facing_me.json"] {
            let samples = load_fixture(file);
            for s in samples.as_array().unwrap() {
                let (axes, _, _) = sample_to_axes_dists(s);
                let toward = toward_cylinder_content(axes.palm_pos, 0.7);
                let face = Vec3::from(axes.palm_normal).dot(Vec3::from(toward));
                assert!(
                    face < 0.2,
                    "{file} FaceMe must not arm via toward_cylinder_content, face_dot={face}"
                );
            }
        }
    }

    #[test]
    fn toward_panel_surface_points_at_mesh_content() {
        let palm = [0.0, 0.0, 0.35];
        let surf = [0.0, 0.0, 0.02];
        let t = Vec3::from(toward_panel_surface(palm, surf));
        assert!(t.z < -0.9, "mesh-local toward panel is −Z, got {t:?}");
    }

    #[test]
    fn curl_freezes_aim_before_fist() {
        let samples = load_fixture("right_open_palm_aimed_at_panel.json");
        let s = &samples.as_array().unwrap()[0];
        let (axes, dists, pinch) = sample_to_axes_dists(s);
        let cfg = ReachConfig::default();
        let mut arm = ReachArming::new(cfg);
        let t0 = Instant::now();
        let dir = reach_direction(&axes, &cfg);
        let origin = reach_origin_stable(&axes);
        arm.step(t0, dists, &axes, CONTENT, pinch, dir, origin);
        arm.step(
            t0 + Duration::from_millis(cfg.arm_hold_ms + 20),
            dists,
            &axes,
            CONTENT,
            pinch,
            dir,
            origin,
        );
        assert!(arm.armed);
        let locked = arm.aim().unwrap().1;
        // Partial curl — below curl_freeze but above fist.
        let curling = [0.09, 0.09, 0.09, 0.09];
        arm.step(
            t0 + Duration::from_millis(cfg.arm_hold_ms + 40),
            curling,
            &axes,
            CONTENT,
            0.05,
            [0.0, 1.0, 0.0],
            origin,
        );
        assert!(arm.aim_frozen);
        assert!(!arm.grabbing);
        assert_eq!(arm.aim().unwrap().1, locked);
        // Fist: direction stays; origin may follow.
        let fist_dists = [0.05, 0.05, 0.05, 0.05];
        let new_origin = [origin[0] + 0.05, origin[1], origin[2]];
        arm.step(
            t0 + Duration::from_millis(cfg.arm_hold_ms + 60),
            fist_dists,
            &axes,
            CONTENT,
            0.05,
            [0.0, 1.0, 0.0],
            new_origin,
        );
        assert!(arm.grabbing);
        assert_eq!(arm.aim().unwrap().1, locked);
        assert_eq!(arm.aim().unwrap().0, new_origin);
    }

    #[test]
    fn arming_hold_and_fist_lock_from_open_fixture() {
        let samples = load_fixture("right_open_palm_aimed_at_panel.json");
        let s = &samples.as_array().unwrap()[0];
        let (axes, dists, pinch) = sample_to_axes_dists(s);
        let cfg = ReachConfig::default();
        let mut arm = ReachArming::new(cfg);
        let t0 = Instant::now();
        let dir = reach_direction(&axes, &cfg);
        let origin = reach_origin_stable(&axes);
        arm.step(t0, dists, &axes, CONTENT, pinch, dir, origin);
        assert!(!arm.armed);
        arm.step(
            t0 + Duration::from_millis(cfg.arm_hold_ms + 20),
            dists,
            &axes,
            CONTENT,
            pinch,
            dir,
            origin,
        );
        assert!(arm.armed);
        let locked = arm.aim().unwrap().1;
        // Curl into fist using loose-fist mean
        let fist_dists = [0.05, 0.05, 0.05, 0.05];
        arm.step(
            t0 + Duration::from_millis(cfg.arm_hold_ms + 40),
            fist_dists,
            &axes,
            CONTENT,
            0.05,
            [0.0, 1.0, 0.0], // would retarget if unlocked
            origin,
        );
        assert!(arm.grabbing);
        assert_eq!(arm.aim().unwrap().1, locked);
    }
}
