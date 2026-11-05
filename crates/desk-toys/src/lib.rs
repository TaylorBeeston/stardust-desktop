//! Physics desk toys for Stardust: shared bits.

pub mod inertia;
pub mod touch;

use glam::{Quat, Vec3};
use stardust_xr_fusion::types::{QuatF, Vec3F};

/// `<PREFIX>_POS="x,y,z"` override for the spawn position (relative to the client root).
pub fn env_pos(var: &str) -> Option<Vec3F> {
    let s = std::env::var(var).ok()?;
    let v: Vec<f32> = s.split(',').map(|p| p.trim().parse().ok()).collect::<Option<_>>()?;
    (v.len() == 3).then(|| [v[0], v[1], v[2]].into())
}

/// Keep only the yaw of a grab pose so toys stay standing upright on the "desk".
pub fn upright(rot: QuatF) -> QuatF {
    let fwd = Quat::from(rot) * Vec3::Z;
    let yaw = fwd.x.atan2(fwd.z);
    Quat::from_rotation_y(yaw).into()
}

pub fn identity_rot() -> QuatF {
    Quat::IDENTITY.into()
}

pub fn init_tracing(default: &str) {
    tracing_subscriber::fmt()
        .compact()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| default.into()),
        )
        .init();
}
