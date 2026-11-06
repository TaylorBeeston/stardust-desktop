//! Open-palm window management helpers (pure — unit-tested).
//!
//! Open palm facing a panel → highlight. Curl to fist → grab; panel follows
//! along the cylinder. Open hand → settle into nearest slot. Pinch stays click
//! (caller must suppress palm WM while pinching).

/// Mean fingertip–palm distance (metres). Larger = more open.
pub fn hand_openness(
    palm: [f32; 3],
    tips: [[f32; 3]; 4], // index, middle, ring, little
) -> f32 {
    let p = glam::Vec3::from(palm);
    tips.iter()
        .map(|t| p.distance(glam::Vec3::from(*t)))
        .sum::<f32>()
        / 4.0
}

pub const OPEN_M: f32 = 0.055;
pub const FIST_M: f32 = 0.032;
/// Thumb–index gap below this → treat as pinch (suppress palm WM).
pub const PINCH_M: f32 = 0.022;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PalmShape {
    Open,
    Fist,
}

/// Hysteresis so mid-band doesn't flicker during a pinch transition.
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

/// Panel centre yaw while dragging so the grabbed UV stays under the ray.
/// `u` increases toward +mesh; +yaw is right on the cylinder.
pub fn slide_yaw_from_uv_delta(
    yaw0: f32,
    u0: f32,
    u_now: f32,
    width_m: f32,
    radius_m: f32,
) -> f32 {
    let r = radius_m.max(0.05);
    // Moving hand to +u should rotate panel so content follows → centre yaw decreases.
    yaw0 + (u0 - u_now) * (width_m / r)
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
    fn slide_yaw_tracks_uv() {
        let y = slide_yaw_from_uv_delta(0.5, 0.5, 0.7, 1.0, 1.0);
        assert!((y - 0.3).abs() < 1e-4, "{y}");
    }
}
