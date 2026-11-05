//! Read-only handlers visible to simular. Optional args: ox oy oz dx dy dz in root space.
use gluon_ipc::{Context, Handler, Interface, RefExt};
use stardust_xr_fusion::{
    client::Client, fields::{FieldRef, RayMarchResult},
    query::{InterfaceDependency, QueriedInterface, QueryableId},
    spatial::{SpatialRef, SpatialInterface},
    spatial_query::{BeamQuery, BeamQueryHandler, BeamQueryHandlerHandler},
    tracked::{Tracked, TrackedExt},
};
use stardust_xr_molecules::{mouse_handler::protocol::MouseHandler, keyboard_handler::protocol::KeyboardHandler};

#[derive(Debug, Handler)]
struct Probe { root: SpatialRef, api: SpatialInterface }
impl BeamQueryHandlerHandler for Probe {
    async fn intersected(&self, _ctx: Context, obj: QueryableId, _field: FieldRef, spatial: SpatialRef,
        interfaces: Vec<QueriedInterface>, sample: RayMarchResult) {
        let transform = self.api.get_relative_transform(self.root.clone(), spatial).await;
        println!("target={obj:?} sample={sample:?} root_transform={transform:?} interfaces={:?}",
            interfaces.iter().map(|i| &i.interface_id).collect::<Vec<_>>());
    }
    async fn interfaces_changed(&self, _: Context, _: QueryableId, _: Vec<QueriedInterface>) {}
    async fn moved(&self, _: Context, _: QueryableId, _: RayMarchResult) {}
    async fn left(&self, _: Context, _: QueryableId) {}
}
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (client, root) = Client::connect(&[]).await?;
    let hmd = Tracked::hmd_spatial().await?;
    println!("hmd={:?}", client.spatial_interface().get_relative_transform(root.clone(), hmd.clone()).await?);
    let args = std::env::args().skip(1).map(|s| s.parse::<f32>()).collect::<Result<Vec<_>, _>>()?;
    let (reference, origin, direction) = if args.len() == 6 {
        let d = glam::Vec3::new(args[3], args[4], args[5]).normalize();
        (root.clone(), [args[0], args[1], args[2]], d.to_array())
    } else if args.is_empty() {
        (hmd.clone(), [0.0; 3], [0.0, 0.0, -1.0])
    } else { return Err("expected no arguments or ox oy oz dx dy dz".into()); };
    println!("ray origin={origin:?} direction={direction:?}");
    let mut keep = Vec::new();
    for id in [MouseHandler::ID, KeyboardHandler::ID] {
        let (node, handler) = BeamQueryHandler::new_node(Probe {root: root.clone(), api: client.spatial_interface().clone()})?;
        let handle = client.spatial_query_interface().beam_query(BeamQuery {
            handler: handler.into_proxy(), interfaces: vec![InterfaceDependency {id: id.into(), optional: false}],
            reference_spatial: reference.clone(), origin: origin.into(), direction: direction.into(),
            max_length: 10.0, margin: 0.0,
        }).await??;
        keep.push((node, handle));
    }
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    Ok(())
}
