//! `CurvedPointerPlane`: Asteroids element that routes XR pointer / hand input onto the
//! curved slice. Replaces Flatland's flat `PointerPlane` UV (`x + w/2`) with
//! [`crate::pointer_uv`] on the true arc. Capture volume follows the arc using a union of thin tangent fields.

use crate::field_shape::field_shape_for_slice;
use crate::hit_test::CylinderSliceHitTest;
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
    /// Pinch-drag horizontally past a threshold → signed delta in metres of arc (+ = +u).
    on_pinch_drag: FnWrapper<dyn Fn(&mut State, f32) + Send + Sync>,
}

impl<State: ValidState> CurvedPointerPlane<State> {
    pub fn new(hit_test: CylinderSliceHitTest) -> Self {
        Self {
            hit_test,
            on_motion: FnWrapper(Box::new(|_, _| {})),
            on_button: FnWrapper(Box::new(|_, _, _| {})),
            on_scroll: FnWrapper(Box::new(|_, _| {})),
            on_pinch_drag: FnWrapper(Box::new(|_, _| {})),
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
    pub fn on_pinch_drag(mut self, f: impl Fn(&mut State, f32) + Send + Sync + 'static) -> Self {
        self.on_pinch_drag = FnWrapper(Box::new(f));
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
    /// UV.u at pinch-drag start; None when not dragging for swap.
    pinch_drag_u0: Option<f32>,
    /// True once a swap drag fired this pinch (suppress further until release).
    pinch_drag_fired: bool,
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
            pinch_drag_u0: None,
            pinch_drag_fired: false,
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
        {
            // Throttled diagnostics: do XR inputs reach the queue, and do they hit the arc?
            use std::sync::atomic::{AtomicU32, Ordering};
            static N: AtomicU32 = AtomicU32::new(0);
            let q = inner.input.input();
            if !q.is_empty() && N.fetch_add(1, Ordering::Relaxed) % 90 == 0 {
                let hits = q.values().filter(|s| snapshot_hit(&h, s.input()).is_some()).count();
                let desc: Vec<String> = q.values().map(|s| match s.input() {
                    InputDataType::Pointer { data } => format!("ptr o={:?} d={:?} dist={:.3}", Vec3::from(data.pose.position), Vec3::from(data.direction()), s.distance()),
                    InputDataType::Hand { data } => format!("hand tip={:?} pinch_gap={:.3} captured={} dist={:.3}", Vec3::from(data.index.tip.pose.position), Vec3::from(data.thumb.tip.pose.position).distance(Vec3::from(data.index.tip.pose.position)), s.captured(), s.distance()),
                    InputDataType::Tip { data } => format!("tip p={:?} dist={:.3}", Vec3::from(data.pose.position), s.distance()),
                }).collect();
                tracing::info!(inputs = q.len(), hits, ?desc, r = h.radius_m, "xr input frame");
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
        // Closest hovering (or captured) input drives motion + scroll + visuals.
        let input = inner.input.input();
        let best = input
            .values()
            .filter_map(|s| snapshot_hit(&h, s.input()).map(|hit| (s, hit)))
            .min_by(|a, b| a.0.distance().total_cmp(&b.0.distance()));
        // Enter/position the wl_pointer at the curved hit BEFORE emitting a pinch button.
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
            action.update(false, &inner.input, hovering, interact);
            if action.actor_started() {
                (self.on_button.0)(state, btn, true);
            }
            if action.actor_stopped() {
                (self.on_button.0)(state, btn, false);
            }
        }
        // Pinch-drag → panel shift: move > ~12 cm of arc while left-pinching.
        const DRAG_M: f32 = 0.12;
        if inner.left.actor_started() {
            if let Some((_, hit)) = best {
                inner.pinch_drag_u0 = Some(hit.uv.0);
                inner.pinch_drag_fired = false;
            }
        }
        if inner.left.actor_stopped() {
            inner.pinch_drag_u0 = None;
            inner.pinch_drag_fired = false;
        }
        if inner.left.actor_acting() && !inner.pinch_drag_fired {
            if let (Some(u0), Some((_, hit))) = (inner.pinch_drag_u0, best) {
                let du = hit.uv.0 - u0;
                let arc = du * h.width_m;
                if arc.abs() >= DRAG_M {
                    (self.on_pinch_drag.0)(state, arc);
                    inner.pinch_drag_fired = true;
                    inner.pinch_drag_u0 = Some(hit.uv.0);
                }
            }
        }

        let interacting = inner.left.actor_acting() || inner.middle.actor_acting() || inner.right.actor_acting();
        let mut viz: Vec<Line> = Vec::new();
        if let Some((snap, hit)) = best {
            if let Some(line) = hover_line_from_input(&h, snap, hit, interacting) {
                viz.push(line);
            }
            viz.push(hover_cursor_ring(&h, hit, interacting));
        }
        let _ = inner.lines.set_lines(viz);
    }
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

