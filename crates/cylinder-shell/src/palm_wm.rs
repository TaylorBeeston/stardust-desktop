//! Open-palm "extend my reach" helpers (pure — unit-tested).
//!
//! Open palm → aim a ray (suppress panel mouse). Fist → grab whatever the ray hits
//! (panel slide/settle, or cylinder bead group-grab). Pinch stays click.

use glam::Vec3;

/// Mean fingertip–palm distance (metres). Larger = more open.
pub fn hand_openness(palm: [f32; 3], tips: [[f32; 3]; 4]) -> f32 {
    let p = Vec3::from(palm);
    tips.iter().map(|t| p.distance(Vec3::from(*t))).sum::<f32>() / 4.0
}

pub const OPEN_M: f32 = 0.055;
pub const FIST_M: f32 = 0.032;
/// Thumb–index gap below this → pinch (click path; suppress palm reach).
pub const PINCH_M: f32 = 0.022;
/// Max reach ray length (metres).
pub const REACH_MAX_M: f32 = 2.5;
/// Bead aim tolerance along a reach ray (metres).
pub const BEAD_RAY_HIT_M: f32 = 0.08;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PalmShape {
    Open,
    Fist,
}

pub fn classify_palm(openness: f32, prev: PalmShape) -> PalmShape {
    match prev {
        PalmShape::Fist => {
            if openness >= OPEN_M {
                PalmShape::Open
            } else {
                PalmShape::Fist
            }
        }
        PalmShape::Open => {
            if openness <= FIST_M {
                PalmShape::Fist
            } else {
                PalmShape::Open
            }
        }
    }
}

/// While open-palm aiming or fist-dragging, Hand must not drive wl_pointer motion/buttons.
pub fn suppress_hand_pointer(shape: PalmShape, pinching: bool, grabbing: bool) -> bool {
    if pinching {
        return false; // pinch owns click
    }
    grabbing || shape == PalmShape::Open
}

/// Candidate aim directions from an open palm (mesh- or cylinder-local).
/// Primary: palm +Y (OpenXR: out of palm). Fallbacks cover awkward orientations
/// and "point at cylinder surface from inside the ring".
pub fn reach_ray_dirs(palm_pos: [f32; 3], palm_quat_xyzw: [f32; 4], cylinder_radius_m: f32) -> [[f32; 3]; 4] {
    let q = glam::Quat::from_xyzw(
        palm_quat_xyzw[0],
        palm_quat_xyzw[1],
        palm_quat_xyzw[2],
        palm_quat_xyzw[3],
    );
    let palm = Vec3::from(palm_pos);
    let out = (q * Vec3::Y).normalize_or_zero();
    let forward = (q * -Vec3::Z).normalize_or_zero();
    let back = (q * Vec3::Z).normalize_or_zero();
    // Toward cylinder surface at same height (works when palm is inside the ring).
    let radial = Vec3::new(palm.x, 0.0, palm.z);
    let to_surface = if radial.length_squared() > 1e-6 {
        let on = radial.normalize() * cylinder_radius_m.max(0.05);
        (Vec3::new(on.x, palm.y, on.z) - palm).normalize_or_zero()
    } else {
        out
    };
    [out.to_array(), forward.to_array(), back.to_array(), to_surface.to_array()]
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

/// Closest approach of ray (origin + t*dir, t≥0) to `point`; None if behind origin.
pub fn ray_closest_point(origin: [f32; 3], dir: [f32; 3], point: [f32; 3]) -> Option<([f32; 3], f32)> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openness_open_vs_fist() {
        let palm = [0.0, 0.0, 0.0];
        let open_tips = [
            [0.0, 0.08, 0.0],
            [0.0, 0.09, 0.0],
            [0.0, 0.08, 0.0],
            [0.0, 0.07, 0.0],
        ];
        let fist_tips = [
            [0.0, 0.02, 0.0],
            [0.0, 0.02, 0.0],
            [0.0, 0.015, 0.0],
            [0.0, 0.015, 0.0],
        ];
        assert!(hand_openness(palm, open_tips) > OPEN_M);
        assert!(hand_openness(palm, fist_tips) < FIST_M);
    }

    #[test]
    fn hysteresis_holds_mid_band() {
        assert_eq!(classify_palm(0.04, PalmShape::Fist), PalmShape::Fist);
        assert_eq!(classify_palm(0.04, PalmShape::Open), PalmShape::Open);
        assert_eq!(classify_palm(0.06, PalmShape::Fist), PalmShape::Open);
        assert_eq!(classify_palm(0.02, PalmShape::Open), PalmShape::Fist);
    }

    #[test]
    fn suppress_when_open_or_grabbing_not_when_pinch() {
        assert!(suppress_hand_pointer(PalmShape::Open, false, false));
        assert!(suppress_hand_pointer(PalmShape::Fist, false, true));
        assert!(!suppress_hand_pointer(PalmShape::Open, true, false));
        assert!(!suppress_hand_pointer(PalmShape::Fist, false, false));
    }

    #[test]
    fn slide_yaw_tracks_uv() {
        let y = slide_yaw_from_uv_delta(0.5, 0.5, 0.7, 1.0, 1.0);
        assert!((y - 0.3).abs() < 1e-4, "{y}");
    }

    #[test]
    fn reach_ray_hits_bead_ahead() {
        let origin = [0.0, 0.0, 0.0];
        let dir = [0.0, 0.0, -1.0];
        let bead = [0.02, 0.0, -0.7];
        assert!(ray_hits_bead(origin, dir, bead, BEAD_RAY_HIT_M));
        assert!(!ray_hits_bead(origin, dir, [0.5, 0.0, -0.7], BEAD_RAY_HIT_M));
    }

    #[test]
    fn open_palm_over_panel_does_not_drive_mouse() {
        // Regression narrative: open + not pinching → suppress.
        let shape = classify_palm(0.08, PalmShape::Open);
        assert_eq!(shape, PalmShape::Open);
        assert!(suppress_hand_pointer(shape, false, false));
    }
}

#[cfg(test)]
mod sim_hands {
    use super::*;

/// Scripted palm-reach controller transitions (headless stand-in for simulated hands).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReachEvent {
    AimMiss,
    AimHit,
    Fist,
    Open,
    Pinch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ReachState {
    pub shape: PalmShape,
    pub highlight: bool,
    pub grabbing: bool,
}

impl ReachState {
    pub fn new() -> Self {
        Self {
            shape: PalmShape::Open,
            highlight: false,
            grabbing: false,
        }
    }

    pub fn step(&mut self, ev: ReachEvent) -> bool {
        // Returns whether hand→mouse should be suppressed after this step.
        let pinching = matches!(ev, ReachEvent::Pinch);
        let openness = match ev {
            ReachEvent::Fist => FIST_M - 0.005,
            ReachEvent::Pinch => FIST_M, // closed-ish but pinch wins
            _ => OPEN_M + 0.01,
        };
        self.shape = if pinching {
            self.shape
        } else {
            classify_palm(openness, self.shape)
        };
        if pinching {
            if self.grabbing {
                self.grabbing = false;
            }
            self.highlight = false;
            return false;
        }
        match (self.grabbing, self.shape, ev) {
            (false, PalmShape::Open, ReachEvent::AimHit) => {
                self.highlight = true;
            }
            (false, PalmShape::Open, ReachEvent::AimMiss) => {
                self.highlight = false;
            }
            (false, PalmShape::Fist, _) if self.highlight => {
                self.grabbing = true;
            }
            (true, PalmShape::Fist, _) => {}
            (true, PalmShape::Open, _) => {
                self.grabbing = false;
                self.highlight = matches!(ev, ReachEvent::AimHit | ReachEvent::Open);
            }
            _ => {}
        }
        suppress_hand_pointer(self.shape, false, self.grabbing)
    }
}


    #[test]
    fn palm_open_over_panel_suppresses_mouse_no_grab() {
        let mut s = ReachState::new();
        assert!(s.step(ReachEvent::AimMiss));
        assert!(!s.highlight);
        assert!(s.step(ReachEvent::AimHit));
        assert!(s.highlight);
        assert!(!s.grabbing);
        assert!(suppress_hand_pointer(s.shape, false, s.grabbing));
    }

    #[test]
    fn fist_grabs_then_open_settles_panel_slide_path() {
        let mut s = ReachState::new();
        assert!(s.step(ReachEvent::AimHit));
        assert!(s.step(ReachEvent::Fist));
        assert!(s.grabbing);
        // Slide yaw helper still works while grabbing.
        let yaw = slide_yaw_from_uv_delta(0.5, 0.5, 0.7, 1.0, 1.0);
        assert!((yaw - 0.3).abs() < 1e-4);
        assert!(s.step(ReachEvent::Open));
        assert!(!s.grabbing);
    }

    #[test]
    fn fist_ray_grabs_distant_bead() {
        // Simulated: open aims at bead 0.7m ahead, fist grabs.
        let origin = [0.0, 0.1, 0.0];
        let dir = [0.0, 0.0, -1.0];
        let bead = [0.01, 0.1, -0.7];
        assert!(ray_hits_bead(origin, dir, bead, BEAD_RAY_HIT_M));
        let mut s = ReachState::new();
        assert!(s.step(ReachEvent::AimHit));
        assert!(s.step(ReachEvent::Fist));
        assert!(s.grabbing);
        assert!(s.step(ReachEvent::Open));
        assert!(!s.grabbing);
    }

    #[test]
    fn pinch_clears_reach_and_allows_click() {
        let mut s = ReachState::new();
        assert!(s.step(ReachEvent::AimHit));
        assert!(!s.step(ReachEvent::Pinch)); // suppress off → click path
        assert!(!s.highlight);
        assert!(!s.grabbing);
    }
}
