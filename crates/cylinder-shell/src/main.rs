//! Panel worker binary — intended `stardust-xr-wayland-service -p` target (Candidate B).
//!
//! Implemented: registers with `cylinder-layout`, receives a slot, computes placement
//! (pose + curved hit-test) and bakes the curved GLB. Not yet implemented: the Asteroids
//! client that consumes `SDXR_PROTECTED_PANEL` via `PanelItemAcceptor` (needs a live
//! Stardust server — headset session). Without `SDXR_PROTECTED_PANEL` this runs the
//! `NoServer` path: layout dry run, then exit 0.

use cylinder_shell::{
    bake_curved_panel_glb, LayoutClient, LayoutEvent, LayoutRequest, PanelMetrics, PanelWorker, SocketLayoutClient,
};
use std::time::Duration;
use tracing_subscriber::EnvFilter;

fn env_u32(k: &str) -> Option<u32> {
    std::env::var(k).ok()?.parse().ok()
}

fn main() {
    tracing_subscriber::fmt()
        .compact()
        .with_env_filter(EnvFilter::from_default_env().add_directive("cylinder_shell=info".parse().unwrap()))
        .init();

    let metrics = PanelMetrics {
        size_px: (env_u32("CYLINDER_WIDTH_PX").unwrap_or(1280), env_u32("CYLINDER_HEIGHT_PX").unwrap_or(720)),
        density_px_per_m: std::env::var("CYLINDER_DENSITY").ok().and_then(|v| v.parse().ok()).unwrap_or(3000.0),
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
    if let Some(c) = client.as_mut() {
        if let Err(e) = c.send(LayoutRequest::Hello { metrics }) {
            tracing::warn!(%e, "layout hello failed");
        } else if let Ok(Some(LayoutEvent::Assigned { slot, group })) = c.recv_timeout(Duration::from_secs(2)) {
            let p = worker.apply_slot(slot.clone(), &group);
            let mesh = bake_curved_panel_glb(slot.width_m, slot.height_m, group.radius_m);
            tracing::info!(id = slot.id.0, az = p.azimuth_rad, pos = ?p.position, ?mesh, "slot assigned");
        }
    }

    if std::env::var_os("SDXR_PROTECTED_PANEL").is_none() {
        tracing::info!("NoServer: no SDXR_PROTECTED_PANEL; layout dry run done");
        if let (Some(c), Some(slot)) = (client.as_mut(), worker.slot.as_ref()) {
            let _ = c.send(LayoutRequest::Bye { id: slot.id });
        }
        return;
    }
    // TODO(headset): Asteroids `run::<CylinderShell>` + PanelItemAcceptor consuming the
    // ProtectedPanel oneshot, SurfaceModel with the baked GLB, curved Field hit-test.
    tracing::error!("SDXR_PROTECTED_PANEL set but XR accept path not implemented yet; exiting");
    std::process::exit(2);
}
