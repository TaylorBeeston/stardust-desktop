//! Asteroids `PalmGrabbable`: open-palm aim + fist grab/release for any Entity.
//!
//! Pair with the normal pinch `Grabbable`. A wide capture sphere receives hands
//! at reach distance; `near_yield_m` leaves contact-range grabs to pinch.

use crate::{
    self as palm_wm, GateBreak, HandAxes, PalmShape, ReachArming, ReachConfig, REACH_MAX_M,
};
use derive_where::derive_where;
use glam::{Affine3A, Quat, Vec3};
use stardust_xr_asteroids::{
    CloneFnWrapper, Component, ComponentCreateInfo, Context, Inners, ValidState,
};
use stardust_xr_fusion::{
    client::FrameInfo,
    fields::{Field, FieldExt, Shape},
    spatial::Transform,
    suis::InputDataType,
    types::Posef,
    Error, Result,
};
use stardust_xr_molecules::input_action::{InputQueue, InputSnapshot};
use std::sync::Arc;
use std::time::Instant;

type OnChangePose<State> = CloneFnWrapper<dyn Fn(&mut State, Posef) + Send + Sync>;
type GrabStart<State> = CloneFnWrapper<dyn Fn(&mut State) + Send + Sync>;
type GrabStop<State> = CloneFnWrapper<dyn Fn(&mut State) + Send + Sync>;

#[derive_where(Debug, Clone)]
pub struct PalmGrabbable<State: ValidState> {
    on_change_pose: OnChangePose<State>,
    grab_start: GrabStart<State>,
    grab_stop: GrabStop<State>,
    capture_radius: f32,
    hit_radius: f32,
    near_yield_m: f32,
}

impl<State: ValidState> PalmGrabbable<State> {
    pub fn new<F: Fn(&mut State, Posef) + Send + Sync + 'static>(on_change: F) -> Self {
        Self {
            on_change_pose: CloneFnWrapper(Arc::new(on_change)),
            grab_start: CloneFnWrapper(Arc::new(|_| ())),
            grab_stop: CloneFnWrapper(Arc::new(|_| ())),
            capture_radius: REACH_MAX_M,
            hit_radius: 0.12,
            near_yield_m: 0.06,
        }
    }

    pub fn grab_start<F: Fn(&mut State) + Send + Sync + 'static>(mut self, f: F) -> Self {
        self.grab_start = CloneFnWrapper(Arc::new(f));
        self
    }
    pub fn grab_stop<F: Fn(&mut State) + Send + Sync + 'static>(mut self, f: F) -> Self {
        self.grab_stop = CloneFnWrapper(Arc::new(f));
        self
    }
    pub fn hit_radius(mut self, m: f32) -> Self {
        self.hit_radius = m;
        self
    }
    pub fn capture_radius(mut self, m: f32) -> Self {
        self.capture_radius = m;
        self
    }
    pub fn near_yield_m(mut self, m: f32) -> Self {
        self.near_yield_m = m;
        self
    }
}

pub struct PalmGrabbableInner {
    _field: Field,
    input: InputQueue,
    reach: ReachArming,
    hit_radius: f32,
    near_yield_m: f32,
    pose: Posef,
    relative: Affine3A,
    grabbing: bool,
    capture_snap: Option<Arc<InputSnapshot>>,
}

impl PalmGrabbableInner {
    pub fn grabbing(&self) -> bool {
        self.grabbing
    }
}

impl<State: ValidState> Component<State> for PalmGrabbable<State> {
    type Inner = PalmGrabbableInner;
    type Error = Error;

    async fn create_inner(
        &self,
        context: &Context,
        info: ComponentCreateInfo<'_>,
    ) -> Result<Self::Inner> {
        let (field, _) = Field::new(
            &context.stardust_client,
            info.spatial,
            Shape::Sphere {
                radius: self.capture_radius.max(0.2),
            },
        )
        .await?;
        let input = InputQueue::new(
            &context.stardust_client,
            info.spatial.clone(),
            field.clone(),
            info.anchor_space.clone(),
        )
        .await?;
        Ok(PalmGrabbableInner {
            _field: field,
            input,
            reach: ReachArming::new(ReachConfig::from_env()),
            hit_radius: self.hit_radius,
            near_yield_m: self.near_yield_m,
            pose: pose_of(info.transform),
            relative: Affine3A::IDENTITY,
            grabbing: false,
            capture_snap: None,
        })
    }

    fn diff(
        &self,
        _old: &Self,
        _context: &Context,
        info: ComponentCreateInfo<'_>,
        inners: &mut Inners<'_, State, Self>,
    ) {
        let inner = inners.self_inner();
        inner.hit_radius = self.hit_radius;
        inner.near_yield_m = self.near_yield_m;
        if !inner.grabbing {
            inner.pose = pose_of(info.transform);
        }
    }

    fn frame(
        &self,
        _context: &Context,
        _info: &FrameInfo,
        state: &mut State,
        inners: &mut Inners<'_, State, Self>,
    ) {
        let inner = inners.self_inner();
        if !inner.input.handle_events() {
            return;
        }
        let now = Instant::now();
        let input_map = inner.input.input();

        struct Cand {
            right: bool,
            dists: [f32; 4],
            pinch_gap: f32,
            axes: HandAxes,
            live_dir: [f32; 3],
            live_origin: [f32; 3],
            gate: GateBreak,
            toward: [f32; 3],
            palm_dist: f32,
            ray_hit: bool,
            snap: Arc<InputSnapshot>,
        }

        let mut hands: Vec<Cand> = Vec::new();
        for snap in input_map.values() {
            let InputDataType::Hand { data } = snap.input() else {
                continue;
            };
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
            let right = matches!(data.chirality, stardust_xr_fusion::suis::Chirality::Right);
            let o = data.palm.pose.orientation;
            let axes = palm_wm::hand_axes_from_quat(
                palm.to_array(),
                [o.v.x, o.v.y, o.v.z, o.s],
                right,
            );
            let proto_n = Vec3::from(data.palm_normal());
            let axes = HandAxes {
                palm_normal: proto_n.to_array(),
                palm_back: (-proto_n).to_array(),
                distal: Vec3::from(data.distal_axis()).to_array(),
                ..axes
            };
            let toward_v = (-palm).normalize_or_zero();
            let toward = if toward_v.length_squared() < 1e-8 {
                [0.0, 0.0, -1.0]
            } else {
                toward_v.to_array()
            };
            let pinching = pinch_gap < palm_wm::PINCH_M
                && palm_wm::classify_curl(
                    palm_wm::mean_tip_distance(dists),
                    PalmShape::Open,
                    inner.reach.cfg(),
                ) != PalmShape::Fist;
            let gate = palm_wm::arm_gate_break(dists, &axes, toward, pinching, inner.reach.cfg());
            let live_dir = palm_wm::reach_direction(&axes, inner.reach.cfg());
            let live_origin = palm_wm::reach_origin_stable(&axes);
            let ray_hit = palm_wm::ray_hits_bead(
                live_origin,
                live_dir,
                [0.0, 0.0, 0.0],
                inner.hit_radius,
            );
            hands.push(Cand {
                right,
                dists,
                pinch_gap,
                axes,
                live_dir,
                live_origin,
                gate,
                toward,
                palm_dist: palm.length(),
                ray_hit,
                snap: snap.clone(),
            });
        }

        let sticky = inner.reach.active_right;
        let pick = hands
            .iter()
            .position(|h| sticky == Some(h.right))
            .or_else(|| {
                hands
                    .iter()
                    .enumerate()
                    .filter(|(_, h)| h.gate.ok && h.ray_hit)
                    .max_by(|a, b| a.1.gate.face_dot.total_cmp(&b.1.gate.face_dot))
                    .map(|(i, _)| i)
            })
            .or_else(|| {
                hands
                    .iter()
                    .enumerate()
                    .filter(|(_, h)| h.gate.ok)
                    .max_by(|a, b| a.1.gate.face_dot.total_cmp(&b.1.gate.face_dot))
                    .map(|(i, _)| i)
            })
            .or_else(|| {
                hands
                    .iter()
                    .enumerate()
                    .max_by(|a, b| a.1.gate.face_dot.total_cmp(&b.1.gate.face_dot))
                    .map(|(i, _)| i)
            });

        let Some(i) = pick else {
            return;
        };
        let hand = &hands[i];

        if !inner.grabbing && hand.palm_dist < inner.near_yield_m {
            return;
        }

        if inner.reach.armed || inner.reach.grabbing || (hand.gate.ok && hand.ray_hit) {
            inner.reach.active_right = Some(hand.right);
        }
        let was_reach_grabbing = inner.reach.grabbing;
        inner.reach.step(
            now,
            hand.dists,
            &hand.axes,
            hand.toward,
            hand.pinch_gap,
            hand.live_dir,
            hand.live_origin,
        );

        let want_grab =
            inner.reach.grabbing && (hand.ray_hit || inner.grabbing || was_reach_grabbing);
        if want_grab {
            let palm = Vec3::from(hand.live_origin);
            let palm_rot = match hand.snap.input() {
                InputDataType::Hand { data } => Quat::from(data.palm.pose.orientation),
                _ => return,
            };
            let grab_pose = Affine3A::from_rotation_translation(palm_rot, palm);

            if !inner.grabbing {
                let current = Affine3A::from_rotation_translation(
                    Quat::from(inner.pose.orientation),
                    Vec3::from(inner.pose.position),
                );
                inner.relative = grab_pose.inverse() * current;
                inner.grabbing = true;
                (self.grab_start.0)(state);
                inner.input.start_capture(&hand.snap);
                inner.capture_snap = Some(hand.snap.clone());
            }

            let new_affine = grab_pose * inner.relative;
            let (_s, r, t) = new_affine.to_scale_rotation_translation();
            let new_pose = Posef {
                position: t.into(),
                orientation: r.into(),
            };
            inner.pose = new_pose;
            (self.on_change_pose.0)(state, new_pose);
            inner.input.start_capture(&hand.snap);
        } else if inner.grabbing {
            inner.grabbing = false;
            if let Some(s) = inner.capture_snap.take() {
                inner.input.release_capture(&s);
            }
            (self.grab_stop.0)(state);
        }
    }
}

fn pose_of(transform: &Transform) -> Posef {
    Posef {
        position: transform.translation,
        orientation: transform.rotation,
    }
}
