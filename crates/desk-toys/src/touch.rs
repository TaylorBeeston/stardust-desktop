//! `Touch`: an asteroids component that hands the toy every input near it, each frame, as
//! physics-friendly contact points (hand joints / controller tips with velocity) and pointer
//! rays, all in the entity's own local space.

use glam::{Quat, Vec3};
use stardust_xr_asteroids::{
    CloneFnWrapper, Component, ComponentCreateInfo, Context, Inners, ValidState,
    components::Grabbable,
};
use stardust_xr_fusion::{Error, Result, client::FrameInfo, suis::InputDataType};
use stardust_xr_molecules::input_action::InputQueue;
use std::{collections::HashMap, fmt::Debug, sync::Arc};

/// A solid bit of a hand/controller: something that can bump into the toy.
#[derive(Debug, Clone, Copy)]
pub struct Contact {
    pub pos: Vec3,
    pub vel: Vec3,
    pub radius: f32,
    /// thumb+index pinched with the pinch point here (for picking things up)
    pub pinch: Option<Vec3>,
    /// an index fingertip (or controller tip): the thing you poke buttons with
    pub poker: bool,
    /// belongs to the left hand/controller (None = unknown)
    pub left: Option<bool>,
}

/// A pointer ray (mouse on flatscreen, controller ray in XR).
#[derive(Debug, Clone, Copy)]
pub struct Ray {
    pub origin: Vec3,
    pub dir: Vec3,
    /// distance along the ray to its deepest point inside the toy's field
    pub deepest: f32,
    /// distance from that deepest point to the field (<= 0 means the ray hits)
    pub distance: f32,
    pub select: bool,
    pub select_started: bool,
    pub scroll: f32,
}
impl Ray {
    pub fn hit(&self) -> bool {
        self.distance <= 0.0
    }
    pub fn point(&self) -> Vec3 {
        self.origin + self.dir * self.deepest
    }
}

#[derive(Debug, Default)]
pub struct TouchFrame {
    /// `FrameInfo.delta` (XR: predicted display period) — integrate physics with this
    pub dt: f32,
    /// `Context::time()` for this frame: seconds since first frame at predicted display time
    pub display_t: f32,
    pub contacts: Vec<Contact>,
    pub rays: Vec<Ray>,
    /// a sibling Grabbable is moving the toy right now
    pub grabbed: bool,
}

type OnFrame<State> = CloneFnWrapper<dyn Fn(&mut State, &TouchFrame) + Send + Sync>;

pub struct Touch<State: ValidState> {
    on_frame: OnFrame<State>,
}
impl<State: ValidState> Touch<State> {
    pub fn new(f: impl Fn(&mut State, &TouchFrame) + Send + Sync + 'static) -> Self {
        Touch { on_frame: CloneFnWrapper(Arc::new(f)) }
    }
}
impl<State: ValidState> Debug for Touch<State> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Touch")
    }
}

pub struct TouchInner {
    input: InputQueue,
    /// (input method key, joint index) -> (last pos, smoothed velocity)
    prev: HashMap<(String, u8), (Vec3, Vec3)>,
    /// previous select per input, to find the press edge
    prev_select: HashMap<String, bool>,
}

impl<State: ValidState> Component<State> for Touch<State> {
    type Inner = TouchInner;
    type Error = Error;

    async fn create_inner(&self, context: &Context, info: ComponentCreateInfo<'_>) -> Result<TouchInner> {
        // relative to the entity itself: toys animate children, never the entity, so
        // contact positions/velocities are stable in this frame
        let input = InputQueue::new(
            &context.stardust_client,
            info.spatial.clone(),
            info.field.clone(),
            info.spatial.spatial_ref().await?,
        )
        .await?;
        Ok(TouchInner { input, prev: HashMap::new(), prev_select: HashMap::new() })
    }

    fn diff(&self, _: &Self, _: &Context, _: ComponentCreateInfo<'_>, _: &mut Inners<'_, State, Self>) {}

    fn frame(&self, context: &Context, info: &FrameInfo, state: &mut State, inners: &mut Inners<'_, State, Self>) {
        let grabbed = inners
            .get::<Grabbable<State>>()
            .is_some_and(|g| g.grabbing());
        let inner = inners.self_inner();
        inner.input.handle_events();
        let dt = info.delta.clamp(1.0 / 500.0, 0.1);
        let display_t = context.time();
        let mut frame = TouchFrame { dt, display_t, grabbed, ..Default::default() };
        let mut seen = Vec::new();

        for (method, snap) in inner.input.input() {
            let key = format!("{method:?}");
            let mut track = |idx: u8, pos: Vec3, radius: f32, pinch: Option<Vec3>, poker: bool, left: Option<bool>, out: &mut Vec<Contact>| {
                let k = (key.clone(), idx);
                let vel = match inner.prev.get(&k) {
                    Some(&(p, v)) => v.lerp((pos - p) / dt, 0.6),
                    None => Vec3::ZERO,
                };
                inner.prev.insert(k.clone(), (pos, vel));
                seen.push(k);
                out.push(Contact { pos, vel, radius, pinch, poker, left });
            };
            match snap.input() {
                InputDataType::Hand { data } => {
                    let p = |j: &stardust_xr_fusion::suis::Joint| Vec3::from(j.pose.position);
                    let pinch = (p(&data.thumb.tip).distance(p(&data.index.tip)) < 0.025)
                        .then(|| (p(&data.thumb.tip) + p(&data.index.tip)) / 2.0);
                    let joints = [
                        (&data.palm, 0.035),
                        (&data.thumb.tip, 0.0),
                        (&data.index.tip, 0.0),
                        (&data.middle.tip, 0.0),
                        (&data.ring.tip, 0.0),
                        (&data.little.tip, 0.0),
                        (&data.index.intermediate, 0.0),
                        (&data.middle.intermediate, 0.0),
                        (&data.ring.intermediate, 0.0),
                    ];
                    let left = Some(data.chirality == stardust_xr_fusion::suis::Chirality::Left);
                    for (i, (j, r)) in joints.into_iter().enumerate() {
                        let radius = if r > 0.0 { r } else { j.radius.max(0.008) };
                        track(i as u8, p(j), radius, pinch, i == 2, left, &mut frame.contacts);
                    }
                }
                InputDataType::Tip { data } => {
                    let left = data.chirality.map(|c| c == stardust_xr_fusion::suis::Chirality::Left);
                    track(0, Vec3::from(data.pose.position), 0.02, None, true, left, &mut frame.contacts);
                }
                InputDataType::Pointer { data } => {
                    let rot = Quat::from(data.pose.orientation);
                    let select = snap.datamap_f32("select") > 0.5;
                    let was = inner.prev_select.insert(key.clone(), select).unwrap_or(false);
                    let scroll = snap.datamap_vec2("scroll_continuous").y
                        + snap.datamap_vec2("scroll_discrete").y;
                    frame.rays.push(Ray {
                        origin: Vec3::from(data.pose.position),
                        dir: rot * Vec3::NEG_Z,
                        deepest: data.deepest_point,
                        distance: snap.distance(),
                        select,
                        select_started: select && !was,
                        scroll,
                    });
                }
            }
        }
        inner.prev.retain(|k, _| seen.contains(k));
        for r in &frame.rays {
            if r.select_started || r.scroll != 0.0 {
                tracing::debug!(distance = r.distance, deepest = r.deepest, select = r.select, scroll = r.scroll, "ray event");
            }
        }
        (self.on_frame.0)(state, &frame);
    }
}
