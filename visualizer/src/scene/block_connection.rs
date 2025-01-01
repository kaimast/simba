use crate::graphics::{Drawable, Graphics, LineStyle};
use crate::scene::ObjectId;

use std::sync::Arc;

use super::SceneObject;

pub struct BlockConnection {
    identifier: ObjectId,
    line: Arc<Drawable>,
}

fn parent_style() -> LineStyle {
    LineStyle {
        fill_color: super::COLOR3.into_vec4(),
        border_color: super::COLOR4.into_vec4(),
        line_width: 1.0,
        border_width: 0.5,
        ..Default::default()
    }
}

fn uncle_style() -> LineStyle {
    LineStyle {
        fill_color: super::COLOR2.into_vec4(),
        border_color: super::COLOR4.into_vec4(),
        line_width: 0.8,
        border_width: 0.1,
        ..Default::default()
    }
}

impl BlockConnection {
    pub async fn new_parent(
        identifier: ObjectId,
        graphics: &Graphics,
        start: glam::Vec2,
        end: glam::Vec2,
    ) -> Self {
        let line = graphics.create_line(start, end, 2, parent_style()).await;
        Self { identifier, line }
    }

    pub async fn new_uncle(
        identifier: ObjectId,
        graphics: &Graphics,
        start: glam::Vec2,
        end: glam::Vec2,
    ) -> Self {
        let line = graphics.create_line(start, end, 1, uncle_style()).await;
        Self { identifier, line }
    }
}

#[cfg_attr(target_arch="wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl SceneObject for BlockConnection {
    fn get_identifier(&self) -> ObjectId {
        self.identifier
    }

    fn get_drawable(&self) -> Arc<Drawable> {
        self.line.clone()
    }
}
