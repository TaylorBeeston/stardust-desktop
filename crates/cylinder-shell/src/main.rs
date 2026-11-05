//! Panel worker binary — intended `stardust-xr-wayland-service -p` target (Candidate B).
//!
//! Without `SDXR_PROTECTED_PANEL`: layout dry-run, exit 0 (`NoServer` path).
//! With `SDXR_PROTECTED_PANEL`: preflight provider + server sockets (fail fast, no hang,
//! no live-session connect on bogus env), then Asteroids `run::<CylinderShell>`.

use cylinder_shell::{
    bake_curved_panel_glb, install_pending, preflight_xr, LayoutClient, LayoutEvent, LayoutRequest,
    PanelMetrics, PanelWorker, PendingLaunch, SocketLayoutClient,
};
use std::path::PathBuf;
use std::time::Duration;
use tracing_subscriber::EnvFilter;

fn env_u32(k: &str) -> Option<u32> {
    std::env::var(k).ok()?.parse().ok()
}

fn runtime_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

fn main() {
    tracing_subscriber::fmt()
        .compact()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive("cylinder_shell=info".parse().unwrap()),
        )
        .init();

    let metrics = PanelMetrics {
        size_px: (
            env_u32("CYLINDER_WIDTH_PX").unwrap_or(1280),
            env_u32("CYLINDER_HEIGHT_PX").unwrap_or(720),
        ),
        density_px_per_m: std::env::var("CYLINDER_DENSITY")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(3000.0),
        app_id: std::env::var("SDXR_WL_APP_ID").ok(),
        title: std::env::var("SDXR_WL_TITLE").ok(),
    };
    let mut worker = PanelWorker::new(metrics.clone());

    let mut client = match SocketLayoutClient::connect() {
        Ok(c) => Some(c),
        Err(e) => {
            tracing::warn!(%e, "cylinder-layout not reachable; panel would fall back flat");
            None
        }
    };

    let mut mesh_path = None;
    let mut radius_m = 0.7f32;
    if let Some(c) = client.as_mut() {
        if let Err(e) = c.send(LayoutRequest::Hello { metrics: metrics.clone() }) {
            tracing::warn!(%e, "layout hello failed");
        } else if let Ok(Some(LayoutEvent::Assigned { slot, group })) =
            c.recv_timeout(Duration::from_secs(2))
        {
            let p = worker.apply_slot(slot.clone(), &group);
            radius_m = group.radius_m;
            match bake_curved_panel_glb(slot.width_m, slot.height_m, group.radius_m) {
                Ok(path) => {
                    tracing::info!(id = slot.id.0, az = p.azimuth_rad, pos = ?p.position, ?path, "slot assigned");
                    mesh_path = Some(path);
                }
                Err(e) => tracing::error!(%e, "mesh bake failed"),
            }
        }
    }

    let sdxr = std::env::var("SDXR_PROTECTED_PANEL").ok();
    if sdxr.is_none() {
        tracing::info!("NoServer: no SDXR_PROTECTED_PANEL; layout dry run done");
        if let (Some(c), Some(slot)) = (client.as_mut(), worker.slot.as_ref()) {
            let _ = c.send(LayoutRequest::Bye { id: slot.id });
        }
        return;
    }

    // Fail fast before asteroids::run — never touch a live session on bogus env.
    if let Err(e) = preflight_xr(sdxr.as_deref(), &runtime_dir()) {
        tracing::error!(%e, "XR preflight failed; not connecting to Stardust");
        if let (Some(c), Some(slot)) = (client.as_mut(), worker.slot.as_ref()) {
            let _ = c.send(LayoutRequest::Bye { id: slot.id });
        }
        std::process::exit(2);
    }

    let placement = match worker.placement {
        Some(p) => p,
        None => {
            // No layout daemon: still accept, place at default cylinder front.
            let mut g = cylinder_shell::CylinderState::default();
            let slot = g.assign_slot(&metrics);
            worker.apply_slot(slot.clone(), &g)
        }
    };
    let mesh_path = match mesh_path {
        Some(p) => p,
        None => match bake_curved_panel_glb(
            placement.hit_test.width_m,
            placement.hit_test.height_m,
            radius_m,
        ) {
            Ok(p) => p,
            Err(e) => {
                tracing::error!(%e, "mesh bake failed; cannot run XR");
                std::process::exit(2);
            }
        },
    };

    install_pending(PendingLaunch {
        metrics,
        placement,
        mesh_path,
        radius_m,
    });

    // Drop layout client before entering the XR loop (socket stays open otherwise).
    drop(client);

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    if let Err(e) = rt.block_on(stardust_xr_asteroids::client::run::<cylinder_shell::CylinderShell>(
        &[],
    )) {
        tracing::error!(%e, "asteroids run failed");
        std::process::exit(2);
    }
}
