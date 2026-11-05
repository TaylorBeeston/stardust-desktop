//! Worker ↔ `cylinder-layout` daemon protocol (Candidate B IPC).

use crate::cylinder::{CylinderState, PanelMetrics, PanelSlot, SlotId};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Default socket path under the user runtime dir.
pub fn default_socket_path() -> PathBuf {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").unwrap_or_else(|| "/tmp".into());
    PathBuf::from(runtime).join("stardust-cylinder/layout.sock")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LayoutRequest {
    Hello { metrics: PanelMetrics },
    Bye { id: SlotId },
    MetricsChanged { id: SlotId, metrics: PanelMetrics },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LayoutEvent {
    Assigned { slot: PanelSlot, group: CylinderState },
    GroupChanged { group: CylinderState, slot: PanelSlot },
    Shutdown,
}

pub trait LayoutClient {
    fn connect() -> std::io::Result<Self>
    where
        Self: Sized;
    fn send(&mut self, req: LayoutRequest) -> std::io::Result<()>;
    fn try_recv(&mut self) -> std::io::Result<Option<LayoutEvent>>;
}

/// Unix-socket client placeholder. Framing (length-prefixed bincode/json) in fill-in.
#[derive(Debug)]
pub struct SocketLayoutClient {
    pub path: PathBuf,
}

impl LayoutClient for SocketLayoutClient {
    fn connect() -> std::io::Result<Self> {
        let path = default_socket_path();
        let _ = path;
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "SocketLayoutClient::connect not implemented",
        ))
    }

    fn send(&mut self, req: LayoutRequest) -> std::io::Result<()> {
        let _ = req;
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "SocketLayoutClient::send not implemented",
        ))
    }

    fn try_recv(&mut self) -> std::io::Result<Option<LayoutEvent>> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "SocketLayoutClient::try_recv not implemented",
        ))
    }
}
