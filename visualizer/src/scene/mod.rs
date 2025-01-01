use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;

use simba::Simulation;

use crate::graphics::{Camera, Color, Drawable, Graphics};
use crate::ui::UiMessages;

mod node;
pub use node::Node;

mod link;
pub use link::Link;

mod block;
pub use block::{Block, BlockMetrics};

mod block_connection;
pub use block_connection::BlockConnection;

mod scenes;
pub use scenes::Scene;

pub const COLOR1: Color = Color::from_rgba(154, 173, 191, 255);
pub const COLOR2: Color = Color::from_rgba(109, 152, 186, 255);
pub const COLOR3: Color = Color::from_rgba(158, 228, 147, 255);
pub const COLOR4: Color = Color::from_rgba(59, 37, 44, 255);
pub const COLOR5: Color = Color::from_rgba(33, 2, 3, 255);
pub const COLOR_BLACK: Color = Color::from_rgba(0, 0, 0, 255);

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, derive_more::Display)]
pub enum ViewType {
    Network,
    Blockchain,
}

pub type ObjectId = u64;

impl ViewType {
    pub const ALL: [Self; 2] = [Self::Network, Self::Blockchain];
}

#[cfg(not(target_arch = "wasm32"))]
#[async_trait::async_trait]
trait SceneObject: Send + Sync {
    fn get_identifier(&self) -> ObjectId;

    fn update(&self) {}

    fn get_drawable(&self) -> Arc<Drawable>;

    fn is_selectable(&self) -> bool {
        false
    }

    fn select(&self) {}

    fn unselect(&self) {}
}

#[cfg(target_arch = "wasm32")]
#[async_trait::async_trait(?Send)]
trait SceneObject {
    fn get_identifier(&self) -> ObjectId;

    fn update(&self) {}

    fn get_drawable(&self) -> Arc<Drawable>;

    fn is_selectable(&self) -> bool {
        false
    }

    fn select(&self) {}

    fn unselect(&self) {}
}

pub struct SceneManager {
    scenes: HashMap<ViewType, Arc<Scene>>,
    active_scene: Mutex<ViewType>,
}

impl SceneManager {
    pub async fn new(
        graphics: Arc<Graphics>,
        ui_messages: Arc<UiMessages>,
        simulation: Arc<Simulation>,
    ) -> Self {
        let network_scene =
            Scene::build_network(graphics.clone(), ui_messages.clone(), simulation.clone()).await;
        let blockchain_scene =
            Scene::build_blockchain(graphics.clone(), ui_messages, &simulation).await;

        let mut scenes = HashMap::new();
        scenes.insert(ViewType::Network, network_scene);
        scenes.insert(ViewType::Blockchain, blockchain_scene);

        let active_scene = ViewType::Network;
        scenes[&active_scene].resume();

        Self {
            scenes,
            active_scene: Mutex::new(active_scene),
        }
    }

    pub fn update(&self) {
        self.get_active_scene().update();
    }

    pub fn set_active_scene(&self, view_type: ViewType) {
        let old;
        let new;

        {
            let mut active_scene = self.active_scene.lock();
            assert!(view_type != *active_scene);

            old = *active_scene;
            *active_scene = view_type;
            new = view_type;
        }

        self.scenes[&old].suspend();
        self.scenes[&new].resume();
    }

    pub fn get_active_scene_type(&self) -> ViewType {
        *self.active_scene.lock()
    }

    pub fn get_active_scene(&self) -> &Scene {
        let active_scene = self.get_active_scene_type();
        &self.scenes[&active_scene]
    }

    pub fn get_active_camera(&self) -> &Arc<Camera> {
        let active_scene = self.get_active_scene_type();
        let scene = &self.scenes[&active_scene];

        scene.get_camera()
    }

    pub fn notify_resize(&self) {
        for (_, scene) in self.scenes.iter() {
            scene.get_camera().notify_resize();
        }
    }

    pub async fn get_drawables(&self) -> (&Arc<Camera>, Vec<Arc<Drawable>>) {
        let active_scene = self.get_active_scene_type();
        let scene = &self.scenes[&active_scene];

        let drawables = scene.get_drawables().await;

        (scene.get_camera(), drawables)
    }
}
