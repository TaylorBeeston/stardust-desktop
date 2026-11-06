//! Labeled hand-pose recorder for tuning palm-reach.
//!
//! Standalone Stardust client (no session restart). Pinch a big label button with
//! either hand → 3s countdown → 3s of both hands' joints (+ best-effort head Tip)
//! to JSONL under ~/.local/state/hand-pose-recorder/.

use chrono::Local;
use gluon_ipc::RefExt;
use stardust_xr_protocol::dir::find_ref_file;
use glam::Vec3;
use serde_json::json;
use stardust_xr_fusion::{
    client::Client,
    drawable::{Lines, LinesExt, Text, TextExt, TextStyle, XAlign, YAlign},
    fields::{Field, FieldExt, Shape},
    spatial::{Spatial, SpatialExt, SpatialRef, Transform},
    suis::InputDataType,
    types::{rgba_linear, QuatF, Vec2F, Vec3F},
};
use stardust_xr_molecules::{
    button::{Button, ButtonSettings, ButtonVisualSettings},
    input_action::InputQueue,
    lines::{self, LineExt},
    UIElement,
};
use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use tokio::sync::broadcast::error::RecvError;
use tracing::{info, warn};

const LABELS: &[&str] = &[
    "Open palm (aimed at panel)",
    "Palm facing me",
    "Back of hand",
    "Relaxed",
    "Fist",
    "Loose fist",
    "Pinch",
    "Point",
];

const COUNTDOWN_S: f32 = 3.0;
const RECORD_S: f32 = 3.0;
const BTN_W: f32 = 0.48;
const BTN_H: f32 = 0.075;
const GAP: f32 = 0.014;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Idle,
    Countdown,
    Recording,
    Done,
}

struct Btn {
    label: String,
    _slot: Spatial,
    button: Button,
    _label_slot: Spatial,
    _label: Text,
}

struct App {
    _anchor: Spatial,
    buttons: Vec<Btn>,
    _status_slot: Spatial,
    status: Text,
    _badge_slot: Spatial,
    badge: Text,
    _counts_slot: Spatial,
    counts_text: Text,
    hand_input: InputQueue,
    _hand_field_spatial: Spatial,
    _hand_field: Field,
    indicator: Lines,
    phase: Phase,
    active_label: Option<String>,
    phase_start: Instant,
    out_dir: PathBuf,
    counts: HashMap<String, u32>,
    record_file: Option<std::fs::File>,
    frame_i: u64,
    /// Ignore button edges for a moment after phase change.
    ignore_btn_until: Instant,
}

fn state_dir() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
        .join(".local/state/hand-pose-recorder")
}

fn load_counts(dir: &Path) -> HashMap<String, u32> {
    fs::read_to_string(dir.join("counts.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_counts(dir: &Path, counts: &HashMap<String, u32>) {
    let _ = fs::create_dir_all(dir);
    let _ = fs::write(
        dir.join("counts.json"),
        serde_json::to_string_pretty(counts).unwrap_or_else(|_| "{}".into()),
    );
}

fn short(label: &str) -> &str {
    match label {
        "Open palm (aimed at panel)" => "Open→",
        "Palm facing me" => "FaceMe",
        "Back of hand" => "Back",
        "Relaxed" => "Relax",
        "Fist" => "Fist",
        "Loose fist" => "Loose",
        "Pinch" => "Pinch",
        "Point" => "Point",
        _ => label,
    }
}

fn counts_summary(counts: &HashMap<String, u32>) -> String {
    let parts: Vec<String> = LABELS
        .iter()
        .map(|l| format!("{}:{}", short(l), counts.get(*l).copied().unwrap_or(0)))
        .collect();
    let mid = parts.len() / 2;
    format!("{}\n{}", parts[..mid].join("  "), parts[mid..].join("  "))
}

fn pose_json(pos: impl Into<Vec3F>, ori: impl Into<QuatF>) -> serde_json::Value {
    let p: Vec3F = pos.into();
    let o: QuatF = ori.into();
    json!({
        "position": [p.x, p.y, p.z],
        "orientation_xyzw": [o.v.x, o.v.y, o.v.z, o.s],
    })
}

fn joint_json(j: &stardust_xr_fusion::suis::Joint) -> serde_json::Value {
    let mut v = pose_json(j.pose.position, j.pose.orientation);
    v["radius"] = json!(j.radius);
    v["distance"] = json!(j.distance);
    v
}

fn finger_json(f: &stardust_xr_fusion::suis::Finger) -> serde_json::Value {
    json!({
        "metacarpal": joint_json(&f.metacarpal),
        "proximal": joint_json(&f.proximal),
        "intermediate": joint_json(&f.intermediate),
        "distal": joint_json(&f.distal),
        "tip": joint_json(&f.tip),
    })
}

fn thumb_json(t: &stardust_xr_fusion::suis::Thumb) -> serde_json::Value {
    json!({
        "metacarpal": joint_json(&t.metacarpal),
        "proximal": joint_json(&t.proximal),
        "distal": joint_json(&t.distal),
        "tip": joint_json(&t.tip),
    })
}

fn hand_json(h: &stardust_xr_fusion::suis::Hand) -> serde_json::Value {
    let right = matches!(h.chirality, stardust_xr_fusion::suis::Chirality::Right);
    let tips = [
        Vec3::from(h.index.tip.pose.position),
        Vec3::from(h.middle.tip.pose.position),
        Vec3::from(h.ring.tip.pose.position),
        Vec3::from(h.little.tip.pose.position),
    ];
    let palm = Vec3::from(h.palm.pose.position);
    let dists: Vec<f32> = tips.iter().map(|t| palm.distance(*t)).collect();
    let mean = dists.iter().sum::<f32>() / 4.0;
    let n = Vec3::from(h.palm_normal());
    let distal = Vec3::from(h.distal_axis());
    json!({
        "chirality": if right { "right" } else { "left" },
        "palm": pose_json(h.palm.pose.position, h.palm.pose.orientation),
        "wrist": pose_json(h.wrist.pose.position, h.wrist.pose.orientation),
        "elbow": h.elbow.as_ref().map(|e| pose_json(e.pose.position, e.pose.orientation)),
        "thumb": thumb_json(&h.thumb),
        "index": finger_json(&h.index),
        "middle": finger_json(&h.middle),
        "ring": finger_json(&h.ring),
        "little": finger_json(&h.little),
        "derived": {
            "palm_normal": [n.x, n.y, n.z],
            "palm_back": [-n.x, -n.y, -n.z],
            "distal_axis": [distal.x, distal.y, distal.z],
            "tip_palm_dists": dists,
            "mean_tip_palm": mean,
            "pinch_gap": Vec3::from(h.thumb.tip.pose.position)
                .distance(Vec3::from(h.index.tip.pose.position)),
        }
    })
}

fn text_style(h: f32) -> TextStyle {
    TextStyle {
        character_height: h,
        color: rgba_linear!(1.0, 1.0, 1.0, 1.0),
        text_align_x: XAlign::Center,
        text_align_y: YAlign::Center,
        font: None,
        bounds: None,
    }
}

impl App {
    async fn new(
        client: &Client<stardust_xr_fusion::client::DefaultHandler>,
        root: &SpatialRef,
    ) -> stardust_xr_fusion::Result<Self> {
        let out_dir = state_dir();
        fs::create_dir_all(&out_dir).ok();
        let counts = load_counts(&out_dir);

        let panel_h = LABELS.len() as f32 * (BTN_H + GAP) + 0.22;
        // Create under client root first (foreign HMD parent often returns InvalidRef).
        let (anchor, anchor_ref) = Spatial::new(
            client,
            root,
            Transform::from_translation([0.0, -0.12, -0.55]),
        )
        .await?;
        // Reparent onto HMD when the server allows it — panel tracks the head.
        if let Some(path) = find_ref_file("stardust-hmd") {
            match SpatialRef::connect(path).await {
                Ok(hmd) => match anchor.set_parent(hmd) {
                    Ok(()) => info!("reparented panel to stardust-hmd (−Z forward of eyes)"),
                    Err(e) => warn!(%e, "HMD reparent failed; panel stays on client root"),
                },
                Err(e) => warn!(%e, "hmd connect failed"),
            }
        }

        let (status_slot, _) = Spatial::new(
            client,
            &anchor_ref,
            Transform::from_translation([0.0, panel_h * 0.5 - 0.02, 0.02]),
        )
        .await?;
        let status = Text::new(
            client,
            &status_slot,
            "Pinch a label with either hand".into(),
            text_style(0.028),
        )
        .await?;

        let (badge_slot, _) = Spatial::new(
            client,
            &anchor_ref,
            Transform::from_translation([0.0, panel_h * 0.5 - 0.055, 0.02]),
        )
        .await?;
        let badge = Text::new(client, &badge_slot, "● IDLE".into(), text_style(0.022)).await?;

        let (counts_slot, _) = Spatial::new(
            client,
            &anchor_ref,
            Transform::from_translation([0.0, -panel_h * 0.5 + 0.04, 0.02]),
        )
        .await?;
        let counts_text = Text::new(
            client,
            &counts_slot,
            counts_summary(&counts),
            text_style(0.015),
        )
        .await?;

        let mut buttons = Vec::new();
        let top = panel_h * 0.5 - 0.11;
        for (i, label) in LABELS.iter().enumerate() {
            let y = top - i as f32 * (BTN_H + GAP);
            let (slot, slot_ref) = Spatial::new(
                client,
                &anchor_ref,
                Transform::from_translation([0.0, y, 0.0]),
            )
            .await?;
            let button = Button::new(
                client,
                &slot_ref,
                Transform::IDENTITY,
                Vec2F::from([BTN_W, BTN_H]),
                ButtonSettings {
                    max_hover_distance: 0.05,
                    visuals: Some(ButtonVisualSettings {
                        line_thickness: 0.0045,
                        accent_color: rgba_linear!(0.3, 0.9, 1.0, 1.0),
                    }),
                },
            )
            .await?;
            let (label_slot, _) = Spatial::new(
                client,
                &slot_ref,
                Transform::from_translation([0.0, 0.0, 0.015]),
            )
            .await?;
            let label_txt = Text::new(
                client,
                &label_slot,
                (*label).to_string(),
                text_style(0.024),
            )
            .await?;
            buttons.push(Btn {
                label: (*label).to_string(),
                _slot: slot,
                button,
                _label_slot: label_slot,
                _label: label_txt,
            });
        }

        let (hand_fs, hand_fref) = Spatial::new(client, &anchor_ref, Transform::IDENTITY).await?;
        let (hand_field, _) = Field::new(client, &hand_fs, Shape::Sphere { radius: 1.5 }).await?;
        let hand_input = InputQueue::new(client, hand_fs.clone(), hand_field.clone(), hand_fref).await?;

        let indicator = Lines::new(client, &anchor, vec![]).await?;

        let mut app = Self {
            _anchor: anchor,
            buttons,
            _status_slot: status_slot,
            status,
            _badge_slot: badge_slot,
            badge,
            _counts_slot: counts_slot,
            counts_text,
            hand_input,
            _hand_field_spatial: hand_fs,
            _hand_field: hand_field,
            indicator,
            phase: Phase::Idle,
            active_label: None,
            phase_start: Instant::now(),
            out_dir,
            counts,
            record_file: None,
            frame_i: 0,
            ignore_btn_until: Instant::now(),
        };
        app.paint_indicator(Phase::Idle);
        Ok(app)
    }

    fn paint_indicator(&self, phase: Phase) {
        let col = match phase {
            Phase::Idle => rgba_linear!(0.55, 0.6, 0.65, 0.4),
            Phase::Countdown => rgba_linear!(1.0, 0.78, 0.12, 0.95),
            Phase::Recording => rgba_linear!(1.0, 0.18, 0.22, 0.98),
            Phase::Done => rgba_linear!(0.15, 0.95, 0.4, 0.98),
        };
        let mut ring = lines::circle(40, 0.0, 0.04);
        ring = ring.thickness(0.007).color(col);
        let y = LABELS.len() as f32 * (BTN_H + GAP) * 0.5 + 0.05;
        for p in &mut ring.points {
            p.point.y += y;
            p.point.z += 0.025;
        }
        let _ = self.indicator.set_lines(vec![ring]);
    }

    fn begin_countdown(&mut self, label: String) {
        info!(%label, "countdown start");
        self.phase = Phase::Countdown;
        self.active_label = Some(label.clone());
        self.phase_start = Instant::now();
        self.ignore_btn_until = Instant::now() + Duration::from_millis(400);
        let _ = self.badge.set_text("● COUNTDOWN");
        let _ = self.status.set_text(format!("Hold pose: {label} — 3…"));
        self.paint_indicator(Phase::Countdown);
    }

    fn begin_recording(&mut self) {
        let label = self.active_label.clone().unwrap_or_else(|| "unknown".into());
        let ts = Local::now().format("%Y%m%d-%H%M%S");
        let slug: String = label
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .collect();
        let path = self.out_dir.join(format!("{ts}_{slug}.jsonl"));
        match OpenOptions::new().create(true).append(true).open(&path) {
            Ok(mut f) => {
                info!(?path, %label, "recording");
                let hdr = json!({
                    "type": "header",
                    "label": label,
                    "started": Local::now().to_rfc3339(),
                    "space": "client_root / panel-local",
                    "note": "head is Tip/Pointer best-effort; derived.palm_normal is protocol −Y face",
                });
                let _ = writeln!(f, "{hdr}");
                self.record_file = Some(f);
                self.frame_i = 0;
                self.phase = Phase::Recording;
                self.phase_start = Instant::now();
                let _ = self.badge.set_text("● RECORDING");
                let _ = self.status.set_text(format!("Recording: {label}"));
                self.paint_indicator(Phase::Recording);
            }
            Err(e) => {
                warn!(%e, "open failed");
                self.phase = Phase::Idle;
                let _ = self.badge.set_text("● IDLE");
                let _ = self.status.set_text("Record failed — check disk");
            }
        }
    }

    fn finish_recording(&mut self) {
        let label = self.active_label.clone().unwrap_or_default();
        if let Some(mut f) = self.record_file.take() {
            let _ = f.flush();
        }
        *self.counts.entry(label.clone()).or_insert(0) += 1;
        save_counts(&self.out_dir, &self.counts);
        let _ = self.counts_text.set_text(counts_summary(&self.counts));
        let n = self.counts.get(&label).copied().unwrap_or(0);
        info!(%label, n, "saved");
        self.phase = Phase::Done;
        self.phase_start = Instant::now();
        self.ignore_btn_until = Instant::now() + Duration::from_millis(500);
        let _ = self.badge.set_text("● DONE");
        let _ = self
            .status
            .set_text(format!("Saved {label} (n={n})"));
        self.paint_indicator(Phase::Done);
    }

    fn dump_frame(&mut self) {
        let Some(f) = self.record_file.as_mut() else {
            return;
        };
        let mut hands = Vec::new();
        let mut head = serde_json::Value::Null;
        for snap in self.hand_input.input().values() {
            match snap.input() {
                InputDataType::Hand { data } => hands.push(hand_json(data)),
                InputDataType::Tip { data } => {
                    if head.is_null() {
                        head = pose_json(data.pose.position, data.pose.orientation);
                        head["kind"] = json!("tip");
                    }
                }
                InputDataType::Pointer { data } => {
                    if head.is_null() {
                        head = pose_json(data.pose.position, data.pose.orientation);
                        head["kind"] = json!("pointer");
                    }
                }
            }
        }
        let row = json!({
            "type": "frame",
            "i": self.frame_i,
            "t_ms": self.phase_start.elapsed().as_millis(),
            "label": self.active_label,
            "hands": hands,
            "head": head,
        });
        let _ = writeln!(f, "{row}");
        self.frame_i += 1;
    }

    fn tick(&mut self) {
        for b in &mut self.buttons {
            let _ = b.button.handle_events();
        }
        let _ = self.hand_input.handle_events();

        if self.phase == Phase::Idle && Instant::now() >= self.ignore_btn_until {
            for b in &self.buttons {
                if b.button.released() {
                    self.begin_countdown(b.label.clone());
                    break;
                }
            }
        }

        let elapsed = self.phase_start.elapsed().as_secs_f32();
        match self.phase {
            Phase::Countdown => {
                let left = (COUNTDOWN_S - elapsed).ceil().max(0.0) as i32;
                if let Some(l) = &self.active_label {
                    let _ = self.status.set_text(format!("Hold pose: {l} — {left}…"));
                }
                if elapsed >= COUNTDOWN_S {
                    self.begin_recording();
                }
            }
            Phase::Recording => {
                self.dump_frame();
                let left = (RECORD_S - elapsed).max(0.0);
                let _ = self.status.set_text(format!(
                    "Recording {} — {:.1}s (f{})",
                    self.active_label.as_deref().unwrap_or("?"),
                    left,
                    self.frame_i
                ));
                if elapsed >= RECORD_S {
                    self.finish_recording();
                }
            }
            Phase::Done => {
                if elapsed >= 1.25 {
                    self.phase = Phase::Idle;
                    self.active_label = None;
                    let _ = self.badge.set_text("● IDLE");
                    let _ = self
                        .status
                        .set_text("Pinch a label with either hand");
                    self.paint_indicator(Phase::Idle);
                }
            }
            Phase::Idle => {}
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    tracing_subscriber::fmt()
        .compact()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("hand_pose_recorder=info".parse().unwrap()),
        )
        .init();

    if std::env::var_os("STARDUST_INSTANCE").is_none() {
        std::env::set_var("STARDUST_INSTANCE", "stardust-0");
    }
    info!(
        instance = %std::env::var("STARDUST_INSTANCE").unwrap(),
        "connecting"
    );

    let (client, root) = Client::connect(&[] as &[&std::path::Path])
        .await
        .expect("connect stardust-server — is the session up?");

    let mut app = App::new(&client, &root)
        .await
        .expect("build hand-pose-recorder UI");
    info!(
        dir = %app.out_dir.display(),
        "READY — floating panel ~0.55m in front of spawn; pinch a label"
    );

    let mut frames = client.frame_receiver();
    loop {
        match frames.recv().await {
            Ok(_) => app.tick(),
            Err(RecvError::Lagged(n)) => warn!(n, "lagged"),
            Err(RecvError::Closed) => break,
        }
    }
}
