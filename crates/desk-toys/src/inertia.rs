//! Felt motion of a toy you're carrying: smoothed linear acceleration and yaw acceleration of
//! the toy's pose, with a dead zone so hand-tracking jitter (a few mm, amplified a lot by
//! differentiating twice) doesn't make the toy tremble while you just hold it.

use glam::{Quat, Vec3};
use std::f32::consts::{PI, TAU};

/// smoothing time constants for velocity and acceleration (s). Yaw is noisier (it comes
/// straight from the hand's orientation), so it gets more.
const TAU_S: f32 = 0.035;
const TAU_YAW: f32 = 0.06;
/// accelerations below this are tracking noise (m/s²); above it they fade in
const DEAD_LINEAR: f32 = 1.5;
const DEAD_YAW: f32 = 6.0;
const MAX_LINEAR: f32 = 60.0;
const MAX_YAW: f32 = 200.0;

#[derive(Debug, Default, Clone)]
pub struct Inertia {
    last: Option<(Vec3, f32)>,
    vel: Vec3,
    acc: Vec3,
    yaw_rate: f32,
    yaw_acc: f32,
}

fn yaw_of(rot: Quat) -> f32 {
    let f = rot * Vec3::Z;
    f.x.atan2(f.z)
}

fn dead_zone(v: f32, dead: f32) -> f32 {
    let m = v.abs();
    if m < dead { 0.0 } else { v.signum() * (m - dead) }
}

impl Inertia {
    /// Feed the toy's pose once per frame (it only changes while it's being carried).
    pub fn sample(&mut self, pos: Vec3, rot: Quat, dt: f32) {
        let yaw = yaw_of(rot);
        let a = 1.0 - (-dt / TAU_S).exp();
        let ay = 1.0 - (-dt / TAU_YAW).exp();
        if let Some((p0, y0)) = self.last {
            let v = (pos - p0) / dt;
            let dyaw = (yaw - y0 + PI).rem_euclid(TAU) - PI;
            let w = dyaw / dt;
            let new_vel = self.vel.lerp(v, a);
            let new_rate = self.yaw_rate + (w - self.yaw_rate) * ay;
            self.acc = self.acc.lerp((new_vel - self.vel) / dt, a);
            self.yaw_acc += ((new_rate - self.yaw_rate) / dt - self.yaw_acc) * ay;
            self.vel = new_vel;
            self.yaw_rate = new_rate;
        }
        self.last = Some((pos, yaw));
    }

    /// Acceleration of the toy in its own (local) frame, jitter removed.
    pub fn linear(&self, rot: Quat) -> Vec3 {
        let m = self.acc.length();
        if m < 1e-6 {
            return Vec3::ZERO;
        }
        let kept = dead_zone(m, DEAD_LINEAR).min(MAX_LINEAR);
        rot.inverse() * (self.acc / m * kept)
    }

    /// Yaw angular acceleration of the toy (rad/s², +Y), jitter removed.
    pub fn yaw(&self) -> f32 {
        dead_zone(self.yaw_acc, DEAD_YAW).clamp(-MAX_YAW, MAX_YAW)
    }

    pub fn moving(&self) -> bool {
        self.linear(Quat::IDENTITY) != Vec3::ZERO || self.yaw() != 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 90.0;

    /// cheap deterministic noise in [-1, 1]
    fn noise(i: u32) -> f32 {
        let x = (i.wrapping_mul(2654435761) >> 8) as f32 / (1u32 << 24) as f32;
        x * 2.0 - 1.0
    }

    #[test]
    fn holding_still_with_tracking_jitter_feels_like_nothing() {
        let mut m = Inertia::default();
        let (mut worst, mut worst_yaw): (f32, f32) = (0.0, 0.0);
        for i in 0..900 {
            // ±1.5 mm position jitter, ±0.5° yaw jitter, every frame
            let p = Vec3::new(noise(i), noise(i + 7), noise(i + 13)) * 0.0015;
            let r = Quat::from_rotation_y(noise(i + 21) * 0.5f32.to_radians());
            m.sample(p, r, DT);
            if i > 30 {
                worst = worst.max(m.linear(Quat::IDENTITY).length());
                worst_yaw = worst_yaw.max(m.yaw().abs());
            }
        }
        println!("jitter leak: linear {worst} m/s², yaw {worst_yaw} rad/s²");
        assert!(worst < 0.5, "jitter leaked through as {worst} m/s²");
        assert!(worst_yaw < 0.5, "yaw jitter leaked through as {worst_yaw} rad/s²");
    }

    #[test]
    fn a_real_shake_comes_through_in_the_toys_frame() {
        let mut m = Inertia::default();
        let yaw = Quat::from_rotation_y(PI / 2.0); // toy turned 90°: world +X is its -Z
        let mut peak = Vec3::ZERO;
        for i in 0..90 {
            // 4 Hz, ±3 cm shake along world X
            let t = i as f32 * DT;
            m.sample(Vec3::new(0.03 * (TAU * 4.0 * t).sin(), 0.0, 0.0), yaw, DT);
            let a = m.linear(yaw);
            if a.length() > peak.length() {
                peak = a;
            }
        }
        assert!(peak.length() > 5.0, "shake too weak: {peak}");
        assert!(peak.z.abs() > 4.0 * peak.x.abs(), "should be along local Z: {peak}");
    }

    #[test]
    fn twisting_reports_yaw_acceleration() {
        let mut m = Inertia::default();
        let mut peak: f32 = 0.0;
        for i in 0..45 {
            let t = i as f32 * DT;
            // spin up to ~6 rad/s over half a second
            m.sample(Vec3::ZERO, Quat::from_rotation_y(6.0 * t * t), DT);
            peak = peak.max(m.yaw());
        }
        println!("twist peak {peak} rad/s²");
        assert!(peak > 5.0, "twist too weak: {peak}");
    }
}
