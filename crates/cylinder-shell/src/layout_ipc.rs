//! Worker <-> `cylinder-layout` daemon protocol (Candidate B IPC).
//!
//! Wire format: newline-delimited JSON (`serde_json`), one message per line, over a
//! Unix stream socket. The daemon owns `CylinderState`; every change that moves a slot
//! pushes `GroupChanged` to the affected worker(s).

use crate::cylinder::{CylinderState, PanelMetrics, PanelSlot, SlotId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Default socket path under the user runtime dir (`CYLINDER_LAYOUT_SOCK` overrides).
pub fn default_socket_path() -> PathBuf {
    if let Some(p) = std::env::var_os("CYLINDER_LAYOUT_SOCK") {
        return p.into();
    }
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").unwrap_or_else(|| "/tmp".into());
    PathBuf::from(runtime).join("stardust-cylinder/layout.sock")
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LayoutRequest {
    Hello { metrics: PanelMetrics },
    Bye { id: SlotId },
    MetricsChanged { id: SlotId, metrics: PanelMetrics },
    // Additive (deviation): group controls for bead grab / radius drag / recenter.
    GroupGrab { delta_yaw: f32, delta_height: f32 },
    SetRadius { radius_m: f32 },
    Reanchor { hmd_yaw: f32, hmd_height: f32 },
    /// Swap two panels in registration order.
    Swap { a: SlotId, b: SlotId },
    /// Move panel to index in registration order.
    Reorder { id: SlotId, index: usize },
    /// Move panel by delta in registration order (−1/+1 for neighbour swap-ish).
    Shift { id: SlotId, delta: i32 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LayoutEvent {
    Assigned { slot: PanelSlot, group: CylinderState },
    GroupChanged { group: CylinderState, slot: PanelSlot },
    Shutdown,
}

pub trait LayoutClient {
    fn connect() -> io::Result<Self>
    where
        Self: Sized;
    fn send(&mut self, req: LayoutRequest) -> io::Result<()>;
    fn try_recv(&mut self) -> io::Result<Option<LayoutEvent>>;
}

fn write_msg<T: Serialize>(w: &mut UnixStream, msg: &T) -> io::Result<()> {
    let mut line = serde_json::to_vec(msg).map_err(io::Error::other)?;
    line.push(b'\n');
    w.write_all(&line)
}

/// Unix-socket client.
#[derive(Debug)]
pub struct SocketLayoutClient {
    pub path: PathBuf,
    stream: UnixStream,
    reader: BufReader<UnixStream>,
    partial: String,
}

impl SocketLayoutClient {
    pub fn connect_to(path: &Path) -> io::Result<Self> {
        let stream = UnixStream::connect(path)?;
        let reader = BufReader::new(stream.try_clone()?);
        reader.get_ref().set_nonblocking(true)?;
        Ok(Self { path: path.into(), stream, reader, partial: String::new() })
    }

    /// Poll `try_recv` until an event arrives or `timeout` elapses.
    pub fn recv_timeout(&mut self, timeout: Duration) -> io::Result<Option<LayoutEvent>> {
        let end = Instant::now() + timeout;
        loop {
            if let Some(ev) = self.try_recv()? {
                return Ok(Some(ev));
            }
            if Instant::now() >= end {
                return Ok(None);
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}

impl LayoutClient for SocketLayoutClient {
    fn connect() -> io::Result<Self> {
        Self::connect_to(&default_socket_path())
    }

    fn send(&mut self, req: LayoutRequest) -> io::Result<()> {
        write_msg(&mut self.stream, &req)
    }

    fn try_recv(&mut self) -> io::Result<Option<LayoutEvent>> {
        match self.reader.read_line(&mut self.partial) {
            Ok(0) => Err(io::Error::new(io::ErrorKind::UnexpectedEof, "layout daemon closed")),
            Ok(_) if self.partial.ends_with('\n') => {
                let ev = serde_json::from_str(self.partial.trim_end()).map_err(io::Error::other);
                self.partial.clear();
                ev.map(Some)
            }
            Ok(_) => Ok(None),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(e) => Err(e),
        }
    }
}

/// Daemon side: shared state plus one writer per registered worker.
#[derive(Default)]
struct Shared {
    state: CylinderState,
    writers: HashMap<SlotId, UnixStream>,
}

impl Shared {
    /// Push every live slot to its worker, except `skip` (already answered).
    fn broadcast(&mut self, skip: Option<SlotId>) {
        let group = self.state.clone();
        let mut dead = vec![];
        for (id, w) in self.writers.iter_mut() {
            if Some(*id) == skip {
                continue;
            }
            if let Some(slot) = group.slot(*id).cloned()
                && write_msg(w, &LayoutEvent::GroupChanged { group: group.clone(), slot }).is_err()
            {
                dead.push(*id);
            }
        }
        for id in dead {
            self.writers.remove(&id);
            self.state.release_slot(id);
        }
    }
}

/// Layout daemon. `bind` removes a stale socket file; `serve` blocks accepting workers.
pub struct LayoutDaemon {
    listener: UnixListener,
    shared: Arc<Mutex<Shared>>,
    pub path: PathBuf,
}

impl LayoutDaemon {
    pub fn bind(path: &Path, state: CylinderState) -> io::Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        if path.exists() {
            if UnixStream::connect(path).is_ok() {
                return Err(io::Error::new(io::ErrorKind::AddrInUse, "cylinder-layout already running"));
            }
            std::fs::remove_file(path)?;
        }
        let listener = UnixListener::bind(path)?;
        Ok(Self { listener, shared: Arc::new(Mutex::new(Shared { state, writers: HashMap::new() })), path: path.into() })
    }

    pub fn snapshot(&self) -> CylinderState {
        self.shared.lock().unwrap().state.clone()
    }

    pub fn serve(&self) -> io::Result<()> {
        for conn in self.listener.incoming() {
            let stream = conn?;
            let shared = self.shared.clone();
            std::thread::spawn(move || {
                if let Err(e) = handle_worker(stream, shared) {
                    tracing::debug!(%e, "worker connection ended");
                }
            });
        }
        Ok(())
    }
}

fn handle_worker(stream: UnixStream, shared: Arc<Mutex<Shared>>) -> io::Result<()> {
    let mut writer = stream.try_clone()?;
    let reader = BufReader::new(stream);
    let mut mine: Option<SlotId> = None;
    let result = (|| -> io::Result<()> {
        for line in reader.lines() {
            let line = line?;
            let req: LayoutRequest = match serde_json::from_str(&line) {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!(%e, "bad layout request");
                    continue;
                }
            };
            let mut sh = shared.lock().unwrap();
            match req {
                LayoutRequest::Hello { metrics } => {
                    if let Some(old) = mine.take() {
                        sh.writers.remove(&old);
                        sh.state.release_slot(old);
                    }
                    let slot = sh.state.assign_slot(&metrics);
                    mine = Some(slot.id);
                    write_msg(&mut writer, &LayoutEvent::Assigned { slot: slot.clone(), group: sh.state.clone() })?;
                    sh.writers.insert(slot.id, writer.try_clone()?);
                    tracing::info!(id = slot.id.0, yaw = slot.yaw_rad, app = ?metrics.app_id, "panel registered");
                    sh.broadcast(Some(slot.id));
                }
                LayoutRequest::Bye { id } => {
                    sh.writers.remove(&id);
                    sh.state.release_slot(id);
                    if mine == Some(id) {
                        mine = None;
                    }
                    sh.broadcast(None);
                }
                LayoutRequest::MetricsChanged { id, metrics } => {
                    if sh.state.update_metrics(id, &metrics).is_some() {
                        sh.broadcast(None);
                    }
                }
                LayoutRequest::GroupGrab { delta_yaw, delta_height } => {
                    sh.state.apply_group_grab(delta_yaw, delta_height);
                    sh.broadcast(None);
                }
                LayoutRequest::SetRadius { radius_m } => {
                    sh.state.set_radius(radius_m);
                    sh.broadcast(None);
                }
                LayoutRequest::Reanchor { hmd_yaw, hmd_height } => {
                    sh.state.reanchor(hmd_yaw, hmd_height);
                    sh.broadcast(None);
                }
                LayoutRequest::Swap { a, b } => {
                    if sh.state.swap_slots(a, b) {
                        sh.broadcast(None);
                    }
                }
                LayoutRequest::Reorder { id, index } => {
                    if sh.state.reorder_slot(id, index) {
                        sh.broadcast(None);
                    }
                }
                LayoutRequest::Shift { id, delta } => {
                    if sh.state.shift_slot(id, delta) {
                        sh.broadcast(None);
                    }
                }
            }
        }
        Ok(())
    })();
    if let Some(id) = mine {
        let mut sh = shared.lock().unwrap();
        sh.writers.remove(&id);
        sh.state.release_slot(id);
        sh.broadcast(None);
        tracing::info!(id = id.0, "panel unregistered (disconnect)");
    }
    result
}


/// Worker-side keep-alive for the layout Unix socket across the XR loop.
///
/// Owns `SocketLayoutClient` on a background thread so the connection stays open for
/// the worker lifetime: incoming `GroupChanged`/`Shutdown` are pushed to an async
/// channel, and `send` forwards `GroupGrab` / `SetRadius` / `Reanchor` / `Bye` /
/// `MetricsChanged` without blocking the Asteroids frame.
pub struct LayoutSession {
    req_tx: std::sync::mpsc::Sender<LayoutRequest>,
    ev_rx: tokio::sync::mpsc::UnboundedReceiver<LayoutEvent>,
    /// Slot id from the initial `Assigned`, if known — used to Bye on drop.
    slot_id: Option<SlotId>,
}

impl LayoutSession {
    /// Take ownership of an already-connected client (after `Hello`/`Assigned`).
    pub fn take_over(client: SocketLayoutClient, slot_id: Option<SlotId>) -> Self {
        let (req_tx, req_rx) = std::sync::mpsc::channel::<LayoutRequest>();
        let (ev_tx, ev_rx) = tokio::sync::mpsc::unbounded_channel();
        std::thread::Builder::new()
            .name("cylinder-layout-ipc".into())
            .spawn(move || layout_session_loop(client, req_rx, ev_tx))
            .expect("spawn layout ipc thread");
        Self { req_tx, ev_rx, slot_id }
    }

    pub fn slot_id(&self) -> Option<SlotId> {
        self.slot_id
    }

    pub fn set_slot_id(&mut self, id: SlotId) {
        self.slot_id = Some(id);
    }

    /// Non-blocking send onto the background thread (socket write happens there).
    pub fn send(&self, req: LayoutRequest) -> Result<(), LayoutSessionClosed> {
        self.req_tx.send(req).map_err(|_| LayoutSessionClosed)
    }

    /// Wait for the next event from the daemon (or `None` if the session ended).
    pub async fn recv(&mut self) -> Option<LayoutEvent> {
        self.ev_rx.recv().await
    }

    /// Cloneable request sender for XR callbacks (bead grab, recenter, …).
    pub fn request_sender(&self) -> LayoutRequestSender {
        LayoutRequestSender(self.req_tx.clone())
    }
}

impl Drop for LayoutSession {
    fn drop(&mut self) {
        if let Some(id) = self.slot_id.take() {
            let _ = self.req_tx.send(LayoutRequest::Bye { id });
        }
    }
}

/// Cheap cloneable handle for sending layout requests from XR input callbacks.
#[derive(Debug, Clone)]
pub struct LayoutRequestSender(std::sync::mpsc::Sender<LayoutRequest>);

impl LayoutRequestSender {
    pub fn send(&self, req: LayoutRequest) -> Result<(), LayoutSessionClosed> {
        self.0.send(req).map_err(|_| LayoutSessionClosed)
    }

    pub fn group_grab(&self, delta_yaw: f32, delta_height: f32) -> Result<(), LayoutSessionClosed> {
        self.send(LayoutRequest::GroupGrab { delta_yaw, delta_height })
    }

    pub fn set_radius(&self, radius_m: f32) -> Result<(), LayoutSessionClosed> {
        self.send(LayoutRequest::SetRadius { radius_m })
    }

    pub fn reanchor(&self, hmd_yaw: f32, hmd_height: f32) -> Result<(), LayoutSessionClosed> {
        self.send(LayoutRequest::Reanchor { hmd_yaw, hmd_height })
    }
    pub fn swap(&self, a: SlotId, b: SlotId) -> Result<(), LayoutSessionClosed> {
        self.send(LayoutRequest::Swap { a, b })
    }
    pub fn reorder(&self, id: SlotId, index: usize) -> Result<(), LayoutSessionClosed> {
        self.send(LayoutRequest::Reorder { id, index })
    }
    pub fn shift(&self, id: SlotId, delta: i32) -> Result<(), LayoutSessionClosed> {
        self.send(LayoutRequest::Shift { id, delta })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutSessionClosed;

impl std::fmt::Display for LayoutSessionClosed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("layout session closed")
    }
}

impl std::error::Error for LayoutSessionClosed {}

fn layout_session_loop(
    mut client: SocketLayoutClient,
    req_rx: std::sync::mpsc::Receiver<LayoutRequest>,
    ev_tx: tokio::sync::mpsc::UnboundedSender<LayoutEvent>,
) {
    loop {
        match client.try_recv() {
            Ok(Some(ev)) => {
                let shutdown = matches!(ev, LayoutEvent::Shutdown);
                if ev_tx.send(ev).is_err() {
                    break;
                }
                if shutdown {
                    break;
                }
            }
            Ok(None) => {}
            Err(e) => {
                tracing::debug!(%e, "layout session recv ended");
                break;
            }
        }
        match req_rx.recv_timeout(Duration::from_millis(2)) {
            Ok(req) => {
                if let Err(e) = client.send(req) {
                    tracing::debug!(%e, "layout session send failed");
                    break;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}

/// Apply a layout event to worker bookkeeping. Returns the new placement when the
/// panel should re-pose (Assigned / GroupChanged). Pure — unit-tested without XR.
pub fn apply_layout_event(
    worker: &mut crate::panel::PanelWorker,
    ev: &LayoutEvent,
) -> Option<crate::panel::SlotPlacement> {
    match ev {
        LayoutEvent::Assigned { slot, group } | LayoutEvent::GroupChanged { slot, group } => {
            Some(worker.apply_slot(slot.clone(), group))
        }
        LayoutEvent::Shutdown => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cylinder::half_arc_rad;

    const T: Duration = Duration::from_secs(2);

    fn metrics(w: u32, app: &str) -> PanelMetrics {
        PanelMetrics { size_px: (w, 720), density_px_per_m: 2000.0, app_id: Some(app.into()), title: None }
    }

    fn start() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stardust-cylinder/layout.sock");
        let d = LayoutDaemon::bind(&path, CylinderState::default()).unwrap();
        std::thread::spawn(move || d.serve());
        (dir, path)
    }

    fn assigned(c: &mut SocketLayoutClient) -> (PanelSlot, CylinderState) {
        match c.recv_timeout(T).unwrap() {
            Some(LayoutEvent::Assigned { slot, group }) => (slot, group),
            other => panic!("expected Assigned, got {other:?}"),
        }
    }

    fn changed(c: &mut SocketLayoutClient) -> (PanelSlot, CylinderState) {
        match c.recv_timeout(T).unwrap() {
            Some(LayoutEvent::GroupChanged { slot, group }) => (slot, group),
            other => panic!("expected GroupChanged, got {other:?}"),
        }
    }

    #[test]
    fn two_fictional_panels_get_disjoint_yaw_slots_over_socket() {
        let (_dir, path) = start();
        let mut ghostty = SocketLayoutClient::connect_to(&path).unwrap();
        let mut mirror = SocketLayoutClient::connect_to(&path).unwrap();
        ghostty.send(LayoutRequest::Hello { metrics: metrics(1280, "com.mitchellh.ghostty") }).unwrap();
        let (a, g) = assigned(&mut ghostty);
        mirror.send(LayoutRequest::Hello { metrics: metrics(2560, "host-mirror-0") }).unwrap();
        let (b, _) = assigned(&mut mirror);
        assert_ne!(a.id, b.id);
        let r = g.radius_m;
        assert!((a.yaw_rad - b.yaw_rad).abs() >= half_arc_rad(&a, r) + half_arc_rad(&b, r) - 1e-5);
        // gap ahead is empty
        let gap = g.gap_m / 2.0 / r;
        for s in [&a, &b] {
            assert!(s.yaw_rad.abs() - half_arc_rad(s, r) >= gap - 1e-5);
        }
        // existing worker sees its (unchanged) slot after the newcomer
        let (a2, _) = changed(&mut ghostty);
        assert_eq!(a2.id, a.id);
    }

    #[test]
    fn group_grab_and_unregister_propagate() {
        let (_dir, path) = start();
        let mut c1 = SocketLayoutClient::connect_to(&path).unwrap();
        let mut c2 = SocketLayoutClient::connect_to(&path).unwrap();
        let mut c3 = SocketLayoutClient::connect_to(&path).unwrap();
        c1.send(LayoutRequest::Hello { metrics: metrics(1000, "a") }).unwrap();
        let (s1, _) = assigned(&mut c1);
        c2.send(LayoutRequest::Hello { metrics: metrics(1000, "b") }).unwrap();
        let _ = assigned(&mut c2);
        let _ = changed(&mut c1);
        c3.send(LayoutRequest::Hello { metrics: metrics(1000, "c") }).unwrap();
        let (s3, _) = assigned(&mut c3);
        let _ = changed(&mut c1);
        let _ = changed(&mut c2);
        assert!(s3.yaw_rad > s1.yaw_rad); // third goes right, outside the first

        c2.send(LayoutRequest::GroupGrab { delta_yaw: 0.25, delta_height: -0.1 }).unwrap();
        for c in [&mut c1, &mut c2, &mut c3] {
            let (_, g) = changed(c);
            assert!((g.yaw_rad - 0.25).abs() < 1e-6 && (g.height_m + 0.1).abs() < 1e-6);
        }

        // drop c1 without Bye: daemon repacks in order -> c2 first-right, c3 first-left
        drop(c1);
        let (s3b, _) = changed(&mut c3);
        let (s2b, _) = changed(&mut c2);
        assert!((s2b.yaw_rad - s1.yaw_rad).abs() < 1e-5);
        assert!((s3b.yaw_rad + s1.yaw_rad).abs() < 1e-5);

        c2.send(LayoutRequest::SetRadius { radius_m: 1.4 }).unwrap();
        let (_, g) = changed(&mut c2);
        assert_eq!(g.radius_m, 1.4);
    }

    #[test]
    fn bind_refuses_live_and_replaces_stale_socket() {
        let (_dir, path) = start();
        // wait for listener thread (bind already happened, so connect works immediately)
        assert_eq!(LayoutDaemon::bind(&path, CylinderState::default()).err().unwrap().kind(), io::ErrorKind::AddrInUse);
        let dir = tempfile::tempdir().unwrap();
        let stale = dir.path().join("layout.sock");
        drop(UnixListener::bind(&stale).unwrap()); // leaves a dead socket file
        assert!(LayoutDaemon::bind(&stale, CylinderState::default()).is_ok());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn layout_session_forwards_group_grab_and_applies() {
        use crate::panel::PanelWorker;
        let (_dir, path) = start();
        let mut c = SocketLayoutClient::connect_to(&path).unwrap();
        c.send(LayoutRequest::Hello { metrics: metrics(1000, "sess") }).unwrap();
        let (slot, group) = assigned(&mut c);
        let mut session = LayoutSession::take_over(c, Some(slot.id));
        let mut worker = PanelWorker::new(metrics(1000, "sess"));
        let p0 = apply_layout_event(
            &mut worker,
            &LayoutEvent::Assigned { slot: slot.clone(), group: group.clone() },
        )
        .unwrap();
        assert!((p0.position.y - group.height_m).abs() < 1e-6);

        session
            .send(LayoutRequest::GroupGrab { delta_yaw: 0.3, delta_height: 0.05 })
            .unwrap();
        let ev = session.recv().await.expect("GroupChanged");
        let p1 = apply_layout_event(&mut worker, &ev).expect("placement");
        match ev {
            LayoutEvent::GroupChanged { group: g, .. } => {
                assert!((g.yaw_rad - 0.3).abs() < 1e-6);
                assert!((g.height_m - 0.05).abs() < 1e-6);
                assert!((p1.position.y - 0.05).abs() < 1e-6);
            }
            other => panic!("expected GroupChanged, got {other:?}"),
        }

        session.request_sender().set_radius(1.25).unwrap();
        let ev = session.recv().await.expect("radius GroupChanged");
        let _ = apply_layout_event(&mut worker, &ev).unwrap();
        match ev {
            LayoutEvent::GroupChanged { group: g, .. } => assert_eq!(g.radius_m, 1.25),
            other => panic!("expected GroupChanged, got {other:?}"),
        }
    }

    #[test]
    fn apply_layout_event_ignores_shutdown() {
        use crate::panel::PanelWorker;
        let mut worker = PanelWorker::new(metrics(800, "x"));
        assert!(apply_layout_event(&mut worker, &LayoutEvent::Shutdown).is_none());
        assert!(worker.placement.is_none());
    }
}
