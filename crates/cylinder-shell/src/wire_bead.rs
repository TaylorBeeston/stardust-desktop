//! XR drawer + grabber for cylinder rings / gap bead (one owner process).
//!
//! Parent of the Lines Spatial is `client.root()` (cylinder frame), not the panel
//! Entity — same frame as [`crate::panel::SlotPlacement`].

use crate::palm_wm::{self, ReachArming, ReachConfig};
use crate::wire::{
    bead_polylines, grab_frame_deltas, ring_polylines, BeadVisualState, WireLayout, BEAD_GRAB_M,
    BEAD_THICKNESS, RING_THICKNESS,
};
use glam::Vec3;
use stardust_xr_asteroids::{Context, CreateInnerInfo, CustomElement, FnWrapper, ValidState};
use stardust_xr_fusion::{
    drawable::{Line, LinePoint, Lines, LinesExt as _},
    fields::{Field, FieldExt as _, Shape},
    spatial::{Spatial, SpatialExt as _, Transform},
    suis::InputDataType,
    types::{rgba_linear, Color},
};
use stardust_xr_molecules::input_action::{InputQueue, InputSnapshot, SingleAction};

type OnGrabDelta<S> = FnWrapper<dyn Fn(&mut S, f32, f32) + Send + Sync>;
type OnGrabEnd<S> = FnWrapper<dyn Fn(&mut S) + Send + Sync>;

#[derive_where::derive_where(Debug)]
pub struct WireBead<State: ValidState> {
    pub layout: WireLayout,
    on_grab_delta: OnGrabDelta<State>,
    on_grab_end: OnGrabEnd<State>,
}

impl<State: ValidState> WireBead<State> {
    pub fn new(layout: WireLayout) -> Self {
        Self {
            layout,
            on_grab_delta: FnWrapper(Box::new(|_, _, _| {})),
            on_grab_end: FnWrapper(Box::new(|_| {})),
        }
    }
    pub fn on_grab_delta(mut self, f: impl Fn(&mut State, f32, f32) + Send + Sync + 'static) -> Self {
        self.on_grab_delta = FnWrapper(Box::new(f));
        self
    }
    pub fn on_grab_end(mut self, f: impl Fn(&mut State) + Send + Sync + 'static) -> Self {
        self.on_grab_end = FnWrapper(Box::new(f));
        self
    }
}

impl<State: ValidState> PartialEq for WireBead<State> {
    fn eq(&self, other: &Self) -> bool {
        self.layout == other.layout
    }
}

pub struct WireBeadInner {
    _root: Spatial,
    lines: Lines,
    /// Big sphere so hands near the cylinder reach the queue; bead test filters.
    _field_spatial: Spatial,
    _field: Field,
    input: InputQueue,
    grab: SingleAction,
    /// Last hand point while grabbing (cylinder frame), for per-frame deltas.
    last_grab_pt: Option<Vec3>,
    drawn_key: Option<(u64, BeadVisualState)>,
    reach: ReachArming,
    palm_aim_bead: bool,
    /// Last drawn reach ray endpoints (for visual keying).
    reach_viz: Option<(Vec3, Vec3)>,
}

fn layout_key(l: &WireLayout) -> u64 {
    // quantize so tiny float noise doesn't redraw every frame
    let q = |v: f32| (v * 1000.0).round() as i32 as u64;
    q(l.radius_m)
        ^ q(l.yaw_rad).wrapping_shl(8)
        ^ q(l.height_m).wrapping_shl(16)
        ^ q(l.panel_height_m).wrapping_shl(24)
        ^ q(l.gap_m).wrapping_shl(32)
}

fn lp(p: Vec3, thickness: f32, color: Color) -> LinePoint {
    LinePoint {
        point: p.into(),
        thickness,
        color,
    }
}

fn line_from(pts: &[Vec3], thickness: f32, color: Color) -> Line {
    Line {
        points: pts.iter().copied().map(|p| lp(p, thickness, color)).collect(),
        cyclic: true,
    }
}

pub fn build_wire_lines(layout: &WireLayout, state: BeadVisualState) -> Vec<Line> {
    let ring_col: Color = if state == 2 {
        rgba_linear!(0.5, 0.7, 1.0, 0.6)
    } else {
        rgba_linear!(0.35, 0.45, 0.6, 0.18)
    };
    let (upper, lower) = ring_polylines(layout);
    let mut ls = vec![
        line_from(&upper, RING_THICKNESS, ring_col),
        line_from(&lower, RING_THICKNESS, ring_col),
    ];
    let bead = layout.bead_pos();
    let bc: Color = match state {
        2 => rgba_linear!(0.6, 0.85, 1.0, 0.95),
        1 => rgba_linear!(0.85, 0.9, 1.0, 0.8),
        _ => rgba_linear!(0.6, 0.65, 0.75, 0.45),
    };
    for circ in bead_polylines(bead) {
        ls.push(line_from(&circ, BEAD_THICKNESS, bc));
    }
    ls
}

fn hand_point(s: &InputSnapshot) -> Option<Vec3> {
    match s.input() {
        InputDataType::Hand { data } => Some(
            Vec3::from(data.thumb.tip.pose.position)
                .lerp(Vec3::from(data.index.tip.pose.position), 0.5),
        ),
        InputDataType::Tip { data } => Some(Vec3::from(data.pose.position)),
        InputDataType::Pointer { .. } => None,
    }
}

fn pinch_or_grab(s: &InputSnapshot) -> bool {
    match s.input() {
        InputDataType::Hand { .. } => s.datamap_f32("pinch_strength") > 0.95,
        _ => s.datamap_f32("grab") > 0.95,
    }
}

impl<State: ValidState> CustomElement<State> for WireBead<State> {
    type Inner = WireBeadInner;
    type Error = stardust_xr_fusion::Error;

    async fn create_inner(
        &self,
        context: &Context,
        info: CreateInnerInfo,
    ) -> Result<Self::Inner, Self::Error> {
        let client = &context.stardust_client;
        // Cylinder frame == client root (panel Entity is a child posed on the cylinder).
        let (root, root_ref) = Spatial::new(client, client.root(), Transform::IDENTITY).await?;
        let (field_spatial, _) = Spatial::new(client, &root_ref, Transform::IDENTITY).await?;
        let (field, _) = Field::new(
            client,
            &field_spatial,
            Shape::Sphere { radius: 6.0 },
        )
        .await?;
        let input = InputQueue::new(client, root.clone(), field.clone(), root_ref).await?;
        let lines = Lines::new(client, &root, build_wire_lines(&self.layout, 0)).await?;
        // Detach from panel child_space so asteroids doesn't fight our root parenting.
        let _ = info.child_space.set_parent(root.spatial_ref().await?);
        tracing::info!(
            r = self.layout.radius_m,
            yaw = self.layout.yaw_rad,
            h = self.layout.height_m,
            "cylinder wire + bead owner online"
        );
        Ok(WireBeadInner {
            _root: root,
            lines,
            _field_spatial: field_spatial,
            _field: field,
            input,
            grab: SingleAction::default(),
            last_grab_pt: None,
            drawn_key: Some((layout_key(&self.layout), 0)),
            reach: ReachArming::new(ReachConfig::from_env()),
            palm_aim_bead: false,
            reach_viz: None,
        })
    }

    fn diff(&self, old: &Self, _context: &Context, inner: &mut Self::Inner) {
        if self.layout != old.layout {
            // Force redraw on next frame with current visual state.
            let state = inner.drawn_key.map(|(_, s)| s).unwrap_or(0);
            let _ = inner.lines.set_lines(build_wire_lines(&self.layout, state));
            inner.drawn_key = Some((layout_key(&self.layout), state));
        }
    }

    fn frame(
        &self,
        _context: &Context,
        _info: &stardust_xr_fusion::client::FrameInfo,
        state: &mut State,
        inner: &mut Self::Inner,
    ) {
        let _ = inner.input.handle_events();
        let bead = self.layout.bead_pos();
        let on_bead = |p: Vec3| p.distance(bead) < BEAD_GRAB_M;
        let near = inner
            .input
            .input()
            .values()
            .any(|s| hand_point(s).is_some_and(|p| p.distance(bead) < BEAD_GRAB_M * 2.5));
        let now = std::time::Instant::now();
        let mut was_grabbing = inner.reach.grabbing;
        let mut ray_end: Option<Vec3> = None;
        let mut palm_pt: Option<Vec3> = None;

        for snap in inner.input.input().values() {
            let InputDataType::Hand { data } = snap.input() else { continue };
            let palm = Vec3::from(data.palm.pose.position);
            palm_pt = Some(palm);
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
            let mut axes = palm_wm::hand_axes_from_quat(palm.to_array(), [o.v.x, o.v.y, o.v.z, o.s], right);
            let proto_n = Vec3::from(data.palm_normal());
            axes.palm_normal = proto_n.to_array();
            axes.palm_back = (-proto_n).to_array();
            axes.distal = Vec3::from(data.distal_axis()).to_array();
            let toward = palm_wm::toward_cylinder_content(palm.to_array(), self.layout.radius_m);
            let live_dir = palm_wm::reach_direction(&axes, inner.reach.cfg());
            let live_origin = palm_wm::reach_origin(&axes, inner.reach.cfg());
            inner.reach.step(now, dists, &axes, toward, pinch_gap, live_dir, live_origin);

            if let Some((orig, dir)) = inner.reach.aim() {
                if palm_wm::ray_hits_bead(orig, dir, bead.to_array(), palm_wm::BEAD_RAY_HIT_M) {
                    if let Some((closest, _)) = palm_wm::ray_closest_point(orig, dir, bead.to_array()) {
                        ray_end = Some(Vec3::from(closest));
                    }
                    inner.palm_aim_bead = inner.reach.armed && !inner.reach.grabbing;
                } else {
                    inner.palm_aim_bead = false;
                    ray_end = Some(Vec3::from(orig) + Vec3::from(dir) * 0.8);
                }
            } else {
                inner.palm_aim_bead = false;
            }
            break;
        }

        // Palm-reach bead grab (distant).
        if inner.reach.grabbing && !was_grabbing && (inner.palm_aim_bead || ray_end.is_some()) {
            // Only claim bead if aim was on bead at grab start — check locked ray.
            if let Some((o, d)) = inner.reach.aim() {
                if palm_wm::ray_hits_bead(o, d, bead.to_array(), palm_wm::BEAD_RAY_HIT_M) {
                    inner.last_grab_pt = palm_pt.or(ray_end);
                    tracing::info!(bead = ?bead, "cylinder bead palm-reach grabbed");
                }
            }
        }
        if inner.reach.grabbing {
            if let Some((o, d)) = inner.reach.aim() {
                if palm_wm::ray_hits_bead(o, d, bead.to_array(), palm_wm::BEAD_RAY_HIT_M * 1.5)
                    || was_grabbing
                {
                    if let Some(p) = palm_pt.or(ray_end) {
                        if let Some(prev) = inner.last_grab_pt {
                            let (dyaw, dh) = grab_frame_deltas(prev, p);
                            if dyaw.abs() > 1e-6 || dh.abs() > 1e-6 {
                                (self.on_grab_delta.0)(state, dyaw, dh);
                            }
                        }
                        inner.last_grab_pt = Some(p);
                    }
                }
            }
        } else if was_grabbing {
            if inner.last_grab_pt.is_some() {
                inner.last_grab_pt = None;
                (self.on_grab_end.0)(state);
                tracing::info!("cylinder bead palm-reach released");
            }
        }

        let palm_owns = inner.reach.grabbing
            && inner
                .reach
                .aim()
                .is_some_and(|(o, d)| palm_wm::ray_hits_bead(o, d, bead.to_array(), palm_wm::BEAD_RAY_HIT_M * 1.5));

        if !palm_owns {
            inner.grab.update(
                true,
                &inner.input,
                |s| hand_point(s).is_some_and(on_bead),
                pinch_or_grab,
            );
            if inner.grab.actor_started() {
                inner.last_grab_pt = inner.grab.actor().and_then(|s| hand_point(s));
                tracing::info!(bead = ?bead, "cylinder bead grabbed");
            }
            if inner.grab.actor_acting() {
                if let Some(p) = inner.grab.actor().and_then(|s| hand_point(s)) {
                    if let Some(prev) = inner.last_grab_pt {
                        let (dyaw, dh) = grab_frame_deltas(prev, p);
                        if dyaw.abs() > 1e-6 || dh.abs() > 1e-6 {
                            (self.on_grab_delta.0)(state, dyaw, dh);
                        }
                    }
                    inner.last_grab_pt = Some(p);
                }
            }
            if inner.grab.actor_stopped() {
                inner.last_grab_pt = None;
                tracing::info!("cylinder bead released");
                (self.on_grab_end.0)(state);
            }
        }

        let acting = palm_owns || inner.grab.actor_acting();
        let visual: BeadVisualState = if acting {
            2
        } else if near || inner.palm_aim_bead {
            1
        } else {
            0
        };

        let mut lines = build_wire_lines(&self.layout, visual);
        if (inner.palm_aim_bead || palm_owns) && inner.reach.aim().is_some() {
            if let (Some(o), Some(e)) = (palm_pt, ray_end) {
                let col = if palm_owns {
                    rgba_linear!(0.2, 0.95, 0.55, 0.95)
                } else {
                    rgba_linear!(0.35, 0.85, 1.0, 0.9)
                };
                lines.push(Line {
                    points: vec![
                        LinePoint { point: o.into(), thickness: 0.004, color: col },
                        LinePoint {
                            point: e.into(),
                            thickness: 0.002,
                            color: rgba_linear!(0.35, 0.85, 1.0, 0.0),
                        },
                    ],
                    cyclic: false,
                });
                inner.reach_viz = Some((o, e));
            }
        } else {
            inner.reach_viz = None;
        }

        let key = (layout_key(&self.layout), visual);
        if inner.drawn_key != Some(key) || inner.reach_viz.is_some() {
            inner.drawn_key = Some(key);
            let _ = inner.lines.set_lines(lines);
        }
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::WireLayout;

    #[test]
    fn build_wire_lines_has_two_rings_and_three_bead_circles() {
        let l = WireLayout {
            radius_m: 0.7,
            yaw_rad: 0.1,
            height_m: 0.0,
            panel_height_m: 0.8,
            gap_m: 0.055,
        };
        let lines = build_wire_lines(&l, 0);
        assert_eq!(lines.len(), 5); // 2 rings + 3 bead circles
        assert!(lines.iter().all(|ln| ln.cyclic));
        assert!(lines[0].points.len() >= 90);
    }
}
