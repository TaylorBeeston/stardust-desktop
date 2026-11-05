//! `CurvedPointerPlane`: Asteroids element that routes XR pointer / hand input onto the
//! curved slice. Replaces Flatland's flat `PointerPlane` UV (`x + w/2`) with
//! [`crate::pointer_uv`] on the true arc. Capture volume is still the chord-AABB Field
//! (Fusion has no cylinder-slice SDF).

use crate::field_shape::slice_aabb_extents;
use crate::hit_test::CylinderSliceHitTest;
use crate::pointer_uv::{mesh_local_point_hit, mesh_local_ray_hit, SliceHit};
use glam::Vec3;
use stardust_xr_asteroids::{Context, CreateInnerInfo, CustomElement, FnWrapper, ValidState};
use stardust_xr_fusion::{
    fields::{Field, FieldExt, Shape},
    spatial::{Spatial, SpatialExt, Transform},
    suis::InputDataType,
};
use stardust_xr_molecules::input_action::{InputQueue, InputSnapshot, SingleAction};

pub const BTN_LEFT: u32 = 0x110;
pub const BTN_RIGHT: u32 = 0x111;
pub const BTN_MIDDLE: u32 = 0x112;

/// Hand / tip hover band in front of the surface (metres), mirroring Flatland.
const HAND_HOVER: std::ops::Range<f32> = -0.02..0.2;
/// Extra Field padding so rays grazing the arc edges still reach us.
const FIELD_PAD_M: f32 = 0.02;

/// Where an input lands on this slice, if anywhere. Pure — unit-tested.
pub fn snapshot_hit(h: &CylinderSliceHitTest, input: &InputDataType) -> Option<SliceHit> {
    match input {
        InputDataType::Pointer { data } => {
            mesh_local_ray_hit(h, Vec3::from(data.pose.position).to_array(), Vec3::from(data.direction()).to_array())
        }
        InputDataType::Hand { data } => mesh_local_point_hit(h, Vec3::from(data.index.tip.pose.position).to_array())
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
}

impl<State: ValidState> CurvedPointerPlane<State> {
    pub fn new(hit_test: CylinderSliceHitTest) -> Self {
        Self {
            hit_test,
            on_motion: FnWrapper(Box::new(|_, _| {})),
            on_button: FnWrapper(Box::new(|_, _, _| {})),
            on_scroll: FnWrapper(Box::new(|_, _| {})),
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
}

fn field_box(h: &CylinderSliceHitTest) -> (Transform, Shape) {
    let (x, y, z) = slice_aabb_extents(h.width_m, h.height_m, h.radius_m);
    // Mesh spans z ∈ [-zmax, 0]; centre the box on that span.
    (
        Transform::from_translation([0.0, 0.0, -z * 0.5]),
        Shape::Box { size: [x + FIELD_PAD_M, y + FIELD_PAD_M, z + FIELD_PAD_M].into() },
    )
}

pub struct CurvedPointerInner {
    _root: Spatial,
    field_spatial: Spatial,
    field: Field,
    input: InputQueue,
    left: SingleAction,
    middle: SingleAction,
    right: SingleAction,
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
        let _ = info.child_space.set_parent(root.spatial_ref().await?);
        Ok(CurvedPointerInner {
            _root: root,
            field_spatial,
            field,
            input,
            left: SingleAction::default(),
            middle: SingleAction::default(),
            right: SingleAction::default(),
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
                tracing::info!(inputs = q.len(), hits, "xr input frame");
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
        // Closest hovering (or captured) input drives motion + scroll.
        let input = inner.input.input();
        let best = input
            .values()
            .filter_map(|s| snapshot_hit(&h, s.input()).map(|hit| (s, hit)))
            .min_by(|a, b| a.0.distance().total_cmp(&b.0.distance()));
        if let Some((snap, hit)) = best {
            (self.on_motion.0)(state, hit.uv);
            let scroll = snap.datamap_vec2("scroll");
            if scroll.x != 0.0 || scroll.y != 0.0 {
                (self.on_scroll.0)(state, [scroll.x, scroll.y]);
            }
        }
    }
}
