use std::sync::Arc;

use winit::application::ApplicationHandler as WinitHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::ModifiersState;
use winit::window::WindowId;

use anyhow::Context;

use crate::graphics::Graphics;
use crate::scene::SceneManager;
use crate::ui::{CursorPosition, UiEvents};

#[derive(Default)]
pub struct WindowLoop {}

struct ApplicationHandler {
    ui_events: Arc<UiEvents>,
    graphics: Arc<Graphics>,
    scene_mgr: Arc<SceneManager>,
    cursor_position: Arc<CursorPosition>,
}

impl WindowLoop {
    /// This will block until the window is closed
    pub fn run(
        &self,
        winit_loop: EventLoop<()>,
        ui_events: Arc<UiEvents>,
        graphics: Arc<Graphics>,
        scene_mgr: Arc<SceneManager>,
        cursor_position: Arc<CursorPosition>,
    ) -> anyhow::Result<()> {
        let mut handler = ApplicationHandler {
            ui_events,
            graphics,
            scene_mgr,
            cursor_position,
        };

        winit_loop
            .run_app(&mut handler)
            .with_context(|| "winit failed")
    }
}

impl WinitHandler for ApplicationHandler {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {}

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        window_event: WindowEvent,
    ) {
        let mut modifiers = ModifiersState::default();
        let mut scale_factor = 1.0;

        match window_event {
            WindowEvent::CloseRequested { .. } | WindowEvent::Destroyed { .. } => {
                log::debug!("Close requested. Shutting down...");
                return;
            }
            WindowEvent::ModifiersChanged(new_modifiers) => {
                modifiers = new_modifiers.state();
            }
            WindowEvent::CursorMoved { position, .. } => {
                let mut lock = self.cursor_position.lock().unwrap();
                *lock = position;
            }
            WindowEvent::ScaleFactorChanged {
                scale_factor: new_val,
                ..
            } => {
                log::debug!("Scale factor changed from {scale_factor} to {new_val}");
                scale_factor = new_val;
                self.graphics.get_renderer().set_scale_factor(scale_factor);
            }
            WindowEvent::Resized(new_size) => {
                log::debug!("Window resized to {new_size:?}");
                self.graphics.get_renderer().set_window_size(new_size);
                self.scene_mgr.notify_resize();
            }
            _ => {}
        }

        if let Some(event) =
            iced_winit::conversion::window_event(window_event, scale_factor, modifiers)
        {
            self.ui_events.lock().unwrap().push(event);
        }
    }
}
