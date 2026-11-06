//! Agent/verify frame dump for flatscreen mode.
//!
//! When `CYLVERIFY_SHOT_REQUEST` is set to a filesystem path, each Update checks for
//! `<path>.request`. If present, captures the primary window to `<path>` (PNG) and
//! writes `<path>.done`. Also triggers on F12. Safe no-op when env unset.

use bevy::{
	app::{Plugin, Update},
	ecs::system::{Commands, Res},
	input::{ButtonInput, keyboard::KeyCode},
	prelude::Local,
	render::view::screenshot::{save_to_disk, Screenshot},
};
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub struct VerifyShotPlugin;

impl Plugin for VerifyShotPlugin {
	fn build(&self, app: &mut bevy::app::App) {
		if std::env::var_os("CYLVERIFY_SHOT_REQUEST").is_none() {
			return;
		}
		app.add_systems(Update, verify_shot_system);
	}
}

fn verify_shot_system(
	mut commands: Commands,
	keys: Res<ButtonInput<KeyCode>>,
	mut last: Local<Option<Instant>>,
) {
	let Ok(base) = std::env::var("CYLVERIFY_SHOT_REQUEST") else {
		return;
	};
	let base = PathBuf::from(base);
	let req = PathBuf::from(format!("{}.request", base.display()));
	let done = PathBuf::from(format!("{}.done", base.display()));

	let f12 = keys.just_pressed(KeyCode::F12);
	let file_req = req.exists();
	if !f12 && !file_req {
		return;
	}
	// debounce 250ms
	if let Some(t) = *last {
		if t.elapsed() < Duration::from_millis(250) {
			return;
		}
	}
	*last = Some(Instant::now());

	let path = if file_req {
		let _ = std::fs::remove_file(&req);
		base
	} else {
		// F12 → timestamp next to request base
		let parent = base.parent().unwrap_or_else(|| std::path::Path::new("."));
		parent.join(format!(
			"f12-{}.png",
			std::time::SystemTime::now()
				.duration_since(std::time::UNIX_EPOCH)
				.map(|d| d.as_secs())
				.unwrap_or(0)
		))
	};

	let done_path = done.clone();
	let path_str = path.to_string_lossy().to_string();
	commands
		.spawn(Screenshot::primary_window())
		.observe(save_to_disk(path_str.clone()));
	// Mark done after a short delay on a thread — observer saves async; poll file size
	std::thread::spawn(move || {
		for _ in 0..100 {
			if path.exists() {
				if let Ok(meta) = std::fs::metadata(&path) {
					if meta.len() > 100 {
						let _ = std::fs::write(&done_path, format!("{}\n", path.display()));
						return;
					}
				}
			}
			std::thread::sleep(Duration::from_millis(50));
		}
		let _ = std::fs::write(&done_path, format!("TIMEOUT {}\n", path.display()));
	});
}
