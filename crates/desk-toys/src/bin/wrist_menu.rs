//! A watch-style menu on the back of your left wrist that turns little widgets and desk toys
//! on and off, and can re-place a running toy in front of you.
//!
//! Tap the watch face to open it. Tap the left/center of a row to toggle that item. When it is
//! on, tap the right **"here"** strip to kill+respawn it with a fresh head-relative
//! `STARDUST_STARTUP_TOKEN` (toys have no relocate IPC yet — place = respawn-at-head).
//! New widgets spawn upright in front of you (offset relative to your head, yaw only).
//!
//! Items: `~/.config/io.github.TaylorBeeston.WristMenu/items.txt` (written with defaults on first run):
//!     label | command | right,up,forward offset from your head [| kind]
//! Optional `kind` is `toy` (default) or `widget` — reserved for a future toychest; ignored for
//! behaviour today. What's on is remembered in `~/.local/state/wrist-menu/enabled` and restored
//! at startup. Widgets already running (e.g. from startup.sh) are adopted.

use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};
use stardust_xr_asteroids::{
    ClientState, Context, CustomElement as _, Element, Migrate, Reify, Tasker, Transformable as _,
    elements::{Lines, Spatial, Text, Tracked},
    Entity,
};
use desk_toys::touch::{Touch, TouchFrame};
use stardust_xr_fusion::{
    client::{Client, DefaultHandler, FrameInfo},
    drawable::{Line, XAlign, YAlign},
    fields::Shape,
    spatial::{Spatial as FSpatial, SpatialExt, Transform},
    suis::Chirality,
    tracked::{Tracked as FTracked, TrackedExt},
    types::{Color, rgba_linear},
};
use stardust_xr_molecules::lines::{LineExt, circle, rounded_rectangle};
use std::{
    collections::{HashMap, HashSet},
    f32::consts::FRAC_PI_2,
    path::PathBuf,
    process::Command,
    sync::Arc,
    time::{Duration, Instant},
};
use tracing::{info, warn};

const ROW_H: f32 = 0.017;
const ROW_W: f32 = 0.085;
const FACE_R: f32 = 0.016;
const AUTO_CLOSE: Duration = Duration::from_secs(12);

const DEFAULT_ITEMS: &str = "\
# Wrist menu items: label | command | spawn offset from your head (right, up, forward is -) [| kind]
# kind is optional: toy (default) or widget — for a future toychest; behaviour is the same today.
Battery | battery-widget | 0.18,-0.2,-0.45 | widget
Spin sign | spin-sign | -0.22,-0.3,-0.45 | toy
Newton's cradle | newtons-cradle | 0.0,-0.32,-0.4 | toy
";

/// Width of the right-hand "here" / place strip on an open row.
const PLACE_STRIP: f32 = 0.022;

/// Optional tag in `items.txt` for a future toychest (stash vs active). Behaviour is identical today.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ItemKind {
    #[default]
    Toy,
    Widget,
}

impl ItemKind {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "" | "toy" => Some(Self::Toy),
            "widget" => Some(Self::Widget),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub label: String,
    pub command: Vec<String>,
    pub offset: Vec3,
    pub kind: ItemKind,
}
impl Item {
    /// process name we look for in /proc (argv[0] basename)
    fn exe(&self) -> &str {
        self.command[0].rsplit('/').next().unwrap()
    }
}

/// Menu / lifecycle request. `PlaceHere` is kill+respawn with a fresh head-relative token —
/// desk toys do not expose relocate IPC yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Start(String),
    Stop(String),
    PlaceHere(String),
}

impl Request {
    pub fn label(&self) -> &str {
        match self {
            Self::Start(l) | Self::Stop(l) | Self::PlaceHere(l) => l,
        }
    }
}

/// Pure toggle decision used by the touch handler and unit-tested without Stardust.
pub fn toggle_request(label: &str, running_or_starting: bool) -> Request {
    if running_or_starting {
        Request::Stop(label.to_string())
    } else {
        Request::Start(label.to_string())
    }
}

/// Expand the queue for the frame processor: `PlaceHere` becomes stop then start so enabled-set
/// persistence and /proc adoption stay consistent. Order is preserved.
pub fn expand_requests(requests: impl IntoIterator<Item = Request>) -> Vec<Request> {
    let mut out = Vec::new();
    for r in requests {
        match r {
            Request::PlaceHere(label) => {
                out.push(Request::Stop(label.clone()));
                out.push(Request::Start(label));
            }
            other => out.push(other),
        }
    }
    out
}

pub fn parse_items(text: &str) -> Vec<Item> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let mut parts = l.split('|').map(str::trim);
            let label = parts.next()?.to_string();
            let command: Vec<String> = parts.next()?.split_whitespace().map(String::from).collect();
            let offset = parts
                .next()
                .and_then(|o| {
                    let v: Vec<f32> = o.split(',').map(|p| p.trim().parse().ok()).collect::<Option<_>>()?;
                    (v.len() == 3).then(|| Vec3::new(v[0], v[1], v[2]))
                })
                .unwrap_or(Vec3::new(0.0, -0.25, -0.45));
            // Optional 4th field: kind. Unknown values are ignored (stay Toy) so old files still load.
            let kind = parts.next().and_then(ItemKind::parse).unwrap_or_default();
            (!label.is_empty() && !command.is_empty()).then_some(Item {
                label,
                command,
                offset,
                kind,
            })
        })
        .collect()
}

fn config_dir() -> PathBuf {
    desk_toys::migrate_app_config("org.stardustxr.WristMenu", "io.github.TaylorBeeston.WristMenu");
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap()).join(".config"))
        .join("io.github.TaylorBeeston.WristMenu")
}
fn enabled_file() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap()).join(".local/state"))
        .join("wrist-menu/enabled")
}

fn load_items() -> Vec<Item> {
    let path = config_dir().join("items.txt");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        let _ = std::fs::create_dir_all(config_dir());
        let _ = std::fs::write(&path, DEFAULT_ITEMS);
        DEFAULT_ITEMS.to_string()
    });
    parse_items(&text)
}

/// `None` = never saved yet (first run: everything on, matching the old startup.sh).
fn load_enabled() -> Option<HashSet<String>> {
    let text = std::fs::read_to_string(enabled_file()).ok()?;
    Some(text.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect())
}
fn save_enabled(items: &[Item], running: &HashSet<String>) {
    let path = enabled_file();
    let _ = std::fs::create_dir_all(path.parent().unwrap());
    let text: String = items
        .iter()
        .filter(|i| running.contains(&i.label))
        .map(|i| format!("{}\n", i.label))
        .collect();
    let _ = std::fs::write(path, text);
}

/// pids (not zombies) whose argv[0] basename is `exe` and that talk to the same Stardust
/// instance we do (unset STARDUST_INSTANCE = the default one).
fn pids_of(exe: &str) -> Vec<i32> {
    let mine = std::env::var("STARDUST_INSTANCE").ok();
    let Ok(dir) = std::fs::read_dir("/proc") else { return vec![] };
    dir.filter_map(|e| e.ok()?.file_name().to_str()?.parse::<i32>().ok())
        .filter(|pid| {
            let base = format!("/proc/{pid}");
            let Ok(cmdline) = std::fs::read(format!("{base}/cmdline")) else { return false };
            let argv0 = cmdline.split(|b| *b == 0).next().unwrap_or_default();
            let name = String::from_utf8_lossy(argv0);
            if name.rsplit('/').next() != Some(exe) {
                return false;
            }
            let stat = std::fs::read_to_string(format!("{base}/stat")).unwrap_or_default();
            if stat.rsplit(')').next().is_some_and(|rest| rest.trim_start().starts_with('Z')) {
                return false;
            }
            let environ = std::fs::read(format!("{base}/environ")).unwrap_or_default();
            let theirs = environ
                .split(|b| *b == 0)
                .find_map(|kv| kv.strip_prefix(b"STARDUST_INSTANCE="))
                .map(|v| String::from_utf8_lossy(v).into_owned());
            theirs == mine
        })
        .collect()
}

/// Upright pose `offset` (right/up/forward) from the head, in the client root's space.
async fn spawn_pose(client: &Client<DefaultHandler>, offset: Vec3) -> Transform {
    let head = async {
        let hmd = FTracked::hmd_spatial().await.ok()?;
        client.spatial_interface().get_relative_transform(client.root().clone(), hmd).await.ok()?.ok()
    }
    .await;
    let (pos, rot) = match head {
        Some(t) => (Vec3::from(t.translation), Quat::from(t.rotation)),
        None => (Vec3::ZERO, Quat::IDENTITY),
    };
    let fwd = rot * Vec3::NEG_Z;
    let yaw = Quat::from_rotation_y((-fwd.x).atan2(-fwd.z));
    Transform::from_translation_rotation(pos + yaw * offset, yaw)
}

/// Launched through a short-lived `sh` that backgrounds it, so the widget is never our child:
/// nothing to reap, and it outlives the menu (it's the session's, like startup.sh's).
async fn spawn(client: Arc<Client<DefaultHandler>>, item: Item) -> std::io::Result<()> {
    let pose = spawn_pose(&client, item.offset).await;
    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg("\"$@\" &").arg("sh").args(&item.command);
    match FSpatial::new(&client, client.root(), pose).await {
        Ok((_spatial, spatial_ref)) => match client.generate_startup_token(spatial_ref).await {
            Ok(token) => {
                cmd.env("STARDUST_STARTUP_TOKEN", token);
            }
            Err(e) => warn!(error = %e, "no startup token; widget spawns at the origin"),
        },
        Err(e) => warn!(error = %e, "couldn't make spawn spatial"),
    }
    tokio::task::spawn_blocking(move || cmd.status().map(drop)).await?
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct State {
    #[serde(skip)]
    items: Vec<Item>,
    #[serde(skip)]
    running: HashSet<String>,
    /// label -> when we asked it to start (shows "…" until it shows up in /proc)
    #[serde(skip)]
    starting: HashMap<String, Instant>,
    #[serde(skip)]
    requests: Vec<Request>,
    #[serde(skip)]
    open: bool,
    #[serde(skip)]
    last_touch: Option<Instant>,
    #[serde(skip)]
    last_scan: Option<Instant>,
    #[serde(skip)]
    restored: bool,
    #[serde(skip)]
    palm: bool,
    #[serde(skip)]
    grip: bool,
    #[serde(skip)]
    desk: bool,
    #[serde(skip)]
    hovered: Option<Target>,
    #[serde(skip)]
    last_press: Option<Instant>,
}

impl Migrate for State {
    type Old = State;
}

impl State {
    fn touch(&mut self) {
        self.last_touch = Some(Instant::now());
    }
    fn toggle(&mut self, label: &str) {
        self.touch();
        let busy = self.running.contains(label) || self.starting.contains_key(label);
        self.requests.push(toggle_request(label, busy));
    }
    /// Re-place a running (or starting) toy in front of the head via kill+respawn.
    /// No-op if the item is off — use toggle/start first.
    fn place_here(&mut self, label: &str) {
        self.touch();
        let busy = self.running.contains(label) || self.starting.contains_key(label);
        if busy {
            self.requests.push(Request::PlaceHere(label.to_string()));
        } else {
            self.requests.push(Request::Start(label.to_string()));
        }
    }
    fn scan(&mut self) {
        self.running = self
            .items
            .iter()
            .filter(|i| !pids_of(i.exe()).is_empty())
            .map(|i| i.label.clone())
            .collect();
        let running = &self.running;
        self.starting
            .retain(|label, t| !running.contains(label) && t.elapsed() < Duration::from_secs(8));
    }
}

impl ClientState for State {
    const APP_ID: &'static str = "io.github.TaylorBeeston.WristMenu";

    fn initial_state_update(&mut self) {
        self.items = load_items();
        self.desk = std::env::var_os("WRIST_MENU_TEST").is_some();
        self.open = self.desk;
        info!(items = ?self.items.iter().map(|i| &i.label).collect::<Vec<_>>(), "wrist menu");
    }

    fn on_frame(&mut self, context: &Context, _info: &FrameInfo) {
        if self.last_scan.is_none_or(|t| t.elapsed() > Duration::from_millis(500)) {
            self.last_scan = Some(Instant::now());
            self.scan();
            if !self.restored {
                self.restored = true;
                let enabled = load_enabled();
                for item in &self.items {
                    let want = enabled.as_ref().is_none_or(|e| e.contains(&item.label));
                    if want && !self.running.contains(&item.label) {
                        self.requests.push(Request::Start(item.label.clone()));
                    }
                }
            }
        }
        if self.open && !self.desk && self.last_touch.is_some_and(|t| t.elapsed() > AUTO_CLOSE) {
            self.open = false;
        }
        let requests = expand_requests(std::mem::take(&mut self.requests));
        if requests.is_empty() {
            return;
        }
        for req in requests {
            let label = req.label().to_string();
            let Some(item) = self.items.iter().find(|i| i.label == label).cloned() else { continue };
            match req {
                Request::Start(_) => {
                    info!(item = %label, "starting");
                    self.starting.insert(label.clone(), Instant::now());
                    let client = context.stardust_client.clone();
                    tokio::spawn(async move {
                        if let Err(e) = spawn(client, item).await {
                            warn!(item = %label, error = %e, "couldn't start");
                        }
                    });
                }
                Request::Stop(_) => {
                    info!(item = %label, "stopping");
                    self.starting.remove(&label);
                    for pid in pids_of(item.exe()) {
                        let _ = Command::new("kill").arg(pid.to_string()).status();
                    }
                    self.running.remove(&label);
                }
                // PlaceHere is expanded away above.
                Request::PlaceHere(_) => unreachable!("expand_requests flattens PlaceHere"),
            }
        }
        let mut wanted = self.running.clone();
        wanted.extend(self.starting.keys().cloned());
        save_enabled(&self.items, &wanted);
    }
}

fn status_color(on: bool, starting: bool) -> Color {
    if starting {
        rgba_linear!(1.0, 0.7, 0.15, 1.0)
    } else if on {
        rgba_linear!(0.2, 0.95, 0.45, 1.0)
    } else {
        rgba_linear!(0.35, 0.37, 0.42, 1.0)
    }
}

fn xy(mut l: Line) -> Line {
    for p in &mut l.points {
        p.point = [p.point.x, p.point.z, 0.0].into();
    }
    l
}

fn dot(color: Color) -> Lines {
    // packed rings = a solid little LED
    Lines::new((1..=3).map(|k| xy(circle(16, 0.0, 0.0011 * k as f32)).thickness(0.0014).color(color)))
}

/// What a poke at a point on the menu would hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Face,
    /// Left/center of a row: toggle on/off.
    RowToggle(usize),
    /// Right strip of a row: place in front (respawn-at-head).
    RowPlace(usize),
    Close,
}

const GOLD: Color = rgba_linear!(0.85, 0.65, 0.2, 1.0);
const DIM: Color = rgba_linear!(0.45, 0.5, 0.6, 1.0);
const BRIGHT: Color = rgba_linear!(1.0, 0.95, 0.7, 1.0);
/// fingertip within this of the menu plane counts as touching it
const SLAB: f32 = 0.006;

/// Menu layout, all in the menu's own plane (XY, facing +Z).
pub struct Layout {
    pub rows: usize,
}
impl Layout {
    pub fn height(&self) -> f32 {
        (self.rows as f32 + 1.0) * ROW_H + 0.008
    }
    pub fn row_y(&self, i: usize) -> f32 {
        self.height() / 2.0 - 0.004 - ROW_H / 2.0 - i as f32 * ROW_H
    }
    pub fn hit(&self, open: bool, x: f32, y: f32) -> Option<Target> {
        if !open {
            return (x.hypot(y) < FACE_R + 0.004).then_some(Target::Face);
        }
        if x.abs() > ROW_W / 2.0 {
            return None;
        }
        (0..=self.rows).find(|&i| (y - self.row_y(i)).abs() < ROW_H / 2.0).map(|i| {
            if i == self.rows {
                Target::Close
            } else if x > ROW_W / 2.0 - PLACE_STRIP {
                Target::RowPlace(i)
            } else {
                Target::RowToggle(i)
            }
        })
    }
}

/// Pokes from any fingertip that isn't on the menu's own (left) hand, from either side of the
/// menu (so a flipped mount still works), plus pointer clicks. Returns (pressed, hovered).
pub fn pokes(layout: &Layout, open: bool, frame: &TouchFrame) -> (Option<Target>, Option<Target>) {
    let mut pressed = None;
    let mut hovered = None;
    for c in frame.contacts.iter().filter(|c| c.poker && c.left != Some(true)) {
        let prev = c.pos - c.vel * frame.dt;
        let Some(t) = layout.hit(open, c.pos.x, c.pos.y) else { continue };
        if c.pos.z.abs() < 0.03 {
            hovered = Some(t);
        }
        let entered = c.pos.z.abs() < SLAB && prev.z.abs() >= SLAB;
        let crossed = c.pos.z.signum() != prev.z.signum() && prev.z.abs() < 0.05;
        if entered || crossed {
            pressed = Some(t);
        }
    }
    for r in frame.rays.iter() {
        if r.dir.z.abs() < 1e-3 {
            continue;
        }
        let p = r.origin + r.dir * (-r.origin.z / r.dir.z);
        if let Some(t) = layout.hit(open, p.x, p.y) {
            hovered = hovered.or(Some(t));
            if r.select_started {
                pressed = Some(t);
            }
        }
    }
    (pressed, hovered)
}

fn on_touch(state: &mut State, frame: &TouchFrame) {
    let layout = Layout { rows: state.items.len() };
    let (pressed, hovered) = pokes(&layout, state.open, frame);
    state.hovered = hovered;
    let Some(target) = pressed else { return };
    if state.last_press.is_some_and(|t| t.elapsed() < Duration::from_millis(450)) {
        return; // one poke = one press, even if the tip jitters through the plane
    }
    state.last_press = Some(Instant::now());
    info!(?target, "poke");
    match target {
        Target::Face => {
            state.open = true;
            state.touch();
        }
        Target::Close => state.open = false,
        Target::RowToggle(i) => {
            if let Some(label) = state.items.get(i).map(|i| i.label.clone()) {
                state.toggle(&label);
            }
        }
        Target::RowPlace(i) => {
            if let Some(label) = state.items.get(i).map(|i| i.label.clone()) {
                state.place_here(&label);
            }
        }
    }
}

impl State {
    fn outline(&self, target: Target, line: Line) -> Lines {
        let color = if self.hovered == Some(target) { BRIGHT } else { DIM };
        Lines::new([line.thickness(0.001).color(color)])
    }

    fn watch_face(&self) -> impl Element<Self> {
        let on = self.running.len();
        let ring = if self.hovered == Some(Target::Face) { BRIGHT } else { GOLD };
        Spatial::default()
            .build()
            .child(
                Lines::new([
                    xy(circle(40, 0.0, FACE_R)).thickness(0.0025).color(ring),
                    xy(circle(40, 0.0, FACE_R - 0.003)).thickness(0.0008).color(DIM),
                ])
                .build(),
            )
            .child(
                Text::new(format!("{on}"))
                    .character_height(0.012)
                    .align_x(XAlign::Center)
                    .align_y(YAlign::Center)
                    .pos([0.0, 0.002, 0.001])
                    .build(),
            )
            .child(
                Text::new("toys")
                    .character_height(0.0045)
                    .color(rgba_linear!(0.7, 0.75, 0.85, 1.0))
                    .align_x(XAlign::Center)
                    .align_y(YAlign::Center)
                    .pos([0.0, -0.0075, 0.001])
                    .build(),
            )
    }

    fn panel(&self) -> impl Element<Self> {
        let layout = Layout { rows: self.items.len() };
        let item_rows = self.items.iter().enumerate().map(|(i, item)| {
            let on = self.running.contains(&item.label);
            let starting = self.starting.contains_key(&item.label);
            let status = if starting {
                "…"
            } else if on {
                "ON"
            } else {
                "OFF"
            };
            // Right strip is the place/here target when the toy is up (or starting); still shown
            // dim when off so the hit zone matches Layout::hit.
            let place_label = if on || starting { "here" } else { "·" };
            let place_color = if self.hovered == Some(Target::RowPlace(i)) {
                BRIGHT
            } else if on || starting {
                rgba_linear!(0.55, 0.75, 1.0, 1.0)
            } else {
                DIM
            };
            Spatial::default()
                .pos([0.0, layout.row_y(i), 0.0])
                .build()
                .child(
                    self.outline(
                        Target::RowToggle(i),
                        rounded_rectangle(ROW_W - PLACE_STRIP - 0.002, ROW_H * 0.86, 0.003, 3),
                    )
                    .pos([-(PLACE_STRIP + 0.002) / 2.0, 0.0, 0.0])
                    .build(),
                )
                .child(
                    self.outline(
                        Target::RowPlace(i),
                        rounded_rectangle(PLACE_STRIP - 0.002, ROW_H * 0.86, 0.002, 2),
                    )
                    .pos([(ROW_W - PLACE_STRIP) / 2.0, 0.0, 0.0])
                    .build(),
                )
                .child(dot(status_color(on, starting)).pos([-ROW_W / 2.0 + 0.007, 0.0, 0.001]).build())
                .child(
                    Text::new(&item.label)
                        .character_height(0.006)
                        .align_x(XAlign::Left)
                        .align_y(YAlign::Center)
                        .pos([-ROW_W / 2.0 + 0.013, 0.0, 0.001])
                        .build(),
                )
                .child(
                    Text::new(status)
                        .character_height(0.0045)
                        .color(status_color(on, starting))
                        .align_x(XAlign::Right)
                        .align_y(YAlign::Center)
                        .pos([ROW_W / 2.0 - PLACE_STRIP - 0.003, 0.0, 0.001])
                        .build(),
                )
                .child(
                    Text::new(place_label)
                        .character_height(0.004)
                        .color(place_color)
                        .align_x(XAlign::Center)
                        .align_y(YAlign::Center)
                        .pos([(ROW_W - PLACE_STRIP) / 2.0, 0.0, 0.001])
                        .build(),
                )
        });
        Spatial::default()
            .build()
            .child(
                Lines::new([rounded_rectangle(ROW_W + 0.008, layout.height(), 0.006, 4)
                    .thickness(0.0015)
                    .color(GOLD)])
                .build(),
            )
            .children(item_rows)
            .child(
                Spatial::default()
                    .pos([0.0, layout.row_y(layout.rows), 0.0])
                    .build()
                    .child(self.outline(Target::Close, rounded_rectangle(ROW_W * 0.5, ROW_H * 0.8, 0.003, 3)).build())
                    .child(
                        Text::new("close")
                            .character_height(0.0055)
                            .color(rgba_linear!(0.7, 0.75, 0.85, 1.0))
                            .align_x(XAlign::Center)
                            .align_y(YAlign::Center)
                            .pos([0.0, 0.0, 0.001])
                            .build(),
                    ),
            )
    }

    fn menu(&self, transform: Transform) -> impl Element<Self> {
        let layout = Layout { rows: self.items.len() };
        let (w, h) = if self.open { (ROW_W + 0.02, layout.height() + 0.01) } else { (FACE_R * 2.0 + 0.01, FACE_R * 2.0 + 0.01) };
        // thick field so fingertips approaching from either side are reported
        Entity::new(Shape::Box { size: [w, h, 0.06].into() })
            .pos(transform.translation)
            .rot(transform.rotation)
            .component(Touch::new(on_touch))
            .build()
            .maybe_child((!self.open).then(|| self.watch_face()))
            .maybe_child(self.open.then(|| self.panel()))
    }
}

impl Reify for State {
    fn reify(&self, _context: &Context, _tasks: impl Tasker<Self>, _props: ()) -> impl Element<Self> {
        // back of the left wrist (+Z from the palm toward the wrist, +Y out of the back of the
        // hand), facing out of the back of the hand. Raised like a watch, the thumb (+X) points
        // at you, so text "up" is the pinky side (-X).
        let watch = Quat::from_rotation_y(FRAC_PI_2) * Quat::from_rotation_x(-FRAC_PI_2);
        Spatial::default()
            .build()
            .child(
                Tracked::hand_palm(Chirality::Left, |s: &mut State, t| s.palm = t)
                    .build()
                    .maybe_child(self.palm.then(|| {
                        // 4.5 cm off the palm joint: clear of the wrist/forearm so it's pokeable
                        self.menu(Transform::from_translation_rotation([0.0, 0.045, 0.075], watch))
                    })),
            )
            .child(
                Tracked::controller_grip(Chirality::Left, |s: &mut State, t| s.grip = t)
                    .build()
                    .maybe_child((self.grip && !self.palm).then(|| {
                        self.menu(Transform::from_translation_rotation(
                            [0.0, 0.035, 0.09],
                            Quat::from_rotation_x(-FRAC_PI_2),
                        ))
                    })),
            )
            .maybe_child(self.desk.then(|| {
                self.menu(Transform::from_translation([0.0, -0.05, -0.22]))
            }))
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    desk_toys::migrate_app_config("org.stardustxr.WristMenu", "io.github.TaylorBeeston.WristMenu");
    desk_toys::init_tracing("wrist_menu=info");
    stardust_xr_asteroids::client::run::<State>(&[]).await.unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_default_items_and_skips_comments() {
        let items = parse_items(DEFAULT_ITEMS);
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].kind, ItemKind::Widget);
        assert_eq!(items[1].label, "Spin sign");
        assert_eq!(items[1].exe(), "spin-sign");
        assert_eq!(items[1].kind, ItemKind::Toy);
        assert_eq!(items[2].offset, Vec3::new(0.0, -0.32, -0.4));
        assert_eq!(items[2].kind, ItemKind::Toy);
    }

    fn tip(x: f32, y: f32, z: f32, vz: f32, left: Option<bool>) -> TouchFrame {
        TouchFrame {
            dt: 1.0 / 90.0,
            contacts: vec![desk_toys::touch::Contact {
                pos: Vec3::new(x, y, z),
                vel: Vec3::new(0.0, 0.0, vz),
                radius: 0.008,
                pinch: None,
                poker: true,
                left,
            }],
            ..Default::default()
        }
    }

    #[test]
    fn poking_a_row_from_either_side_presses_it_but_the_wrist_hand_never_does() {
        let l = Layout { rows: 3 };
        let y = l.row_y(1);
        // left/center of the row = toggle
        assert_eq!(
            pokes(&l, true, &tip(0.01, y, 0.004, -0.5, Some(false))).0,
            Some(Target::RowToggle(1))
        );
        // ...or from behind (mount flipped)
        assert_eq!(
            pokes(&l, true, &tip(0.01, y, -0.004, 0.5, None)).0,
            Some(Target::RowToggle(1))
        );
        // resting in the plane: hover, not a new press
        assert_eq!(
            pokes(&l, true, &tip(0.01, y, 0.001, 0.0, None)),
            (None, Some(Target::RowToggle(1)))
        );
        // the left hand (the one wearing the menu) is ignored
        assert_eq!(pokes(&l, true, &tip(0.01, y, 0.004, -0.5, Some(true))).0, None);
        // last row is close; closed menu only has the face
        assert_eq!(
            pokes(&l, true, &tip(0.0, l.row_y(3), 0.004, -0.5, None)).0,
            Some(Target::Close)
        );
        assert_eq!(pokes(&l, false, &tip(0.0, 0.0, 0.004, -0.5, None)).0, Some(Target::Face));
    }

    #[test]
    fn right_strip_of_a_row_is_the_place_here_target() {
        let l = Layout { rows: 2 };
        let y = l.row_y(0);
        let x_place = ROW_W / 2.0 - PLACE_STRIP / 2.0;
        assert_eq!(l.hit(true, x_place, y), Some(Target::RowPlace(0)));
        assert_eq!(
            pokes(&l, true, &tip(x_place, y, 0.004, -0.5, None)).0,
            Some(Target::RowPlace(0))
        );
        // just left of the strip stays toggle
        let x_toggle = ROW_W / 2.0 - PLACE_STRIP - 0.002;
        assert_eq!(l.hit(true, x_toggle, y), Some(Target::RowToggle(0)));
    }

    #[test]
    fn bad_offsets_fall_back_and_blank_or_commandless_lines_are_skipped() {
        let text = "  # comment\n\nA | a | 1,2\nB | b | x,y,z\n | nolabel\nC |\n  D|d --x|0.1, 0.2 ,-0.3  \n";
        let items = parse_items(text);
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, ["A", "B", "D"]);
        let fallback = Vec3::new(0.0, -0.25, -0.45);
        assert_eq!(items[0].offset, fallback); // 2 components
        assert_eq!(items[1].offset, fallback); // not numbers
        assert_eq!(items[2].offset, Vec3::new(0.1, 0.2, -0.3));
        assert_eq!(items[2].command, vec!["d", "--x"]);
        assert_eq!(items[2].kind, ItemKind::Toy);
    }

    #[test]
    fn missing_offset_gets_a_default_and_args_split() {
        let items = parse_items("Clock | /opt/bin/clock --big\n bad line\n");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].command, vec!["/opt/bin/clock", "--big"]);
        assert_eq!(items[0].exe(), "clock");
        assert_eq!(items[0].kind, ItemKind::Toy);
    }

    #[test]
    fn optional_kind_tag_is_backward_compatible() {
        let items = parse_items(
            "A | a | 0,0,-1\nB | b | 0,0,-1 | widget\nC | c | 0,0,-1 | TOY\nD | d | 0,0,-1 | mystery\n",
        );
        assert_eq!(items[0].kind, ItemKind::Toy);
        assert_eq!(items[1].kind, ItemKind::Widget);
        assert_eq!(items[2].kind, ItemKind::Toy);
        // unknown kind -> ignored, stays default Toy
        assert_eq!(items[3].kind, ItemKind::Toy);
    }

    #[test]
    fn toggle_request_flips_busy_state() {
        assert_eq!(toggle_request("Spin", false), Request::Start("Spin".into()));
        assert_eq!(toggle_request("Spin", true), Request::Stop("Spin".into()));
    }

    #[test]
    fn place_here_expands_to_stop_then_start() {
        let q = expand_requests([
            Request::PlaceHere("Cradle".into()),
            Request::Start("Battery".into()),
            Request::Stop("Sign".into()),
        ]);
        assert_eq!(
            q,
            vec![
                Request::Stop("Cradle".into()),
                Request::Start("Cradle".into()),
                Request::Start("Battery".into()),
                Request::Stop("Sign".into()),
            ]
        );
    }

    #[test]
    fn place_here_on_state_queues_respawn_when_busy_else_start() {
        let mut state = State::default();
        state.items = parse_items("Spin sign | spin-sign | 0,0,-1 | toy\n");
        state.running.insert("Spin sign".into());
        state.place_here("Spin sign");
        assert_eq!(state.requests, vec![Request::PlaceHere("Spin sign".into())]);

        state.requests.clear();
        state.running.clear();
        state.place_here("Spin sign");
        assert_eq!(state.requests, vec![Request::Start("Spin sign".into())]);
    }
}
