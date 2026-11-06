//! Open-palm "extend my reach" — fitted from labeled `hand-pose-recorder` takes (2026-11-06).
//!
//! ## Measured (protocol + labeled JSONL)
//! - Palm **face** normal = quat × **−Y** (`Hand::palm_normal`); agrees with wrist→middle ×
//!   index→little (sign by chirality) at n·jn ≈ 0.97. Palm **back** = +Y (9564d90 bug).
//! - Activation (held-out 30%): Open palm arms 100%; FaceMe / Back / Relaxed / Point / Pinch 0%.
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
}

impl Default for ReachConfig {
    fn default() -> Self {
        // Fitted 2026-11-06 from right+left labeled takes.
        Self {
            extend_m: 0.112,
            extend_min_m: 0.092,
            fist_m: 0.062,
            fist_open_m: 0.090,
            face_dot: 0.50,
            max_distal_up: 0.915,
            arm_hold_ms: 150,
            distal_blend: 0.25,
            origin_distal_m: 0.02,
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

pub fn reach_origin(axes: &HandAxes, cfg: &ReachConfig) -> [f32; 3] {
    (Vec3::from(axes.palm_pos) + Vec3::from(axes.distal) * cfg.origin_distal_m).to_array()
}

pub fn toward_cylinder_content(palm_pos: [f32; 3], cylinder_radius_m: f32) -> [f32; 3] {
    let palm = Vec3::from(palm_pos);
    let radial = Vec3::new(palm.x, 0.0, palm.z);
    if radial.length_squared() < 1e-8 {
        return [0.0, 0.0, -1.0];
    }
    let on = radial.normalize() * cylinder_radius_m.max(0.05);
    (Vec3::new(on.x, palm.y, on.z) - palm).normalize_or_zero().to_array()
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
    if pinching {
        return false;
    }
    fingers_extended(dists, cfg)
        && palm_faces_content(axes, toward_content, cfg)
        && distal_not_too_up(axes, cfg)
}

#[derive(Debug, Clone)]
pub struct ReachArming {
    pub armed: bool,
    pub shape: PalmShape,
    pub grabbing: bool,
    pub locked_dir: Option<[f32; 3]>,
    pub locked_origin: Option<[f32; 3]>,
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
            self.gate_since = None;
            self.shape = PalmShape::Open;
            return;
        }

        if self.grabbing {
            if self.shape == PalmShape::Open {
                self.grabbing = false;
                self.locked_dir = None;
                self.locked_origin = None;
                self.armed = false;
                self.gate_since = None;
            }
            return;
        }

        // Curl into a fist while armed → grab immediately (gate will fail once curled).
        if was_armed && self.shape == PalmShape::Fist {
            self.grabbing = true;
            self.armed = true;
            self.locked_dir = Some(self.locked_dir.unwrap_or(live_dir));
            self.locked_origin = Some(self.locked_origin.unwrap_or(live_origin));
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
                }
            }
        }

        if self.armed {
            self.locked_dir = Some(live_dir);
            self.locked_origin = Some(live_origin);
        } else {
            self.locked_dir = None;
            self.locked_origin = None;
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
    fn arming_hold_and_fist_lock_from_open_fixture() {
        let samples = load_fixture("right_open_palm_aimed_at_panel.json");
        let s = &samples.as_array().unwrap()[0];
        let (axes, dists, pinch) = sample_to_axes_dists(s);
        let cfg = ReachConfig::default();
        let mut arm = ReachArming::new(cfg);
        let t0 = Instant::now();
        let dir = reach_direction(&axes, &cfg);
        let origin = reach_origin(&axes, &cfg);
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
