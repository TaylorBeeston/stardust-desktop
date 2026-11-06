//! `CurvedPointerPlane`: Asteroids element that routes XR pointer / hand input onto the
//! curved slice. Replaces Flatland's flat `PointerPlane` UV (`x + w/2`) with
//! [`crate::pointer_uv`] on the true arc. Capture volume follows the arc using a union of thin tangent fields.

use crate::field_shape::field_shape_for_slice;
use crate::hit_test::CylinderSliceHitTest;
use crate::palm_wm::{self, ReachArming, ReachConfig};
use crate::pointer_uv::{mesh_local_point_hit, mesh_local_ray_hit, OVERLAY_INSET_M, SliceHit};
use glam::Vec3;
use stardust_xr_asteroids::{Context, CreateInnerInfo, CustomElement, FnWrapper, ValidState};
use stardust_xr_fusion::{
    drawable::{Line, LinePoint, Lines, LinesExt as _},
    fields::{Field, FieldExt, Shape},
    spatial::{Spatial, SpatialExt, Transform},
    suis::InputDataType,
    types::rgba_linear,
};
use stardust_xr_molecules::{
    hover_plane::HoverPlaneSettings,
    input_action::{InputQueue, InputSnapshot, SingleAction},
    lines::{self, LineExt as _},
};

pub const BTN_LEFT: u32 = 0x110;
pub const BTN_RIGHT: u32 = 0x111;
pub const BTN_MIDDLE: u32 = 0x112;

/// Hand / tip hover band either side of the surface (metres), like Flatland's `|z| < 0.2`.
/// Symmetric: on the concave (inside) face the radial offset is negative.
const HAND_HOVER: std::ops::Range<f32> = -0.2..0.2;

/// Where an input lands on this slice, if anywhere. Pure — unit-tested.
pub fn snapshot_hit(h: &CylinderSliceHitTest, input: &InputDataType) -> Option<SliceHit> {
    match input {
        InputDataType::Pointer { data } => {
            mesh_local_ray_hit(h, Vec3::from(data.pose.position).to_array(), Vec3::from(data.direction()).to_array())
        }
        // Flatland pointer_input::hover_point(stable=true) uses pinch, not index tip.
        InputDataType::Hand { data } => mesh_local_point_hit(h, Vec3::from(data.stable_pinch_position()).to_array())
            .filter(|hit| HAND_HOVER.contains(&hit.distance)),
        InputDataType::Tip { data } => mesh_local_point_hit(h, Vec3::from(data.pose.position).to_array())
            .filter(|hit| HAND_HOVER.contains(&hit.distance)),
    }
}

#[derive_where::derive_where(Debug, PartialEq)]
#[allow(clippy::type_complexity)]
pub struct CurvedPointerPlane<State: ValidState> {
    pub hit_test: CylinderSliceHitTest,
    on_motion: FnWrapper<dyn Fn(&mut State, (f32, f32)) + Send + Sync>,
    on_button: FnWrapper<dyn Fn(&mut State, u32, bool) + Send + Sync>,
    on_scroll: FnWrapper<dyn Fn(&mut State, [f32; 2]) + Send + Sync>,
    /// Open-palm highlight (ray hits panel while hand is open, not pinching).
    on_palm_highlight: FnWrapper<dyn Fn(&mut State, bool) + Send + Sync>,
    /// Fist while highlighted: slide panel centre yaw (radians, group-relative).
    on_palm_slide: FnWrapper<dyn Fn(&mut State, f32) + Send + Sync>,
    /// Open hand after a grab: settle into nearest slot.
    on_palm_settle: FnWrapper<dyn Fn(&mut State) + Send + Sync>,
}

impl<State: ValidState> CurvedPointerPlane<State> {
    pub fn new(hit_test: CylinderSliceHitTest) -> Self {
        Self {
            hit_test,
            on_motion: FnWrapper(Box::new(|_, _| {})),
            on_button: FnWrapper(Box::new(|_, _, _| {})),
            on_scroll: FnWrapper(Box::new(|_, _| {})),
            on_palm_highlight: FnWrapper(Box::new(|_, _| {})),
            on_palm_slide: FnWrapper(Box::new(|_, _| {})),
            on_palm_settle: FnWrapper(Box::new(|_| {})),
        }
    }
    /// Called with normalized UV (u right, v down) on the curved slice.
    pub fn on_motion(mut self, f: impl Fn(&mut State, (f32, f32)) + Send + Sync + 'static) -> Self {
        self.on_motion = FnWrapper(Box::new(f));
        self
    }
    pub fn on_button(mut self, f: impl Fn(&mut State, u32, bool) + Send + Sync + 'static) -> Self {
        self.on_button = FnWrapper(Box::new(f));
        self
    }
    pub fn on_scroll(mut self, f: impl Fn(&mut State, [f32; 2]) + Send + Sync + 'static) -> Self {
        self.on_scroll = FnWrapper(Box::new(f));
        self
    }
    pub fn on_palm_highlight(mut self, f: impl Fn(&mut State, bool) + Send + Sync + 'static) -> Self {
        self.on_palm_highlight = FnWrapper(Box::new(f));
        self
    }
    pub fn on_palm_slide(mut self, f: impl Fn(&mut State, f32) + Send + Sync + 'static) -> Self {
        self.on_palm_slide = FnWrapper(Box::new(f));
        self
    }
    pub fn on_palm_settle(mut self, f: impl Fn(&mut State) + Send + Sync + 'static) -> Self {
        self.on_palm_settle = FnWrapper(Box::new(f));
        self
    }
}

fn field_box(h: &CylinderSliceHitTest) -> (Transform, Shape) {
    (Transform::IDENTITY, field_shape_for_slice(h.width_m, h.height_m, h.radius_m))
}

pub struct CurvedPointerInner {
    _root: Spatial,
    field_spatial: Spatial,
    field: Field,
    input: InputQueue,
    left: SingleAction,
    middle: SingleAction,
    right: SingleAction,
    /// Flatland-style hover ray/line + hit cursor (updated every input frame).
    lines: Lines,
    reach: ReachArming,
    /// UV.u at grab start (panel slide).
    palm_grab_u0: Option<f32>,
    palm_grab_yaw0: Option<f32>,
    palm_highlight: bool,
    /// Quantized signature of last `set_lines` payload — skip unchanged frames.
    last_lines_sig: u64,
}

impl<State: ValidState> CustomElement<State> for CurvedPointerPlane<State> {
    type Inner = CurvedPointerInner;
    type Error = stardust_xr_fusion::Error;

    async fn create_inner(&self, context: &Context, info: CreateInnerInfo) -> Result<Self::Inner, Self::Error> {
        let client = &context.stardust_client;
        let (root, root_ref) = Spatial::new(client, &info.parent_space, Transform::IDENTITY).await?;
        let (tf, shape) = field_box(&self.hit_test);
        let (field_spatial, _) = Spatial::new(client, &root_ref, tf).await?;
        let (field, _) = Field::new(client, &field_spatial, shape).await?;
        // Snapshots arrive in `root` space == mesh-local frame.
        let input = InputQueue::new(client, root.clone(), field.clone(), root_ref).await?;
        let lines = Lines::new(client, &root, Vec::new()).await?;
        let _ = info.child_space.set_parent(root.spatial_ref().await?);
        Ok(CurvedPointerInner {
            _root: root,
            field_spatial,
            field,
            input,
            left: SingleAction::default(),
            middle: SingleAction::default(),
            right: SingleAction::default(),
            lines,
            reach: ReachArming::new(ReachConfig::from_env()),
            palm_grab_u0: None,
            palm_grab_yaw0: None,
            palm_highlight: false,
            last_lines_sig: 0,
        })
    }

    fn diff(&self, old: &Self, _context: &Context, inner: &mut Self::Inner) {
        if self.hit_test != old.hit_test {
            let (tf, shape) = field_box(&self.hit_test);
            let _ = inner.field_spatial.set_local_transform(tf);
            let _ = inner.field.set_shape(shape);
        }
    }

    fn frame(
        &self,
        _context: &Context,
        _info: &stardust_xr_fusion::client::FrameInfo,
        state: &mut State,
        inner: &mut Self::Inner,
    ) {
        if !inner.input.handle_events() {
            return;
        }
        let h = self.hit_test;
        let input = inner.input.input();
        let now = std::time::Instant::now();

        // --- Palm reach evaluation (before mouse forwarding) ---
        let mut palm_hit: Option<SliceHit> = None;
        let mut viz_origin: Option<Vec3> = None;
        let mut viz_end: Option<Vec3> = None;
        let mut debug_line: Option<String> = None;
        let was_grabbing = inner.reach.grabbing;
        let was_armed = inner.reach.armed;

        for snap in input.values() {
            let InputDataType::Hand { data } = snap.input() else { continue };
            let palm = Vec3::from(data.palm.pose.position);
            let tips = [
                Vec3::from(data.index.tip.pose.position).to_array(),
                Vec3::from(data.middle.tip.pose.position).to_array(),
                Vec3::from(data.ring.tip.pose.position).to_array(),
                Vec3::from(data.little.tip.pose.position).to_array(),
            ];
            let dists = palm_wm::finger_tip_distances(palm.to_array(), tips);
            let pinch_gap = Vec3::from(data.thumb.tip.pose.position)
                .distance(Vec3::from(data.index.tip.pose.position));
            let _pinching = pinch_gap < palm_wm::PINCH_M;
            let right = matches!(data.chirality, stardust_xr_fusion::suis::Chirality::Right);
            let o = data.palm.pose.orientation;
            let axes = palm_wm::hand_axes_from_quat(palm.to_array(), [o.v.x, o.v.y, o.v.z, o.s], right);
            // Prefer protocol palm_normal if available (same as −Y).
            let proto_n = Vec3::from(data.palm_normal());
            let axes = palm_wm::HandAxes {
                palm_normal: proto_n.to_array(),
                palm_back: (-proto_n).to_array(),
                distal: Vec3::from(data.distal_axis()).to_array(),
                ..axes
            };
            // Mesh-local: toward THIS panel's surface (not same-azimuth radial — that inverted face_dot).
            let surf = h.surface_point(0.5, 0.5, 0.0);
            let toward = palm_wm::toward_panel_surface(palm.to_array(), surf);
            let live_dir = palm_wm::reach_direction(&axes, inner.reach.cfg());
            let live_origin = palm_wm::reach_origin_stable(&axes);
            inner.reach.step(now, dists, &axes, toward, pinch_gap, live_dir, live_origin);

            if palm_wm::palm_debug_enabled() {
                use std::sync::atomic::{AtomicU32, Ordering};
                static N: AtomicU32 = AtomicU32::new(0);
                if N.fetch_add(1, Ordering::Relaxed) % 30 == 0 {
                    let face = Vec3::from(axes.palm_normal).dot(Vec3::from(toward));
                    debug_line = Some(format!(
                        "palm_dbg right={right} dists={dists:?} mean={:.3} pinch={:.3} n={:?} back={:?} distal={:?} toward={:?} face_dot={:.2} armed={} grab={} shape={:?}",
                        palm_wm::mean_tip_distance(dists),
                        pinch_gap,
                        axes.palm_normal,
                        axes.palm_back,
                        axes.distal,
                        toward,
                        face,
                        inner.reach.armed,
                        inner.reach.grabbing,
                        inner.reach.shape,
                    ));
                }
            }

            if let Some((orig, dir)) = inner.reach.aim() {
                viz_origin = Some(Vec3::from(orig));
                if let Some(hit) = mesh_local_ray_hit(&h, orig, dir) {
                    palm_hit = Some(hit);
                    viz_end = Some(Vec3::from(h.surface_point(hit.uv.0, hit.uv.1, OVERLAY_INSET_M)));
                } else {
                    viz_end = Some(Vec3::from(orig) + Vec3::from(dir) * 0.8);
                }
            }
            break;
        }
        if let Some(line) = debug_line {
            tracing::info!("{line}");
        }

        let suppress = {
            let gap = input.values().find_map(|s| match s.input() {
                InputDataType::Hand { data } => Some(
                    Vec3::from(data.thumb.tip.pose.position)
                        .distance(Vec3::from(data.index.tip.pose.position)),
                ),
                _ => None,
            }).unwrap_or(1.0);
            inner.reach.suppress_pointer(gap)
        };

        {
            use std::sync::atomic::{AtomicU32, Ordering};
            static N: AtomicU32 = AtomicU32::new(0);
            let q = &input;
            if !q.is_empty() && N.fetch_add(1, Ordering::Relaxed) % 90 == 0 {
                let hits = q.values().filter(|s| snapshot_hit(&h, s.input()).is_some()).count();
                tracing::info!(
                    inputs = q.len(),
                    hits,
                    armed = inner.reach.armed,
                    grabbing = inner.reach.grabbing,
                    shape = ?inner.reach.shape,
                    suppress,
                    r = h.radius_m,
                    "xr input frame"
                );
            }
        }

        let hovering = |s: &InputSnapshot| snapshot_hit(&h, s.input()).is_some();
        let pressed = |key: &'static str, finger: fn(&stardust_xr_fusion::suis::Hand) -> &stardust_xr_fusion::suis::Finger| {
            move |s: &InputSnapshot| match s.input() {
                InputDataType::Hand { data } => {
                    Vec3::from(data.thumb.tip.pose.position).distance(Vec3::from(finger(data).tip.pose.position)) < 0.02
                }
                _ => s.datamap_f32(key) > 0.5,
            }
        };

        let best = input
            .values()
            .filter(|s| {
                if suppress {
                    !matches!(s.input(), InputDataType::Hand { .. })
                } else {
                    true
                }
            })
            .filter_map(|s| snapshot_hit(&h, s.input()).map(|hit| (s, hit)))
            .min_by(|a, b| a.0.distance().total_cmp(&b.0.distance()));
        if let Some((snap, hit)) = best {
            (self.on_motion.0)(state, hit.uv);
            let scroll = snap.datamap_vec2("scroll");
            if scroll.x != 0.0 || scroll.y != 0.0 {
                (self.on_scroll.0)(state, [scroll.x, scroll.y]);
            }
        }

        for (action, btn, interact) in [
            (&mut inner.left, BTN_LEFT, Box::new(pressed("select", |h| &h.index)) as Box<dyn Fn(&InputSnapshot) -> bool>),
            (&mut inner.middle, BTN_MIDDLE, Box::new(pressed("middle", |h| &h.middle))),
            (&mut inner.right, BTN_RIGHT, Box::new(pressed("context", |h| &h.ring))),
        ] {
            if suppress {
                action.update(
                    false,
                    &inner.input,
                    |s| !matches!(s.input(), InputDataType::Hand { .. }) && hovering(s),
                    interact,
                );
            } else {
                action.update(false, &inner.input, hovering, interact);
            }
            if action.actor_started() {
                (self.on_button.0)(state, btn, true);
            }
            if action.actor_stopped() {
                (self.on_button.0)(state, btn, false);
            }
        }

        // Highlight / slide / settle from arming state machine.
        let want_hl = inner.reach.armed && !inner.reach.grabbing && palm_hit.is_some();
        if want_hl != inner.palm_highlight {
            inner.palm_highlight = want_hl;
            (self.on_palm_highlight.0)(state, want_hl);
            if want_hl {
                tracing::info!(?palm_hit, "palm reach highlight panel");
            }
        }
        if inner.reach.grabbing && !was_grabbing {
            if let Some(hit) = palm_hit {
                inner.palm_grab_u0 = Some(hit.uv.0);
                inner.palm_grab_yaw0 = None;
                (self.on_palm_highlight.0)(state, true);
                inner.palm_highlight = true;
                (self.on_palm_slide.0)(state, 0.0);
                tracing::info!(u = hit.uv.0, "palm fist grab panel");
            }
        } else if inner.reach.grabbing {
            if let Some(hit) = palm_hit {
                let u0 = inner.palm_grab_u0.unwrap_or(0.5);
                let dyaw = palm_wm::slide_yaw_from_uv_delta(0.0, u0, hit.uv.0, h.width_m, h.radius_m);
                (self.on_palm_slide.0)(state, dyaw);
            }
        } else if was_grabbing {
            inner.palm_grab_u0 = None;
            inner.palm_grab_yaw0 = None;
            (self.on_palm_settle.0)(state);
            let hl = want_hl;
            inner.palm_highlight = hl;
            (self.on_palm_highlight.0)(state, hl);
            tracing::info!("palm open settle panel");
        } else if was_armed && !inner.reach.armed && inner.palm_highlight {
            inner.palm_highlight = false;
            (self.on_palm_highlight.0)(state, false);
        }

        let interacting = inner.left.actor_acting()
            || inner.middle.actor_acting()
            || inner.right.actor_acting()
            || inner.reach.grabbing;
        let mut viz: Vec<Line> = Vec::new();
        if suppress {
            if let (Some(o), Some(e)) = (viz_origin, viz_end) {
                let col = if inner.reach.grabbing {
                    rgba_linear!(0.2, 0.95, 0.55, 0.95)
                } else if palm_hit.is_some() {
                    rgba_linear!(0.35, 0.85, 1.0, 0.9)
                } else {
                    rgba_linear!(0.7, 0.75, 0.85, 0.55)
                };
                let end_col = if inner.reach.grabbing {
                    rgba_linear!(0.2, 0.95, 0.55, 0.0)
                } else if palm_hit.is_some() {
                    rgba_linear!(0.35, 0.85, 1.0, 0.0)
                } else {
                    rgba_linear!(0.7, 0.75, 0.85, 0.0)
                };
                viz.push(Line {
                    points: vec![
                        LinePoint { point: o.into(), thickness: 0.003, color: col },
                        LinePoint { point: e.into(), thickness: 0.0015, color: end_col },
                    ],
                    cyclic: false,
                });
            }
            if let Some(hit) = palm_hit {
                viz.push(hover_cursor_ring(&h, hit, inner.reach.grabbing));
            }
            // Drag preview: ghost panel at the hand + slot outline where it will land.
            if inner.reach.grabbing {
                if let Some(o) = viz_origin {
                    let dir = inner
                        .reach
                        .aim()
                        .map(|(_, d)| Vec3::from(d))
                        .unwrap_or(Vec3::NEG_Z);
                    viz.push(ghost_panel_at_hand(o, dir, h.width_m * 0.35, h.height_m * 0.35));
                }
                if let Some(hit) = palm_hit {
                    viz.push(slot_outline_at_hit(&h, hit));
                }
            }
        } else if let Some((snap, hit)) = best {
            if let Some(line) = hover_line_from_input(&h, snap, hit, interacting) {
                viz.push(line);
            }
            viz.push(hover_cursor_ring(&h, hit, interacting));
        }
        let sig = lines_signature(&viz);
        if sig != inner.last_lines_sig {
            inner.last_lines_sig = sig;
            let _ = inner.lines.set_lines(viz);
        }
    }
}



/// Quantize line endpoints so tiny hand jitter does not spam `set_lines` → server.
fn lines_signature(lines: &[Line]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    lines.len().hash(&mut h);
    for line in lines {
        line.cyclic.hash(&mut h);
        line.points.len().hash(&mut h);
        for p in &line.points {
            quant_mm(p.point.x).hash(&mut h);
            quant_mm(p.point.y).hash(&mut h);
            quant_mm(p.point.z).hash(&mut h);
            quant_mm(p.thickness * 1000.0).hash(&mut h);
        }
    }
    h.finish()
}

fn quant_mm(v: f32) -> i32 {
    (v * 200.0).round() as i32 // 5 mm bins
}

/// Ghost rectangle floating at the hand while fist-dragging a panel.
fn ghost_panel_at_hand(palm: Vec3, dir: Vec3, width_m: f32, height_m: f32) -> Line {
    let forward = dir.normalize_or_zero();
    let centre = palm + forward * 0.06;
    let up = if forward.cross(Vec3::Y).length_squared() > 1e-6 {
        forward.cross(Vec3::Y).cross(forward).normalize_or_zero()
    } else {
        Vec3::X
    };
    let right = up.cross(forward).normalize_or_zero();
    let hw = width_m * 0.5;
    let hh = height_m * 0.5;
    let corners = [
        centre + right * (-hw) + up * hh,
        centre + right * hw + up * hh,
        centre + right * hw + up * (-hh),
        centre + right * (-hw) + up * (-hh),
    ];
    let mut line = lines::line_from_points(corners.into_iter().map(|c| c.to_array()).collect());
    line.cyclic = true;
    line.thickness(0.0025)
        .color(rgba_linear!(0.25, 0.95, 0.55, 0.85))
}

/// Landing-slot outline on the cylinder at the current ray hit.
fn slot_outline_at_hit(h: &CylinderSliceHitTest, hit: SliceHit) -> Line {
    let _ = hit; // this panel's mesh is already the landing slot
    crate::outline::curved_panel_border_segs(h.width_m, h.height_m, h.radius_m, 16)
        .thickness(0.004)
        .color(rgba_linear!(1.0, 0.85, 0.25, 0.9))
}

/// Flatland `pointer_input::line_from_input` / `line_from_point`, but the surface
/// endpoint is the **curved** hit (not a flat z=0 clamp).
///
/// Flatland visuals (HoverPlaneSettings::default):
/// - Pointer (ray) devices: no line
/// - Hand: surface ← stable_pinch projected to plane; end ← predicted_pinch
/// - Tip: both ends from tip pose (surface clamped to plane)
/// - start thickness 0.0, end thickness 0.005; end color alpha 0
fn hover_line_from_input(h: &CylinderSliceHitTest, input: &InputSnapshot, hit: SliceHit, interacting: bool) -> Option<Line> {
    let end = match input.input() {
        InputDataType::Pointer { .. } => return None,
        InputDataType::Hand { data } => Vec3::from(data.predicted_pinch_position()),
        InputDataType::Tip { data } => Vec3::from(data.pose.position),
    };
    Some(hover_line_at_hit(h, hit, end, interacting))
}

fn hover_line_at_hit(h: &CylinderSliceHitTest, hit: SliceHit, end: Vec3, interacting: bool) -> Line {
    hover_line(Vec3::from(h.surface_point(hit.uv.0, hit.uv.1, OVERLAY_INSET_M)), end, interacting)
}

fn hover_line(surface: Vec3, fingertip: Vec3, interacting: bool) -> Line {
    let settings = HoverPlaneSettings::default();
    Line {
        points: vec![
            LinePoint {
                point: surface.into(),
                thickness: settings.line_start_thickness,
                color: if interacting {
                    settings.line_start_color_interact
                } else {
                    settings.line_start_color_hover
                },
            },
            LinePoint {
                point: fingertip.into(),
                thickness: settings.line_end_thickness,
                color: if interacting {
                    settings.line_end_color_interact
                } else {
                    settings.line_end_color_hover
                },
            },
        ],
        cyclic: false,
    }
}

/// Small ring on the curved surface at the hit UV (Flatland touch_input::ring style).
fn hover_cursor_ring(h: &CylinderSliceHitTest, hit: SliceHit, interacting: bool) -> Line {
    let color = if interacting {
        rgba_linear!(0.0, 1.0, 0.75, 0.9)
    } else {
        rgba_linear!(1.0, 1.0, 1.0, 0.85)
    };
    let points = (0..20).map(|i| {
        let theta = i as f32 / 20.0 * std::f32::consts::TAU;
        let (s, c) = theta.sin_cos();
        h.surface_point(hit.uv.0 + c * 0.008 / h.width_m,
            hit.uv.1 + s * 0.008 / h.height_m, OVERLAY_INSET_M)
    });
    let mut ring = lines::line_from_points(points.collect());
    ring.cyclic = true;
    ring.thickness(0.0015).color(color)
}

#[cfg(test)]
mod visual_tests {
    use super::*;

    #[test]
    fn hover_line_has_two_points() {
        let line = hover_line(Vec3::ZERO, Vec3::new(0.0, 0.0, 0.1), false);
        assert_eq!(line.points.len(), 2);
        assert!(!line.cyclic);
        let s = HoverPlaneSettings::default();
        assert!((line.points[0].thickness - s.line_start_thickness).abs() < 1e-6);
        assert!((line.points[1].thickness - s.line_end_thickness).abs() < 1e-6);
        // Surface endpoint is first (same order as Flatland line_from_point).
        assert_eq!(line.points[0].point, Vec3::ZERO.into());
        assert_eq!(line.points[1].point, Vec3::new(0.0, 0.0, 0.1).into());
    }

    #[test]
    fn hover_cursor_ring_centred_on_uv() {
        let h = CylinderSliceHitTest {
            radius_m: 0.7,
            yaw_rad: 0.0,
            width_m: 0.6,
            height_m: 0.4,
        };
        let hit = SliceHit { uv: (0.5, 0.5), distance: 0.0 };
        let ring = hover_cursor_ring(&h, hit, false);
        assert!(ring.points.len() >= 8);
        assert!(ring.cyclic);
    }

    #[test]
    fn hover_ring_vertices_follow_inset_radius_at_edge_uvs() {
        let h = crate::mesh::rendered_slice(0.42666668, 0.24, 0.45);
        for u in [0.0, 0.02, 0.5, 0.98, 1.0] {
            let hit = SliceHit {uv: (u, 0.5), distance: 0.0};
            let ring = hover_cursor_ring(&h, hit, false);
            for p in ring.points {
                let rho = (p.point.x * p.point.x + (p.point.z - h.radius_m).powi(2)).sqrt();
                assert!((rho - (h.radius_m - OVERLAY_INSET_M)).abs() < 1e-5);
            }
        }
    }


    #[test]
    fn hover_line_endpoint_uses_inset_arc_not_chord_at_edges() {
        let h = crate::mesh::rendered_slice(0.42666668, 0.24, 0.45);
        for u in [0.01, 0.99] {
            let line = hover_line_at_hit(&h, SliceHit {uv: (u, 0.5), distance: 0.0}, Vec3::new(0.0, 0.0, 0.2), false);
            let p = line.points[0].point;
            assert!(p.z > 0.04, "edge endpoint must not land on z=0 chord");
            assert!((p.x.hypot(p.z - h.radius_m) - (h.radius_m - OVERLAY_INSET_M)).abs() < 1e-5);
        }
    }

}

