use std::collections::HashMap;
use std::sync::Arc;

use simba::{BlockId, GENESIS_BLOCK};

use super::SceneObject;
use crate::graphics::{Drawable, Graphics, RectangleStyle};
use crate::scene::ObjectId;
use crate::ui::{ObjectPropertyValue, UiMessage, UiMessages};

pub struct BlockMetrics {
    pub parent_id: Option<BlockId>,
    pub uncle_ids: Vec<BlockId>,
    pub height: u64,
    pub num_transactions: usize,
}

pub struct Block {
    identifier: ObjectId,
    block_id: BlockId,
    rectangle: Arc<Drawable>,
    ui_messages: Arc<UiMessages>,
    metrics: BlockMetrics,
}

fn unselected_block_style() -> RectangleStyle {
    RectangleStyle {
        width: 10.0,
        height: 10.0,
        border_width: 1.0,
        fill_color: super::COLOR1.into_vec4(),
        border_color: super::COLOR4.into_vec4(),
        ..Default::default()
    }
}

fn selected_block_style() -> RectangleStyle {
    RectangleStyle {
        width: 10.0,
        height: 10.0,
        border_width: 2.0,
        fill_color: super::COLOR1.into_vec4(),
        border_color: super::COLOR_BLACK.into_vec4(),
        ..Default::default()
    }
}

impl Block {
    pub async fn new(
        identifier: ObjectId,
        block_id: BlockId,
        graphics: &Graphics,
        ui_messages: Arc<UiMessages>,
        position: glam::Vec2,
        metrics: BlockMetrics,
    ) -> Self {
        let rectangle = graphics
            .create_rectangle(position, 5, unselected_block_style())
            .await;
        Self {
            identifier,
            block_id,
            rectangle,
            ui_messages,
            metrics,
        }
    }
}

#[cfg_attr(target_arch="wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl SceneObject for Block {
    fn get_identifier(&self) -> ObjectId {
        self.identifier
    }

    fn get_drawable(&self) -> Arc<Drawable> {
        self.rectangle.clone()
    }

    fn is_selectable(&self) -> bool {
        true
    }

    fn select(&self) {
        self.rectangle.set_style(selected_block_style());

        let mut properties = HashMap::new();

        let name = if self.block_id == GENESIS_BLOCK {
            "Genesis Block".to_string()
        } else {
            format!("Block #{:X}", self.block_id)
        };

        if self.block_id != GENESIS_BLOCK {
            properties.insert(
                "NumTransactions".to_string(),
                (
                    ObjectPropertyValue::Int(self.metrics.num_transactions as i64),
                    None,
                ),
            );
            properties.insert(
                "Height".to_string(),
                (ObjectPropertyValue::Int(self.metrics.height as i64), None),
            );
            properties.insert(
                "Parent".to_string(),
                (
                    ObjectPropertyValue::Id(*self.metrics.parent_id.as_ref().unwrap()),
                    None,
                ),
            );
            properties.insert(
                "Uncles".to_string(),
                (
                    ObjectPropertyValue::IdList(self.metrics.uncle_ids.clone()),
                    None,
                ),
            );
        }

        let msg = UiMessage::ObjectSelected { name, properties };

        self.ui_messages.push(msg);
    }

    fn unselect(&self) {
        self.rectangle.set_style(unselected_block_style());

        let msg = UiMessage::ObjectUnselected;
        self.ui_messages.push(msg);
    }
}
