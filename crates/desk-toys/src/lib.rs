//! Physics desk toys for Stardust: shared bits.

pub mod inertia;
pub mod touch;

use glam::{Quat, Vec3};
use stardust_xr_asteroids::Context;
use stardust_xr_fusion::types::{QuatF, Vec3F};

/// Seconds to extrapolate a coasting pose for display (Nova: predict to predicted display time).
pub fn pose_predict_dt(context: &Context, sim_t: f32) -> f32 {
    inertia::pose_predict_dt(sim_t, context.time(), context.predicted_display_time())
}

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

/// Copy `~/.config/<old_id>` → `~/.config/<new_id>` once (never deletes old).
/// Used when renaming ClientState APP_ID / wrist-menu config dirs.
pub fn migrate_app_config(old_id: &str, new_id: &str) {
    let Some(dirs) = directories::BaseDirs::new() else {
        return;
    };
    let old = dirs.config_dir().join(old_id);
    let new = dirs.config_dir().join(new_id);
    if new.exists() || !old.exists() {
        return;
    }
    if let Err(e) = copy_dir_recursive(&old, &new) {
        tracing::warn!(?e, from = %old.display(), to = %new.display(), "config migrate failed");
    } else {
        tracing::info!(from = %old.display(), to = %new.display(), "migrated app config (old kept)");
    }
}

fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let to = dst.join(entry.file_name());
        let ft = entry.file_type()?;
        if ft.is_dir() {
            copy_dir_recursive(&entry.path(), &to)?;
        } else if ft.is_file() {
            std::fs::copy(entry.path(), to)?;
        }
    }
    Ok(())
}
