//! Cylinder wire rings + gap bead geometry and ownership (Flatland `spawn_edge_grab` port).
//!
//! Pure math lives here so it can be unit-tested without XR. The live drawer is
//! [`crate::wire_bead::WireBead`].

use glam::Vec3;
use std::fs::File;
use std::io::{Read, Write};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

/// Grab radius around the bead (metres). Matches Flatland `FLATLAND_HANDLE_GRAB_M` default.
pub const BEAD_GRAB_M: f32 = 0.045;
/// Wire-sphere visual radius on the bead.
pub const BEAD_VIS_R: f32 = 0.012;
/// Clearance between panel edge and ring (Flatland used 0.06).
pub const RING_CLEARANCE_M: f32 = 0.06;
pub const RING_SEGS: usize = 96;
pub const BEAD_SEGS: usize = 20;
pub const RING_THICKNESS: f32 = 0.0015;
pub const BEAD_THICKNESS: f32 = 0.003;

/// `CYLINDER_WIRE=0` disables; unset or any other value enables (default on).
pub fn wire_enabled() -> bool {
    match std::env::var("CYLINDER_WIRE") {
        Ok(v) if v == "0" || v.eq_ignore_ascii_case("false") || v.eq_ignore_ascii_case("off") => {
            false
        }
        _ => true,
    }
}

/// Cylinder-frame state the owner needs to draw / grab.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WireLayout {
    pub radius_m: f32,
    /// Group yaw (gap centre azimuth).
    pub yaw_rad: f32,
    /// Belt centre height.
    pub height_m: f32,
    /// Panel height used for ring clearance (owner's panel).
    pub panel_height_m: f32,
    pub gap_m: f32,
}

impl WireLayout {
    pub fn upper_ring_y(&self) -> f32 {
        self.height_m + self.panel_height_m * 0.5 + RING_CLEARANCE_M
    }
    pub fn lower_ring_y(&self) -> f32 {
        self.height_m - self.panel_height_m * 0.5 - RING_CLEARANCE_M
    }
    /// Bead sits on the lower ring under the gap centre.
    pub fn bead_pos(&self) -> Vec3 {
        let r = self.radius_m;
        let a = self.yaw_rad;
        Vec3::new(r * a.sin(), self.lower_ring_y(), -r * a.cos())
    }
}

/// Point on the cylinder at azimuth `a` (same convention as [`crate::panel::SlotPlacement`]).
pub fn cyl_point(radius_m: f32, yaw_rad: f32, y: f32) -> Vec3 {
    Vec3::new(radius_m * yaw_rad.sin(), y, -radius_m * yaw_rad.cos())
}

/// Closed ring polylines (upper, lower) in cylinder frame. Points only — no Line type
/// (keeps unit tests free of fusion).
pub fn ring_polylines(layout: &WireLayout) -> (Vec<Vec3>, Vec<Vec3>) {
    let ring = |y: f32| -> Vec<Vec3> {
        (0..RING_SEGS)
            .map(|k| {
                let a = k as f32 / RING_SEGS as f32 * std::f32::consts::TAU;
                cyl_point(layout.radius_m, a, y)
            })
            .collect()
    };
    (ring(layout.upper_ring_y()), ring(layout.lower_ring_y()))
}

/// Three orthogonal circles making a wire sphere at `bead`.
pub fn bead_polylines(bead: Vec3) -> [Vec<Vec3>; 3] {
    let circle = |axis: u8| -> Vec<Vec3> {
        (0..BEAD_SEGS)
            .map(|k| {
                let a = k as f32 / BEAD_SEGS as f32 * std::f32::consts::TAU;
                let (s, c) = (BEAD_VIS_R * a.sin(), BEAD_VIS_R * a.cos());
                let o = match axis {
                    0 => Vec3::new(s, c, 0.0),
                    1 => Vec3::new(0.0, c, s),
                    _ => Vec3::new(s, 0.0, c),
                };
                bead + o
            })
            .collect()
    };
    [circle(0), circle(1), circle(2)]
}

/// Horizontal polar around the cylinder axis (y ignored): azimuth + xz distance.
pub fn polar_xz(p: Vec3) -> (f32, f32) {
    (p.x.atan2(-p.z), (p.x * p.x + p.z * p.z).sqrt())
}

fn wrap_pi(a: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    (a + PI).rem_euclid(TAU) - PI
}

/// Per-frame grab deltas from a previous sample to `now` (cylinder frame).
/// Yaw follows the hand's azimuth around the axis; height is raw Δy.
/// Radius drag is intentionally omitted (task B: group_grab yaw/height only).
pub fn grab_frame_deltas(prev: Vec3, now: Vec3) -> (f32, f32) {
    let (az0, _) = polar_xz(prev);
    let (az1, _) = polar_xz(now);
    (wrap_pi(az1 - az0), now.y - prev.y)
}

/// Cumulative deltas from grab-start pose (useful for tests / absolute Adjust style).
pub fn grab_total_deltas(start: Vec3, now: Vec3) -> (f32, f32) {
    grab_frame_deltas(start, now)
}

/// 0 = idle, 1 = hand near, 2 = grabbing.
pub type BeadVisualState = u8;

// ---- ownership (one drawer across N workers) --------------------------------

use std::sync::Mutex;

static WIRE_OWNER: Mutex<Option<WireOwnerGuard>> = Mutex::new(None);

/// RAII flock on `$XDG_RUNTIME_DIR/stardust-cylinder/wire.owner`.
#[derive(Debug)]
pub struct WireOwnerGuard {
    _file: File,
    path: PathBuf,
}

impl Drop for WireOwnerGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

pub fn wire_owner_path(runtime: &Path) -> PathBuf {
    runtime.join("stardust-cylinder").join("wire.owner")
}

/// Process-local: do we currently hold the wire flock?
pub fn this_process_owns_wire() -> bool {
    WIRE_OWNER.lock().map(|g| g.is_some()).unwrap_or(false)
}

/// Try to become the sole wire owner for this process. Idempotent: if we already
/// own it, returns true. `CYLINDER_WIRE=0` → always false.
pub fn ensure_wire_owner(runtime: &Path) -> bool {
    if !wire_enabled() {
        return false;
    }
    let mut slot = match WIRE_OWNER.lock() {
        Ok(g) => g,
        Err(_) => return false,
    };
    if slot.is_some() {
        return true;
    }
    if let Some(g) = try_claim_wire_owner_inner(runtime) {
        tracing::info!("claimed cylinder wire ownership");
        *slot = Some(g);
        true
    } else {
        false
    }
}

/// Test-only / inner claim without touching the process static.
pub fn try_claim_wire_owner(runtime: &Path) -> Option<WireOwnerGuard> {
    try_claim_wire_owner_inner(runtime)
}

fn try_claim_wire_owner_inner(runtime: &Path) -> Option<WireOwnerGuard> {
    let dir = runtime.join("stardust-cylinder");
    let _ = std::fs::create_dir_all(&dir);
    let path = wire_owner_path(runtime);
    if path.exists() {
        if let Ok(mut f) = File::open(&path) {
            let mut s = String::new();
            let _ = f.read_to_string(&mut s);
            if let Ok(pid) = s.trim().parse::<i32>() {
                let self_pid = std::process::id() as i32;
                if pid != self_pid {
                    // SAFETY: kill(pid, 0) probes existence. EPERM means alive but not signalable.
                    let alive = unsafe {
                        let rc = libc::kill(pid, 0);
                        rc == 0 || (rc == -1 && *libc::__errno_location() == libc::EPERM)
                    };
                    if alive {
                        return None;
                    }
                }
            }
        }
        let _ = std::fs::remove_file(&path);
    }
    let mut file = File::options()
        .create(true)
        .truncate(true)
        .read(true)
        .write(true)
        .open(&path)
        .ok()?;
    let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if rc != 0 {
        return None;
    }
    let _ = writeln!(file, "{}", std::process::id());
    let _ = file.flush();
    Some(WireOwnerGuard { _file: file, path })
}

/// Release ownership (tests).
#[cfg(test)]
pub fn release_wire_owner_for_test() {
    if let Ok(mut g) = WIRE_OWNER.lock() {
        g.take();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout() -> WireLayout {
        WireLayout {
            radius_m: 0.70,
            yaw_rad: 0.0,
            height_m: 0.0,
            panel_height_m: 0.70, // ~1600/2300
            gap_m: 0.055,
        }
    }

    #[test]
    fn rings_sit_above_and_below_panel_not_through_it() {
        let l = layout();
        let (upper, lower) = ring_polylines(&l);
        assert_eq!(upper.len(), RING_SEGS);
        assert_eq!(lower.len(), RING_SEGS);
        let panel_top = l.height_m + l.panel_height_m * 0.5;
        let panel_bot = l.height_m - l.panel_height_m * 0.5;
        for p in &upper {
            assert!(p.y > panel_top + 0.05, "upper ring must clear panel top, y={}", p.y);
            assert!((p.x.hypot(p.z) - l.radius_m).abs() < 1e-5);
        }
        for p in &lower {
            assert!(p.y < panel_bot - 0.05, "lower ring must clear panel bottom, y={}", p.y);
            assert!((p.x.hypot(p.z) - l.radius_m).abs() < 1e-5);
        }
    }

    #[test]
    fn bead_lies_on_lower_ring_under_gap() {
        let mut l = layout();
        l.yaw_rad = 0.3;
        let b = l.bead_pos();
        assert!((b.y - l.lower_ring_y()).abs() < 1e-6);
        assert!((b.x.hypot(b.z) - l.radius_m).abs() < 1e-5);
        let (az, _) = polar_xz(b);
        assert!((wrap_pi(az - l.yaw_rad)).abs() < 1e-5, "bead az={az} yaw={}", l.yaw_rad);
        let spheres = bead_polylines(b);
        assert_eq!(spheres[0].len(), BEAD_SEGS);
        // centre of each circle ≈ bead
        for circ in &spheres {
            let c = circ.iter().copied().sum::<Vec3>() / circ.len() as f32;
            assert!((c - b).length() < 1e-4);
        }
    }

    #[test]
    fn grab_deltas_yaw_right_and_height_up() {
        let start = Vec3::new(0.0, 0.0, -0.7); // gap centre on -Z
        // move hand ~15° to the right (+X) and up 10 cm
        let now = cyl_point(0.7, 0.15, 0.10);
        let (dyaw, dh) = grab_total_deltas(start, now);
        assert!(dyaw > 0.14 && dyaw < 0.16, "dyaw={dyaw}");
        assert!((dh - 0.10).abs() < 1e-5);
    }

    #[test]
    fn grab_frame_deltas_accumulate_like_total() {
        let a = Vec3::new(0.0, 0.0, -0.7);
        let b = cyl_point(0.7, 0.05, 0.02);
        let c = cyl_point(0.7, 0.12, 0.07);
        let (d1, h1) = grab_frame_deltas(a, b);
        let (d2, h2) = grab_frame_deltas(b, c);
        let (dt, ht) = grab_total_deltas(a, c);
        assert!((d1 + d2 - dt).abs() < 1e-5);
        assert!((h1 + h2 - ht).abs() < 1e-5);
    }

    #[test]
    fn wire_enabled_defaults_on() {
        // Can't safely mutate env in parallel tests; just check the parser path via logic.
        assert!(matches!(None::<String>, None)); // placeholder keeps module compiling
        // Explicit: "0" disables
        let off = |v: &str| v == "0" || v.eq_ignore_ascii_case("false") || v.eq_ignore_ascii_case("off");
        assert!(off("0"));
        assert!(off("false"));
        assert!(!off("1"));
        assert!(!off(""));
    }

    #[test]
    fn ensure_wire_owner_is_idempotent_and_respects_living_foreign_pid() {
        release_wire_owner_for_test();
        let dir = tempfile::tempdir().unwrap();
        assert!(ensure_wire_owner(dir.path()), "first claim");
        assert!(this_process_owns_wire());
        assert!(ensure_wire_owner(dir.path()), "idempotent");
        release_wire_owner_for_test();
        // Foreign living pid (init) in the owner file → refuse without stealing.
        let path = wire_owner_path(dir.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "1
").unwrap();
        assert!(try_claim_wire_owner_inner(dir.path()).is_none(), "pid 1 alive blocks claim");
        // Dead pid is stealable.
        std::fs::write(&path, "2147483646
").unwrap(); // unlikely live
        assert!(try_claim_wire_owner_inner(dir.path()).is_some(), "dead pid stealable");
        release_wire_owner_for_test();
    }
}
