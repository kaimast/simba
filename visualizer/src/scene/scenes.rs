use std::collections::{hash_map, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use simba::{BlockEvent, BlockId, LinkEvent, Location, NodeEvent, Simulation, GENESIS_BLOCK};

use glam::Vec2;

use dashmap::DashMap;

use parking_lot::Mutex;
use tokio::sync::mpsc;

use crate::graphics::{Camera, Drawable, Graphics};
use crate::scene::{Block, BlockConnection, BlockMetrics, Link, Node, ObjectId, SceneObject};
use crate::ui::UiMessages;

use crate::spawn_task;

#[derive(Clone)]
struct ObjWrapper(Arc<dyn SceneObject>);

#[derive(Default)]
struct BlockchainLayout {
    epochs: parking_lot::Mutex<HashMap<u64, usize>>,
    block_positions: parking_lot::Mutex<HashMap<BlockId, Vec2>>,
    minmax_pos: parking_lot::Mutex<(Vec2, Vec2)>,
}

pub struct Scene {
    next_object_id: AtomicU64,
    camera: Arc<Camera>,
    objects: DashMap<ObjectId, ObjWrapper>,
    selected: Mutex<Option<Arc<dyn SceneObject>>>,
}

impl Scene {
    /// This creates all the visual representations of nodes and links
    pub async fn build_network(
        graphics: Arc<Graphics>,
        ui_messages: Arc<UiMessages>,
        simulation: Arc<Simulation>,
    ) -> Arc<Self> {
        let min_pos = Vec2::new(
            Location::MIN_LONGITUDE as f32,
            Location::MIN_LATITUDE as f32,
        );
        let max_pos = Vec2::new(
            Location::MAX_LONGITUDE as f32,
            Location::MAX_LATITUDE as f32,
        );

        let camera = graphics.create_camera(min_pos, max_pos).await;

        camera.look_at(Vec2::new(0.0, 0.0));
        camera.set_zoom(2.0);

        let obj = Arc::new(Scene {
            objects: Default::default(),
            camera,
            selected: Mutex::new(None),
            next_object_id: AtomicU64::new(1),
        });

        let node_map = Arc::new(DashMap::new());
        let (node_event_sender, mut node_event_receiver) = mpsc::unbounded_channel();

        let sim_cpy = simulation.clone();

        {
            let scene = obj.clone();
            let graphics = graphics.clone();

            spawn_task(async move {
                while let Some((node_idx, event)) = node_event_receiver.recv().await {
                    log::debug!("Got new node event index={node_idx} event={event:?}");

                    match event {
                        NodeEvent::Created(node_id) => {
                            let node_map = node_map.clone();
                            let simulation = sim_cpy.clone();

                            let loc = simulation.get_node_location(node_idx);
                            let position = Vec2::new(loc.longitude as f32, loc.latitude as f32);
                            let obj_id = scene.next_object_id.fetch_add(1, Ordering::SeqCst);

                            let scene_obj = Arc::new(
                                Node::new(
                                    obj_id,
                                    node_id,
                                    node_idx,
                                    &graphics,
                                    ui_messages.clone(),
                                    simulation,
                                    position,
                                )
                                .await,
                            );

                            scene.objects.insert(obj_id, ObjWrapper(scene_obj.clone()));
                            node_map.insert(node_idx, scene_obj);

                            log::trace!("Created render object for node #{node_id}");
                        }
                        NodeEvent::StatisticsUpdated => {
                            let node = node_map.get(&node_idx).expect("No such node");
                            node.notify_properties_changed();
                        }
                    }
                }
            });
        }

        simulation.set_node_event_callback(Box::new(move |node_id, event: NodeEvent| {
            if let Err(err) = node_event_sender.send((node_id, event)) {
                log::trace!("Failed to forward node event: {err:?}");
            }
        }));

        let scene = obj.clone();

        let links = Arc::new(DashMap::new());
        let (link_event_sender, mut link_event_receiver) = mpsc::unbounded_channel();

        {
            let graphics = graphics.clone();
            let simulation = simulation.clone();
            spawn_task(async move {
                while let Some((link_id, event)) = link_event_receiver.recv().await {
                    match event {
                        LinkEvent::Created { node1, node2 } => {
                            let obj_id = scene.next_object_id.fetch_add(1, Ordering::SeqCst);

                            let loc1 = simulation.get_node_location(node1);
                            let start = Vec2::new(loc1.longitude as f32, loc1.latitude as f32);

                            let loc2 = simulation.get_node_location(node2);
                            let end = Vec2::new(loc2.longitude as f32, loc2.latitude as f32);

                            let scene_obj =
                                Arc::new(Link::new(obj_id, &graphics, start, end).await);
                            scene.objects.insert(obj_id, ObjWrapper(scene_obj.clone()));
                            links.insert(link_id, scene_obj);
                        }
                        LinkEvent::Active => {
                            links.get(&link_id).expect("no such link").mark_active();
                        }
                        LinkEvent::Inactive => {
                            links.get(&link_id).expect("no such link").mark_inactive();
                        }
                    }
                }
            });
        }

        simulation.set_link_event_callback(Box::new(move |link_id, event: LinkEvent| {
            if let Err(err) = link_event_sender.send((link_id, event)) {
                log::trace!("Failed to forward link event: {err:?}");
            }
        }));

        obj
    }

    pub async fn build_blockchain(
        graphics: Arc<Graphics>,
        ui_messages: Arc<UiMessages>,
        simulation: &Simulation,
    ) -> Arc<Self> {
        let layout = Arc::new(BlockchainLayout::default());

        let metrics = BlockMetrics {
            parent_id: None,
            uncle_ids: vec![],
            num_transactions: 0,
            height: 0,
        };

        //FIXME emit event for genesis block and get rid of this
        let genesis_block = Arc::new(
            Block::new(
                0,
                GENESIS_BLOCK,
                &graphics,
                ui_messages.clone(),
                Vec2::new(0.0, 0.0),
                metrics,
            )
            .await,
        );

        layout
            .block_positions
            .lock()
            .insert(GENESIS_BLOCK, Vec2::new(0.0, 0.0));

        let camera = graphics
            .create_camera(Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0))
            .await;
        camera.look_at(Vec2::new(0.0, 0.0));
        camera.set_zoom(1.0);

        let objects: DashMap<ObjectId, ObjWrapper> = DashMap::new();
        objects.insert(0, ObjWrapper(genesis_block));

        let obj = Arc::new(Scene {
            objects,
            camera,
            selected: Mutex::new(None),
            next_object_id: AtomicU64::new(1),
        });

        let (block_event_sender, mut block_event_receiver) = mpsc::unbounded_channel();

        simulation.set_block_event_callback(Box::new(move |block_id, event: BlockEvent| {
            if let Err(err) = block_event_sender.send((block_id, event)) {
                log::warn!("Forwarding block event failed. Are we shutting down? {err:?}");
            }
        }));

        let scene = obj.clone();

        spawn_task(async move {
            while let Some((block_id, block_event)) = block_event_receiver.recv().await {
                match block_event {
                    BlockEvent::Created {
                        parent,
                        uncles,
                        height,
                        num_transactions,
                    } => {
                        let x = height as f32 * 20.0;

                        let pos = match layout.epochs.lock().entry(height) {
                            hash_map::Entry::Vacant(e) => {
                                e.insert(1);
                                0
                            }
                            hash_map::Entry::Occupied(mut e) => {
                                let pos = *e.get();
                                e.insert(pos + 1);
                                pos
                            }
                        };

                        let y = if pos % 2 == 0 {
                            10.0 * ((pos / 2) as f32)
                        } else {
                            -10.0 * ((1 + pos / 2) as f32)
                        };

                        let pos = Vec2::new(x, y);

                        let minmax_change = {
                            let mut lock = layout.minmax_pos.lock();
                            let (min_pos, max_pos) = *lock;

                            let new_min_pos = Vec2::new(min_pos.x.min(pos.x), min_pos.y.min(pos.y));
                            let new_max_pos = Vec2::new(max_pos.x.max(pos.x), max_pos.y.max(pos.y));

                            if new_min_pos != min_pos || new_max_pos != max_pos {
                                *lock = (new_min_pos, new_max_pos);
                                Some((new_min_pos, new_max_pos))
                            } else {
                                None
                            }
                        };

                        if let Some((min_pos, max_pos)) = minmax_change {
                            scene.get_camera().set_min_max_pos(min_pos, max_pos);
                        }

                        let parent_pos = {
                            let mut block_positions = layout.block_positions.lock();

                            block_positions.insert(block_id, pos);
                            *block_positions.get(&parent).expect("No parent position")
                        };

                        let mut uncle_positions = {
                            let mut uncle_poss = vec![];
                            let block_positions = layout.block_positions.lock();

                            for uncle_id in uncles.iter() {
                                let pos = block_positions.get(uncle_id).expect("No uncle position");

                                uncle_poss.push(*pos);
                            }

                            uncle_poss
                        };

                        let obj_id = scene.next_object_id.fetch_add(1, Ordering::SeqCst);
                        let metrics = BlockMetrics {
                            uncle_ids: uncles,
                            height,
                            num_transactions,
                            parent_id: Some(parent),
                        };

                        let block_obj = Arc::new(
                            Block::new(
                                obj_id,
                                block_id,
                                &graphics,
                                ui_messages.clone(),
                                pos,
                                metrics,
                            )
                            .await,
                        );
                        scene.objects.insert(obj_id, ObjWrapper(block_obj));

                        let conn_id = scene.next_object_id.fetch_add(1, Ordering::SeqCst);
                        let conn_obj = Arc::new(
                            BlockConnection::new_parent(conn_id, &graphics, parent_pos, pos).await,
                        );

                        scene.objects.insert(conn_id, ObjWrapper(conn_obj));

                        for uncle_pos in uncle_positions.drain(..) {
                            let conn_id = scene.next_object_id.fetch_add(1, Ordering::SeqCst);
                            let obj = Arc::new(
                                BlockConnection::new_uncle(conn_id, &graphics, uncle_pos, pos)
                                    .await,
                            );
                            scene.objects.insert(conn_id, ObjWrapper(obj));
                        }
                    }
                }
            }
        });

        obj
    }

    #[tracing::instrument(skip(self))]
    pub fn update(&self) {
        for obj in self.objects.iter() {
            obj.0.update();
        }
    }

    #[tracing::instrument(skip(self))]
    pub fn handle_click(&self, position: Vec2) {
        for obj in self.objects.iter() {
            let obj = &obj.0;
            let drawable = obj.get_drawable();

            //TODO use pixel perfect selection
            if drawable.get_bbox().contains(&position) && obj.is_selectable() {
                let mut selected = self.selected.lock();
                if let Some(prev) = selected.take() {
                    prev.unselect();

                    // Object was clicked again; unselect
                    if prev.get_identifier() == obj.get_identifier() {
                        return;
                    }
                }

                obj.select();
                *selected = Some(obj.clone());
                return;
            }
        }
    }

    pub async fn get_drawables(&self) -> Vec<Arc<Drawable>> {
        let mut result = vec![];

        let view_bbox = self.camera.get_view_bbox();

        for obj in self.objects.iter() {
            let drawable = obj.0.get_drawable();

            // Cull using bounding box
            if view_bbox.overlaps(&drawable.get_bbox()) {
                result.push(drawable);
            }
        }

        result
    }

    pub fn get_camera(&self) -> &Arc<Camera> {
        &self.camera
    }

    #[tracing::instrument(skip(self))]
    pub fn suspend(&self) {
        if let Some(obj) = self.selected.lock().take() {
            obj.unselect();
        }

        self.camera.suspend();
    }

    #[tracing::instrument(skip(self))]
    pub fn resume(&self) {
        self.camera.resume();
    }
}
