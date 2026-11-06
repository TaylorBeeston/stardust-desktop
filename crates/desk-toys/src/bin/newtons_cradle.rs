//! A Newton's cradle. Pinch a ball (or click-drag it with a pointer), pull it back and let go;
//! or just bat the balls with your hand. Elastic sequential-impulse collisions between
//! pendulums, so 1, 2 or 3 balls in means 1, 2 or 3 balls out.

use palm_reach::PalmGrabbable;
use desk_toys::{
    env_pos, identity_rot,
    inertia::Inertia,
    init_tracing,
    touch::{Touch, TouchFrame},
    upright,
};
use glam::{Mat4, Quat, Vec3};
use serde::{Deserialize, Serialize};
use stardust_xr_asteroids::{
    ClientState, Context, CustomElement as _, Element, Entity, Migrate, Reify, Tasker,
    Transformable as _,
    components::{Grabbable, PointerMode},
    elements::{Lines, Model},
};
use stardust_xr_fusion::{
    drawable::Line,
    fields::Shape,
    types::{QuatF, Vec3F, rgba_linear},
};
use stardust_xr_molecules::lines::{LineExt, line_from_points};
use tracing::info;

pub const N: usize = 5;
const R: f32 = 0.012;
const GAP: f32 = 0.0003;
const SPACING: f32 = 2.0 * R + GAP;
const L: f32 = 0.10;
const PIVOT_Y: f32 = 0.15;
const BAR_Z: f32 = 0.028;
/// furthest a ball may be pulled back; the frame is wide enough to clear it
const MAX_ANGLE: f32 = 0.9;
// 2 * (outer rest x + L*sin(MAX_ANGLE) + R + clearance)
const WIDTH: f32 = 0.30;
const G: f32 = 9.81;
const DAMP: f32 = 0.05;
const PIVOT_FRICTION: f32 = 0.12;
const RESTITUTION: f32 = 0.985;
const CONTACT: f32 = 0.0006;
const MAX_SPEED: f32 = 3.0;
// Preserve the old sub-millisecond collision resolution, independent of display rate.
const PHYSICS_DT: f64 = 1.0 / 1440.0;

fn rest_x(i: usize) -> f32 {
    (i as f32 - (N - 1) as f32 / 2.0) * SPACING
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HeldBy {
    Pinch,
    Ray,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Cradle {
    pub phi: [f32; N],
    pub omega: [f32; N],
    pub held: Option<(usize, HeldBy)>,
    accumulator: f64,
}

impl Cradle {
    pub fn center(&self, i: usize) -> Vec3 {
        Self::center_at(i, self.phi[i])
    }

    fn center_at(i: usize, phi: f32) -> Vec3 {
        let (s, c) = phi.sin_cos();
        Vec3::new(rest_x(i) + L * s, PIVOT_Y - L * c, 0.0)
    }

    /// Collision-constrained poses must not coast independently past the next impact.
    /// Clamp the render horizon to zero; physics stays authoritative at contact.
    pub fn display_center(&self, i: usize, _predict_dt: f32) -> Vec3 {
        self.center(i)
    }

    pub fn display_phi(&self, i: usize, _predict_dt: f32) -> f32 {
        self.phi[i]
    }

    fn angle_toward(i: usize, p: Vec3) -> f32 {
        (p.x - rest_x(i))
            .atan2(PIVOT_Y - p.y)
            .clamp(-MAX_ANGLE, MAX_ANGLE)
    }

    /// Settled: nothing held, nothing shaking, everything (nearly) still. Snaps to exact rest
    /// so a sleeping cradle sends no updates at all.
    fn settle(&mut self, target: Option<(usize, f32)>, accel: Vec3) -> bool {
        let still = target.is_none()
            && accel == Vec3::ZERO
            && self.phi.iter().all(|p| p.abs() < 0.003)
            && self.omega.iter().all(|w| w.abs() < 0.02);
        if still {
            self.phi = [0.0; N];
            self.omega = [0.0; N];
        }
        still
    }

    /// Advance the pendulums; `target` drags one (held) ball toward an angle; `accel` is the
    /// frame's own acceleration in toy space (you carrying/shaking it), felt as a pseudo-force.
    pub fn step(&mut self, dt: f32, target: Option<(usize, f32)>, accel: Vec3) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        if self.settle(target, accel) {
            self.accumulator = 0.0;
            return;
        }
        // effective gravity in the toy's frame: real gravity minus the frame's acceleration
        let (gx, gy) = (-accel.x, -G - accel.y);
        self.accumulator += f64::from(dt.min(0.1));
        let h = PHYSICS_DT as f32;
        let held = target.map(|(i, _)| i);
        while self.accumulator + 1e-7 >= PHYSICS_DT {
            self.accumulator -= PHYSICS_DT;
            for i in 0..N {
                match target {
                    Some((j, angle)) if j == i => {
                        let next = self.phi[i] + (angle - self.phi[i]) * (h * 25.0).min(1.0);
                        self.omega[i] = (next - self.phi[i]) / h;
                        self.phi[i] = next;
                    }
                    _ => {
                        let (s, c) = self.phi[i].sin_cos();
                        // air drag + a little pivot friction, which is what finally stops a
                        // real cradle's last tiny sway (and lets this one go to sleep)
                        let pivot = PIVOT_FRICTION * (self.omega[i] / 0.05).clamp(-1.0, 1.0);
                        self.omega[i] += ((gx * c + gy * s) / L - DAMP * self.omega[i] - pivot) * h;
                        self.phi[i] += self.omega[i] * h;
                        // hard stop just inside the frame legs
                        if self.phi[i].abs() > MAX_ANGLE + 0.1 {
                            self.phi[i] = (MAX_ANGLE + 0.1) * self.phi[i].signum();
                            self.omega[i] *= -0.3;
                        }
                    }
                }
            }
            self.collide(held);
        }
    }

    fn speed(&self, i: usize) -> f32 {
        L * self.omega[i] * self.phi[i].cos()
    }
    fn set_speed(&mut self, i: usize, v: f32) {
        let v = v.clamp(-MAX_SPEED, MAX_SPEED);
        self.omega[i] = v / (L * self.phi[i].cos().max(0.2));
    }
    fn shift(&mut self, i: usize, dx: f32) {
        self.phi[i] += dx / (L * self.phi[i].cos().max(0.2));
    }

    /// Equal masses on a line: sequential impulses, swept until nothing is approaching.
    fn collide(&mut self, held: Option<usize>) {
        for _ in 0..N * 2 {
            let mut any = false;
            for i in 0..N - 1 {
                let j = i + 1;
                let gap = self.center(j).x - self.center(i).x - 2.0 * R;
                if gap > CONTACT {
                    continue;
                }
                let (vi, vj) = (self.speed(i), self.speed(j));
                if vi - vj > 1e-4 {
                    any = true;
                    let e = RESTITUTION;
                    match (held == Some(i), held == Some(j)) {
                        (true, _) => self.set_speed(j, vi + e * (vi - vj)),
                        (_, true) => self.set_speed(i, vj - e * (vi - vj)),
                        _ => {
                            let mid = (vi + vj) / 2.0;
                            self.set_speed(i, mid - e * (vi - vj) / 2.0);
                            self.set_speed(j, mid + e * (vi - vj) / 2.0);
                        }
                    }
                }
                if gap < 0.0 {
                    match (held == Some(i), held == Some(j)) {
                        (true, _) => self.shift(j, -gap),
                        (_, true) => self.shift(i, gap),
                        _ => {
                            self.shift(i, gap / 2.0);
                            self.shift(j, -gap / 2.0);
                        }
                    }
                }
            }
            if !any {
                break;
            }
        }
    }

    /// Hands bat balls; pinches and pointer clicks pick one up. Returns the held target.
    pub fn interact(&mut self, frame: &TouchFrame) -> Option<(usize, f32)> {
        // keep or drop the current hold
        if let Some((i, by)) = self.held {
            let c = self.center(i);
            let target = match by {
                HeldBy::Pinch => frame
                    .contacts
                    .iter()
                    .filter_map(|k| k.pinch)
                    .find(|p| p.distance(c) < 0.08),
                HeldBy::Ray => frame
                    .rays
                    .iter()
                    .find(|r| r.select && r.dir.z.abs() > 1e-3)
                    .map(|r| r.origin + r.dir * (-r.origin.z / r.dir.z)),
            };
            match target {
                Some(p) => return Some((i, Self::angle_toward(i, p))),
                None => {
                    info!(ball = i, "let go");
                    self.held = None;
                }
            }
        }
        // pick up
        for k in &frame.contacts {
            if let Some(p) = k.pinch {
                if let Some(i) = (0..N).find(|&i| p.distance(self.center(i)) < R + 0.012) {
                    info!(ball = i, "pinched");
                    self.held = Some((i, HeldBy::Pinch));
                    return Some((i, Self::angle_toward(i, p)));
                }
            }
        }
        for r in frame.rays.iter().filter(|r| r.select_started) {
            let near = (0..N)
                .map(|i| {
                    let d = self.center(i) - r.origin;
                    (i, (d - r.dir * d.dot(r.dir)).length())
                })
                .filter(|&(_, miss)| miss < R + 0.006)
                .min_by(|a, b| a.1.total_cmp(&b.1));
            if let Some((i, _)) = near {
                info!(ball = i, "grabbed by pointer");
                self.held = Some((i, HeldBy::Ray));
                return None;
            }
        }
        // bat
        for k in &frame.contacts {
            for i in 0..N {
                let b = self.center(i);
                let d = b.distance(k.pos);
                if d >= R + k.radius || d < 1e-5 {
                    continue;
                }
                let n = (b - k.pos) / d;
                let (s, c) = self.phi[i].sin_cos();
                let t = Vec3::new(c, s, 0.0);
                let push = n.dot(t);
                let hand = k.vel.dot(t);
                if push.abs() > 0.2 && (hand - self.speed(i)) * push.signum() > 0.0 {
                    self.set_speed(i, hand);
                }
                self.shift(i, (R + k.radius - d) * push);
            }
        }
        None
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct State {
    pos: Vec3F,
    #[serde(default = "identity_rot")]
    rot: QuatF,
    #[serde(skip)]
    cradle: Cradle,
    #[serde(skip)]
    inertia: Inertia,
}
impl Default for State {
    fn default() -> Self {
        State {
            pos: [0.0; 3].into(),
            rot: identity_rot(),
            cradle: Cradle::default(),
            inertia: Inertia::default(),
        }
    }
}
impl Migrate for State {
    type Old = State;
}
impl ClientState for State {
    const APP_ID: &'static str = "io.github.TaylorBeeston.NewtonsCradles";
    fn initial_state_update(&mut self) {
        if let Some(pos) = env_pos("CRADLE_POS") {
            self.pos = pos;
        }
    }
}

fn on_frame(state: &mut State, frame: &TouchFrame) {
    let rot = Quat::from(state.rot);
    state.inertia.sample(Vec3::from(state.pos), rot, frame.dt);
    let target = state.cradle.interact(frame);
    state
        .cradle
        .step(frame.dt, target, state.inertia.linear(rot));
}

/// Chrome frame as rods: (from, to) in toy space. `rod.glb` is a unit rod along +Y.
fn frame_rods() -> Vec<(Vec3, Vec3)> {
    let (w, d) = (WIDTH / 2.0, BAR_Z);
    let mut rods = Vec::new();
    for z in [-d, d] {
        rods.push((Vec3::new(-w, 0.0, z), Vec3::new(-w, PIVOT_Y, z)));
        rods.push((Vec3::new(w, 0.0, z), Vec3::new(w, PIVOT_Y, z)));
        rods.push((Vec3::new(-w, PIVOT_Y, z), Vec3::new(w, PIVOT_Y, z)));
    }
    rods
}

fn rod(from: Vec3, to: Vec3) -> impl Element<State> {
    let len = from.distance(to);
    Model::namespaced("io.github.TaylorBeeston.DeskToys", "rod")
        .pos(from)
        .rot(Quat::from_rotation_arc(Vec3::Y, (to - from) / len))
        .scl([0.003, len, 0.003])
        .build()
}

fn string_lines(c: &Cradle, predict_dt: f32) -> Vec<Line> {
    // fishing-line thin, faintly warm so it reads against dark rooms
    let thread = rgba_linear!(0.42, 0.4, 0.37, 1.0);
    (0..N)
        .flat_map(|i| {
            let phi = c.display_phi(i, predict_dt);
            let (s, co) = phi.sin_cos();
            let top = c.display_center(i, predict_dt) + Vec3::new(-s, co, 0.0) * R;
            [-BAR_Z, BAR_Z].map(|z| {
                line_from_points(vec![[rest_x(i), PIVOT_Y, z], top.into()])
                    .thickness(0.0006)
                    .color(thread)
            })
        })
        .collect()
}

impl Reify for State {
    fn reify(
        &self,
        _context: &Context,
        _tasks: impl Tasker<Self>,
        _props: (),
    ) -> impl Element<Self> {
        let predict = 0.0;
        let boxed = |size: [f32; 3], at: [f32; 3]| Shape::Transform {
            shape: Box::new(Shape::Box { size: size.into() }),
            transform: Mat4::from_translation(at.into()).into(),
        };
        // the move handle: base + the whole chrome frame (bars and legs), never the balls, so
        // pinching the frame moves the cradle and pinching a ball pulls the ball
        let (w, d, t) = (WIDTH / 2.0, BAR_Z, 0.014);
        let mut handle = vec![boxed(
            [WIDTH + 0.03, t, 2.0 * BAR_Z + 0.04],
            [0.0, -0.007, 0.0],
        )];
        for z in [-d, d] {
            handle.push(boxed([WIDTH + t, t, t], [0.0, PIVOT_Y, z]));
            for x in [-w, w] {
                handle.push(boxed([t, PIVOT_Y, t], [x, PIVOT_Y / 2.0, z]));
            }
        }
        let base = Shape::Union { shapes: handle };
        let play = Shape::Transform {
            shape: Box::new(Shape::Box {
                size: [WIDTH + 0.06, PIVOT_Y + 0.03, 2.0 * BAR_Z + 0.05].into(),
            }),
            transform: Mat4::from_translation([0.0, PIVOT_Y / 2.0, 0.0].into()).into(),
        };
        let balls = (0..N).map(|i| {
            Model::namespaced("io.github.TaylorBeeston.DeskToys", "ball")
                .pos(self.cradle.display_center(i, predict))
                .build()
        });
        // balls/frame live on a child entity that senses touch; the handle entity above only
        // moves the toy
        Entity::new(base)
            .pos(self.pos)
            .rot(self.rot)
            .component((
                Grabbable::new(|state: &mut State, pose| {
                    state.pos = pose.position;
                    state.rot = upright(pose.orientation);
                })
                .max_distance(0.015)
                .pointer_mode(PointerMode::Align),
                PalmGrabbable::new(|state: &mut State, pose| {
                    state.pos = pose.position;
                    state.rot = upright(pose.orientation);
                })
                .hit_radius(0.18),
            ))
            .build()
            .child(
                Entity::new(play)
                    .component(Touch::new(on_frame))
                    .build()
                    .child(
                        Model::namespaced("io.github.TaylorBeeston.DeskToys", "cradle_base")
                            .build(),
                    )
                    .children(frame_rods().into_iter().map(|(a, b)| rod(a, b)))
                    .children([-1.0, 1.0].into_iter().flat_map(|sx| {
                        [-BAR_Z, BAR_Z].map(|z| {
                            Model::namespaced("io.github.TaylorBeeston.DeskToys", "ball")
                                .pos([sx * WIDTH / 2.0, PIVOT_Y, z])
                                .scl([0.4; 3])
                                .build()
                        })
                    }))
                    .child(Lines::new(string_lines(&self.cradle, predict)).build())
                    .children(balls),
            )
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    desk_toys::migrate_app_config(
        "org.stardustxr.NewtonsCradle",
        "io.github.TaylorBeeston.NewtonsCradles",
    );
    init_tracing("newtons_cradle=info");
    stardust_xr_asteroids::client::run::<State>(&[&stardust_xr_fusion::project_local_resources!(
        "data"
    )])
    .await
    .unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(c: &mut Cradle, secs: f32) {
        for _ in 0..(secs * 90.0) as usize {
            c.step(1.0 / 90.0, None, Vec3::ZERO);
        }
    }

    #[test]
    fn one_ball_in_one_ball_out() {
        let mut c = Cradle::default();
        c.phi[0] = -0.6;
        run(&mut c, 0.32); // quarter period in, quarter period out
        assert!(
            c.phi[N - 1] > 0.45,
            "last ball should swing out: {:?}",
            c.phi
        );
        for i in 0..N - 1 {
            assert!(
                c.phi[i].abs() < 0.08,
                "ball {i} should be ~still: {:?}",
                c.phi
            );
        }
    }

    #[test]
    fn two_balls_in_two_balls_out() {
        let mut c = Cradle::default();
        c.phi[0] = -0.6;
        c.phi[1] = -0.6;
        run(&mut c, 0.32);
        assert!(c.phi[N - 1] > 0.4 && c.phi[N - 2] > 0.4, "{:?}", c.phi);
        assert!(
            c.phi[0].abs() < 0.08 && c.phi[1].abs() < 0.08,
            "{:?}",
            c.phi
        );
    }

    #[test]
    fn shoving_the_frame_sideways_sets_the_balls_swinging() {
        let mut c = Cradle::default();
        // a quick shove: 25 m/s² toward +X for 60 ms, then held still
        for _ in 0..5 {
            c.step(1.0 / 90.0, None, Vec3::new(25.0, 0.0, 0.0));
        }
        let mut peak: f32 = 0.0;
        for _ in 0..90 {
            c.step(1.0 / 90.0, None, Vec3::ZERO);
            peak = peak.max(c.phi.iter().fold(0.0f32, |m, p| m.max(p.abs())));
        }
        assert!(peak > 0.1, "balls barely moved: {peak}");
    }

    #[test]
    fn a_settled_cradle_goes_to_sleep_and_stays_exactly_still() {
        let mut c = Cradle::default();
        c.phi[0] = -0.6;
        run(&mut c, 120.0);
        assert_eq!(c.phi, [0.0; N], "should have snapped to rest");
        let before = c.clone();
        run(&mut c, 1.0);
        assert_eq!(c, before);
    }

    #[test]
    fn balls_never_overlap() {
        let mut c = Cradle::default();
        c.phi[0] = -1.0;
        c.phi[1] = -0.5;
        for _ in 0..(90 * 5) {
            c.step(1.0 / 90.0, None, Vec3::ZERO);
            for i in 0..N - 1 {
                let gap = c.center(i + 1).x - c.center(i).x - 2.0 * R;
                assert!(gap > -0.002, "overlap {gap} at {i}: {:?}", c.phi);
            }
        }
    }

    /// Same elapsed time at different tick rates → same ball angles (substepped integrator).
    #[test]
    fn cradle_step_is_frame_rate_independent() {
        let run = |hz: f32| {
            let mut c = Cradle::default();
            c.phi[0] = -0.8;
            let total = 0.4_f32;
            let n = (total * hz).round() as u32;
            let dt = total / n as f32;
            for _ in 0..n {
                c.step(dt, None, Vec3::ZERO);
            }
            c.phi
        };
        let a = run(30.0);
        let b = run(90.0);
        let c = run(144.0);
        let mut worst = 0.0f32;
        for i in 0..N {
            worst = worst.max((a[i] - b[i]).abs()).max((b[i] - c[i]).abs());
        }
        println!("cradle φ 30={a:?} 90={b:?} 144={c:?}; worst Δ {worst}");
        assert!(worst < 0.03, "frame-rate drift {worst}");
    }

    #[test]
    fn display_prediction_does_not_cross_contact() {
        let mut c = Cradle::default();
        c.omega[0] = 5.0;
        for dt in [0.0, 0.005, 0.01, 0.03] {
            let gap = c.display_center(1, dt).x - c.display_center(0, dt).x - 2.0 * R;
            println!("display horizon {dt}s: gap {gap}m");
            assert!(gap >= -1e-6, "prediction penetrated by {}m", -gap);
        }
    }

    fn momentum_energy(c: &Cradle) -> (f32, f32) {
        let momentum = (0..N).map(|i| c.speed(i)).sum();
        let energy = (0..N)
            .map(|i| 0.5 * (L * c.omega[i]).powi(2) + G * L * (1.0 - c.phi[i].cos()))
            .sum();
        (momentum, energy)
    }

    #[test]
    fn collision_preserves_momentum_and_dissipates_only_restitution_loss() {
        let mut c = Cradle::default();
        c.omega[0] = 5.0;
        let (p0, e0) = momentum_energy(&c);
        c.collide(None);
        let (p1, e1) = momentum_energy(&c);
        println!("collision momentum {p0} -> {p1}; energy {e0} -> {e1}");
        assert!((p1 - p0).abs() < 1e-6);
        assert!(e1 <= e0 + 1e-6 && e1 > 0.9 * e0);
    }

    #[test]
    fn collision_trajectory_and_energy_match_at_30_90_144_fps() {
        let run = |hz: u32| {
            let mut c = Cradle::default();
            c.phi[0] = -0.8;
            let initial = momentum_energy(&c).1;
            for _ in 0..hz * 2 {
                c.step(1.0 / hz as f32, None, Vec3::ZERO);
                for i in 0..N - 1 {
                    let gap =
                        c.display_center(i + 1, 0.03).x - c.display_center(i, 0.03).x - 2.0 * R;
                    assert!(gap > -0.00002, "{hz}Hz overlap {}", -gap);
                }
            }
            let metrics = momentum_energy(&c);
            println!(
                "{hz}Hz: momentum={} energy={} (initial={initial}) phi={:?}",
                metrics.0, metrics.1, c.phi
            );
            assert!(metrics.1 < initial);
            (c, metrics)
        };
        let (reference, metrics) = run(30);
        for hz in [90, 144] {
            let (c, m) = run(hz);
            assert!((metrics.0 - m.0).abs() < 1e-5);
            assert!((metrics.1 - m.1).abs() < 1e-5);
            for i in 0..N {
                assert!((c.phi[i] - reference.phi[i]).abs() < 1e-5);
                assert!((c.omega[i] - reference.omega[i]).abs() < 1e-5);
            }
        }
    }
}
