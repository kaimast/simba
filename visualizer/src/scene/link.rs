use std::sync::Arc;

use parking_lot::Mutex;

use crate::graphics::{Drawable, Graphics, LineStyle};
use crate::scene::ObjectId;

use super::SceneObject;

struct LinkState {
    active_current: bool,
    active_new: bool,
}

pub struct Link {
    identifier: ObjectId,
    line: Arc<Drawable>,
    state: Mutex<LinkState>,
}

fn active_link_style() -> LineStyle {
    LineStyle {
        fill_color: super::COLOR3.into_vec4(),
        border_color: super::COLOR4.into_vec4(),
        line_width: 1.0,
        border_width: 0.1,
        ..Default::default()
    }
}

fn inactive_link_style() -> LineStyle {
    LineStyle {
        fill_color: super::COLOR4.into_vec4(),
        border_color: super::COLOR4.into_vec4(),
        line_width: 0.5,
        border_width: 0.05,
        ..Default::default()
    }
}

impl Link {
    pub async fn new(
        identifier: ObjectId,
        graphics: &Graphics,
        start: glam::Vec2,
        end: glam::Vec2,
    ) -> Self {
        let line = graphics
            .create_line(start, end, 1, active_link_style())
            .await;
        let state = Mutex::new(LinkState {
            active_current: false,
            active_new: false,
        });

        Self {
            identifier,
            line,
            state,
        }
    }

    pub fn mark_active(&self) {
        let mut state = self.state.lock();
        state.active_new = true;
    }

    pub fn mark_inactive(&self) {
        let mut state = self.state.lock();
        state.active_new = false;
    }
}

#[cfg_attr(target_arch="wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl SceneObject for Link {
    fn get_identifier(&self) -> ObjectId {
        self.identifier
    }

    fn update(&self) {
        let new_active = {
            let mut state = self.state.lock();

            if state.active_new == state.active_current {
                None
            } else {
                state.active_current = state.active_new;
                Some(state.active_current)
            }
        };

        if let Some(is_active) = new_active {
            if is_active {
                self.line.set_style(active_link_style());
            } else {
                self.line.set_style(inactive_link_style());
            }
        }
    }

    fn get_drawable(&self) -> Arc<Drawable> {
        self.line.clone()
    }
}
