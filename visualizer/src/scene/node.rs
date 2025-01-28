use simba::{NodeIndex, ObjectId as SimObjectId, Simulation};

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::graphics::{CircleStyle, Drawable, Graphics};
use crate::scene::ObjectId;
use crate::ui::{
    ObjectPropertyMap, ObjectPropertyUnit, ObjectPropertyValue, UiMessage, UiMessages,
};

use super::SceneObject;

pub struct Node {
    identifier: ObjectId,
    node_index: NodeIndex,
    object_id: SimObjectId,
    ui_messages: Arc<UiMessages>,
    circle: Arc<Drawable>,
    is_selected: AtomicBool,
    simulation: Arc<Simulation>,
}

fn selected_node_style() -> CircleStyle {
    CircleStyle {
        radius: 4.0,
        border_width: 1.0,
        fill_color: super::COLOR1.into_vec4(),
        border_color: super::COLOR_BLACK.into_vec4(),
        ..Default::default()
    }
}

fn unselected_node_style() -> CircleStyle {
    CircleStyle {
        radius: 4.0,
        border_width: 1.0,
        fill_color: super::COLOR1.into_vec4(),
        border_color: super::COLOR4.into_vec4(),
        ..Default::default()
    }
}

impl Node {
    pub async fn new(
        identifier: ObjectId,
        object_id: SimObjectId,
        node_index: NodeIndex,
        graphics: &Graphics,
        ui_messages: Arc<UiMessages>,
        simulation: Arc<Simulation>,
        position: glam::Vec2,
    ) -> Self {
        let circle = graphics
            .create_circle(position, 2, unselected_node_style())
            .await;
        Self {
            is_selected: AtomicBool::new(false),
            identifier,
            object_id,
            node_index,
            circle,
            ui_messages,
            simulation,
        }
    }

    fn generate_properties(&self) -> ObjectPropertyMap {
        let stats = self.simulation.get_node_statistics(self.node_index);
        let mut properties = HashMap::new();
        properties.insert(
            "object_id".to_string(),
            (ObjectPropertyValue::ObjectId(self.object_id), None),
        );

        properties.insert(
            "incoming_data".to_string(),
            (
                ObjectPropertyValue::Int(stats.incoming_data as i64),
                Some(ObjectPropertyUnit::BitsPerSecond),
            ),
        );

        properties
    }

    pub fn notify_properties_changed(&self) {
        if self.is_selected.load(Ordering::SeqCst) {
            let properties = self.generate_properties();
            let msg = UiMessage::UpdateSelectedObject { properties };
            self.ui_messages.push(msg);
        }
    }
}

#[cfg_attr(target_arch="wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl SceneObject for Node {
    fn get_identifier(&self) -> ObjectId {
        self.identifier
    }

    fn update(&self) {}

    fn get_drawable(&self) -> Arc<Drawable> {
        self.circle.clone()
    }

    fn is_selectable(&self) -> bool {
        true
    }

    fn select(&self) {
        self.is_selected.store(true, Ordering::SeqCst);
        self.circle.set_style(selected_node_style());

        let name = format!("Node #{}", self.node_index);
        let properties = self.generate_properties();

        let msg = UiMessage::ObjectSelected { name, properties };
        self.ui_messages.push(msg);
    }

    fn unselect(&self) {
        self.is_selected.store(false, Ordering::SeqCst);
        self.circle.set_style(unselected_node_style());
    }
}
