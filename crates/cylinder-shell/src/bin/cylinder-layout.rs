//! Layout daemon — owns `CylinderState`, assigns `PanelSlot`s to workers.

use cylinder_shell::{CylinderState, default_socket_path};
use tracing_subscriber::EnvFilter;

fn main() {
    tracing_subscriber::fmt()
        .compact()
        .with_env_filter(EnvFilter::from_default_env().add_directive("cylinder_shell=info".parse().unwrap()))
        .init();

    let state = CylinderState::default();
    let sock = default_socket_path();
    tracing::info!(?sock, radius_m = state.radius_m, gap_m = state.gap_m, "cylinder-layout scaffold");
    tracing::warn!("scaffold only — not listening yet");
}
