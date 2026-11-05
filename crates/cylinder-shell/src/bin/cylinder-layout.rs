//! Layout daemon — owns `CylinderState`, assigns `PanelSlot`s to workers.
//! Socket: `$CYLINDER_LAYOUT_SOCK` or `$XDG_RUNTIME_DIR/stardust-cylinder/layout.sock`.

use cylinder_shell::{CylinderState, LayoutDaemon, default_socket_path};
use tracing_subscriber::EnvFilter;

fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt()
        .compact()
        .with_env_filter(EnvFilter::from_default_env().add_directive("cylinder_shell=info".parse().unwrap()))
        .init();

    let state = CylinderState::default();
    let sock = default_socket_path();
    let daemon = LayoutDaemon::bind(&sock, state.clone())?;
    tracing::info!(?sock, radius_m = state.radius_m, gap_m = state.gap_m, "cylinder-layout listening");
    daemon.serve()
}
