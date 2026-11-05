//! Panel worker binary — intended `stardust-xr-wayland-service -p` target (Candidate B).
//!
//! Accepts one PanelItem via `PanelItemAcceptor` / `SDXR_PROTECTED_PANEL`, registers with
//! `cylinder-layout`, draws a curved mesh, owns curved hit-test. Asteroids client fill-in
//! lands against `docs/cylinder-shell-design.md`.

use cylinder_shell::PanelMetrics;
use tracing_subscriber::EnvFilter;

fn main() {
    tracing_subscriber::fmt()
        .compact()
        .with_env_filter(EnvFilter::from_default_env().add_directive("cylinder_shell=info".parse().unwrap()))
        .init();

    tracing::info!(
        app_id = %std::env::var("SDXR_WL_APP_ID").unwrap_or_default(),
        title = %std::env::var("SDXR_WL_TITLE").unwrap_or_default(),
        "cylinder-shell worker scaffold; PanelItemAcceptor + layout IPC not wired yet"
    );

    // Keep a typed mention of metrics so the binary links the lib API.
    let _metrics = PanelMetrics {
        size_px: (1280, 720),
        density_px_per_m: 3000.0,
        app_id: std::env::var("SDXR_WL_APP_ID").ok(),
        title: std::env::var("SDXR_WL_TITLE").ok(),
    };

    // Full Asteroids `run::<CylinderShell>(...)` + PanelItemAcceptor goes here.
    // Exiting 0 keeps `cargo check` / headless smoke calm; wayland-service will time out
    // until fill-in connects the ProtectedPanel oneshot (expected until then).
    tracing::warn!("scaffold only — not accepting panels yet");
}
