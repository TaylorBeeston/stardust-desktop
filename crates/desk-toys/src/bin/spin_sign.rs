//! A slappable spinning sign on a post. Slap it with an open hand (or click/scroll on it
//! with a pointer) and it spins on its post, slowing down and settling on a face.

use desk_toys::{
    env_pos, identity_rot, inertia::{self, Inertia}, init_tracing, pose_predict_dt,
    touch::{Touch, TouchFrame},
    upright,
};
use glam::{Mat4, Quat, Vec3};
use serde::{Deserialize, Serialize};
use stardust_xr_asteroids::{
    ClientState, Context, CustomElement as _, Element, Entity, Migrate, Reify, Tasker,
    Transformable as _,
    components::{Grabbable, PointerMode},
    elements::{Model, Spatial, Text},
};
use stardust_xr_fusion::{
    drawable::{XAlign, YAlign},
    fields::Shape,
    types::{QuatF, Vec3F, rgba_linear},
};
use std::f32::consts::{PI, TAU};
use tracing::info;

const BOARD_R: f32 = 0.06;
const BOARD_T: f32 = 0.0065;
const POST_H: f32 = 0.14;
const CENTER_Y: f32 = POST_H + BOARD_R;
const MAX_SPIN: f32 = 45.0;

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Spin {
    /// board angle about the post (radians, 0 = front faces +Z)
    pub angle: f32,
    /// rad/s
    pub omega: f32,
    /// accumulated angle toward the next full turn
    acc: f32,
    pub spins: u32,
    /// `Context::time()` when `angle`/`omega` were last integrated
    sim_t: f32,
}

impl Spin {
    /// Integrate drag, bearing friction and the face detent (it likes to stop showing a face).
    /// Resting on a face: snap exactly onto it and stop integrating, so a sleeping sign
    /// sends no updates at all.
    fn settle(&mut self) -> bool {
        if self.omega == 0.0 && (2.0 * self.angle).sin().abs() < 0.01 {
            self.angle = ((self.angle / PI).round() * PI).rem_euclid(TAU);
            return true;
        }
        false
    }

    /// Carrying the sign and twisting it: the board sits on a bearing, so it wants to keep its
    /// world orientation; relative to the post it turns the other way (then friction drags it
    /// along, and it keeps going when you stop).
    pub fn carry(&mut self, yaw_accel: f32, dt: f32) {
        self.omega = (self.omega - yaw_accel * dt).clamp(-MAX_SPIN, MAX_SPIN);
    }

    pub fn step(&mut self, dt: f32) {
        if self.settle() {
            return;
        }
        let substeps = (dt / 0.004).ceil().max(1.0) as usize;
        let h = dt / substeps as f32;
        for _ in 0..substeps {
            let damping = if self.omega.abs() < 3.0 { 1.4 } else { 0.3 };
            let friction = 0.8 * (self.omega / 0.3).clamp(-1.0, 1.0);
            let detent = 9.0 * (2.0 * self.angle).sin() / 2.0;
            self.omega += (-damping * self.omega - friction - detent) * h;
            let d = self.omega * h;
            self.angle = (self.angle + d).rem_euclid(TAU);
            self.acc += d;
            if self.acc.abs() >= TAU {
                self.acc -= TAU * self.acc.signum();
                self.spins += 1;
            }
        }
        if self.omega.abs() < 0.02 && (2.0 * self.angle).sin().abs() < 0.01 {
            self.omega = 0.0;
        }
    }

    /// Pose-predicted board angle for display (coasts with current omega).
    pub fn display_angle(&self, predict_dt: f32) -> f32 {
        inertia::coast(self.angle, self.omega, predict_dt).rem_euclid(TAU)
    }

    fn to_board(&self, p: Vec3) -> Vec3 {
        Quat::from_rotation_y(-self.angle) * (p - Vec3::Y * CENTER_Y)
    }

    fn on_board(q: Vec3, r: f32) -> bool {
        q.x.hypot(q.y) < BOARD_R + r * 0.5
    }

    /// Hands/controllers bump the board: anything moving into a face drags the board
    /// along at its own angular rate about the post.
    pub fn touch(&mut self, frame: &TouchFrame) -> bool {
        let mut best: Option<f32> = None;
        for c in &frame.contacts {
            let q = self.to_board(c.pos);
            let qp = self.to_board(c.pos - c.vel * frame.dt);
            if !Self::on_board(q, c.radius) {
                continue;
            }
            let band = BOARD_T / 2.0 + c.radius;
            let side = if q.z.abs() < band {
                // already inside: trust where it is now; just arrived: where it came from
                if qp.z.abs() < band { q.z.signum() } else { qp.z.signum() }
            } else if q.z.signum() != qp.z.signum() && qp.z.abs() < 0.2 {
                // moved clean through the board in one frame
                qp.z.signum()
            } else {
                continue;
            };
            let rel = c.pos - Vec3::Y * CENTER_Y;
            let r2 = rel.x * rel.x + rel.z * rel.z;
            if r2 < 0.015 * 0.015 {
                continue;
            }
            let hand_omega = (rel.z * c.vel.x - rel.x * c.vel.z) / r2;
            if (hand_omega - self.omega) * q.x * side <= 0.0 {
                continue; // moving away from / along the face
            }
            if best.is_none_or(|b| (hand_omega - self.omega).abs() > (b - self.omega).abs()) {
                best = Some(hand_omega);
            }
        }
        let Some(hand_omega) = best else { return false };
        // a slap gives a little extra, it's a toy
        self.omega += (hand_omega * 1.15 - self.omega) * 0.85;
        self.omega = self.omega.clamp(-MAX_SPIN, MAX_SPIN);
        true
    }

    /// Pointer flick: click on the board pushes that face away from you; scroll spins too.
    pub fn poke(&mut self, frame: &TouchFrame) -> bool {
        let mut hit = false;
        for ray in &frame.rays {
            if ray.select_started && ray.hit() {
                let q = self.to_board(ray.point());
                let side = self.to_board(ray.origin).z.signum();
                let x = if q.x.abs() < 0.005 { 1.0 } else { q.x.signum() };
                self.omega = (self.omega + 18.0 * x * side).clamp(-MAX_SPIN, MAX_SPIN);
                hit = true;
            }
            if ray.hit() && ray.scroll != 0.0 {
                self.omega = (self.omega + ray.scroll * 1.5).clamp(-MAX_SPIN, MAX_SPIN);
                hit = true;
            }
        }
        hit
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct State {
    pos: Vec3F,
    #[serde(default = "identity_rot")]
    rot: QuatF,
    #[serde(skip)]
    spin: Spin,
    #[serde(skip)]
    inertia: Inertia,
}
impl Default for State {
    fn default() -> Self {
        State { pos: [0.0; 3].into(), rot: identity_rot(), spin: Spin::default(), inertia: Inertia::default() }
    }
}
impl Migrate for State {
    type Old = State;
}
impl ClientState for State {
    const APP_ID: &'static str = "io.github.TaylorBeeston.SpinSign";
    fn initial_state_update(&mut self) {
        if let Some(pos) = env_pos("SPIN_SIGN_POS") {
            self.pos = pos;
        }
    }
}

fn on_frame(state: &mut State, frame: &TouchFrame) {
    state.inertia.sample(Vec3::from(state.pos), Quat::from(state.rot), frame.dt);
    let spin = &mut state.spin;
    spin.carry(state.inertia.yaw(), frame.dt);
    let before = spin.spins;
    if !frame.grabbed && (spin.touch(frame) | spin.poke(frame)) {
        info!(omega = spin.omega, "slapped");
    }
    // Integrate with FrameInfo.delta, then stamp sim time for declarative pose predict.
    spin.step(frame.dt);
    spin.sim_t = frame.display_t;
    if spin.spins != before && spin.spins % 10 == 0 {
        info!(spins = spin.spins, "spin milestone");
    }
}

impl Reify for State {
    fn reify(&self, context: &Context, _tasks: impl Tasker<Self>, _props: ()) -> impl Element<Self> {
        let top = CENTER_Y + BOARD_R + 0.01;
        let field = Shape::Transform {
            shape: Box::new(Shape::Cylinder { length: top, radius: BOARD_R + 0.01 }),
            transform: Mat4::from_translation(Vec3::Y * top / 2.0).into(),
        };
        let spins = self.spin.spins;
        let angle = self.spin.display_angle(pose_predict_dt(context, self.spin.sim_t));
        Entity::new(field)
            .pos(self.pos)
            .rot(self.rot)
            .component((
                Grabbable::new(|state: &mut State, pose| {
                    state.pos = pose.position;
                    state.rot = upright(pose.orientation);
                })
                .max_distance(0.02)
                .pointer_mode(PointerMode::Align),
                Touch::new(on_frame),
            ))
            .build()
            // models from tools/gen_models.py: textured faces, gold rim, chrome post
            .child(Model::namespaced("io.github.TaylorBeeston.DeskToys", "sign_stand").build())
            .child(
                Spatial::default()
                    .pos([0.0, CENTER_Y, 0.0])
                    .rot(Quat::from_rotation_y(angle))
                    .build()
                    .child(Model::namespaced("io.github.TaylorBeeston.DeskToys", "sign_board").build()),
            )
            .maybe_child((spins > 0).then(|| {
                Text::new(format!("{spins}"))
                    .character_height(0.009)
                    .color(rgba_linear!(1.0, 0.8, 0.3, 1.0))
                    .align_x(XAlign::Center)
                    .align_y(YAlign::Bottom)
                    .pos([0.0, 0.0015, 0.0405])
                    .build()
            }))
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    desk_toys::migrate_app_config("org.stardustxr.SpinSign", "io.github.TaylorBeeston.SpinSign");
    init_tracing("spin_sign=info");
    stardust_xr_asteroids::client::run::<State>(&[&stardust_xr_fusion::project_local_resources!("data")])
        .await
        .unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    use desk_toys::touch::Contact;

    fn slap(x: f32, vz: f32) -> TouchFrame {
        touch_at(x, 0.01, vz)
    }

    fn touch_at(x: f32, z: f32, vz: f32) -> TouchFrame {
        TouchFrame {
            dt: 1.0 / 90.0,
            contacts: vec![Contact {
                pos: Vec3::new(x, CENTER_Y, z),
                vel: Vec3::new(0.0, 0.0, vz),
                radius: 0.01,
                pinch: None,
                poker: false,
                left: None,
            }],
            ..Default::default()
        }
    }

    #[test]
    fn slap_on_right_half_from_front_spins_positive_then_settles_on_a_face() {
        let mut s = Spin::default();
        assert!(s.touch(&slap(0.04, -2.5)));
        assert!(s.omega > 30.0, "omega {}", s.omega);
        for _ in 0..(90 * 30) {
            s.step(1.0 / 90.0);
        }
        assert!(s.spins >= 3, "spins {}", s.spins);
        assert_eq!(s.omega, 0.0);
        let face = (s.angle / PI).round() * PI;
        assert!((s.angle - face).abs() < 0.02, "angle {}", s.angle);
    }

    #[test]
    fn hand_moving_away_from_the_face_does_nothing() {
        let mut s = Spin::default();
        assert!(!s.touch(&touch_at(0.04, 0.012, 0.5)));
        assert_eq!(s.omega, 0.0);
    }

    #[test]
    fn twisting_the_sign_while_carrying_it_spins_the_board() {
        let mut s = Spin::default();
        // twist the post +Y at 40 rad/s² for a quarter second, then stop
        for _ in 0..22 {
            s.carry(40.0, 1.0 / 90.0);
            s.step(1.0 / 90.0);
        }
        // while you twist, the board lags behind the post…
        assert!(s.omega < -3.0, "omega {}", s.omega);
        for _ in 0..22 {
            s.carry(-40.0, 1.0 / 90.0);
            s.step(1.0 / 90.0);
        }
        // …and friction dragged it along, so once the post stops it keeps going your way
        assert!(s.omega > 0.5, "omega {}", s.omega);
    }

    #[test]
    fn a_settled_sign_stays_exactly_still() {
        let mut s = Spin::default();
        s.angle = PI + 0.003;
        s.step(1.0 / 90.0);
        assert_eq!(s.angle, PI);
        let before = s;
        for _ in 0..90 {
            s.step(1.0 / 90.0);
        }
        assert_eq!(s, before);
    }

    #[test]
    fn left_half_spins_the_other_way() {
        let mut s = Spin::default();
        s.touch(&slap(-0.04, -2.5));
        assert!(s.omega < -30.0);
    }

    /// Integrating the same elapsed time at 30Hz vs 90Hz must land near the same angle.
    #[test]
    fn spin_step_is_frame_rate_independent() {
        let run = |hz: f32| {
            let mut s = Spin::default();
            s.omega = 12.0;
            let total = 0.25_f32;
            let n = (total * hz).round() as u32;
            let dt = total / n as f32;
            for _ in 0..n {
                s.step(dt);
            }
            (s.angle, s.omega)
        };
        let (a30, w30) = run(30.0);
        let (a90, w90) = run(90.0);
        let (a144, w144) = run(144.0);
        // angles are rem_euclid; compare wrapped
        let wrap = |a: f32, b: f32| {
            let d = (a - b + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
            d.abs()
        };
        let d_30_90 = wrap(a30, a90);
        let d_90_144 = wrap(a90, a144);
        println!("angles 30/90/144: {a30} {a90} {a144}; wrap Δ {d_30_90} {d_90_144}; ω {w30} {w90} {w144}");
        assert!(d_30_90 < 0.05, "30 vs 90 angle drift {d_30_90}");
        assert!(d_90_144 < 0.05, "90 vs 144 angle drift {d_90_144}");
        assert!((w30 - w90).abs() < 0.5, "30 vs 90 omega drift {}", (w30 - w90).abs());
    }

    #[test]
    fn display_angle_coasts_with_omega() {
        let mut s = Spin::default();
        s.angle = 1.0;
        s.omega = 2.0;
        let shown = s.display_angle(0.01);
        assert!((shown - 1.02).abs() < 1e-5, "shown {shown}");
    }
}
