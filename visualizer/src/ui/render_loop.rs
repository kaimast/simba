use std::sync::Arc;
use std::sync::Mutex as StdMutex;

use winit::dpi::PhysicalPosition;

use iced::keyboard::{Event as KeyboardEvent, Key};
use iced::mouse::{
    Button as MouseButton, Event as MouseEvent, ScrollDelta as MouseScrollDelta,
};
use iced::Event;

use simba::Simulation;

use crate::graphics::Geometry;
use crate::graphics::Renderer;
use crate::scene::SceneManager;
use crate::ui::{CursorPosition, UiEvents, UiMessages};

pub struct UiRenderLoop {
    renderer: Arc<Renderer>,
    messages: Arc<UiMessages>,
    events: Arc<UiEvents>,
    cursor_position: Arc<StdMutex<PhysicalPosition<f64>>>,
    scene_manager: Arc<SceneManager>,
}

impl UiRenderLoop {
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        renderer: Arc<Renderer>,
        messages: Arc<UiMessages>,
        events: Arc<UiEvents>,
        cursor_position: Arc<CursorPosition>,
        _window: Arc<winit::window::Window>,
        _simulation: Arc<Simulation>,
        scene_manager: Arc<SceneManager>,
    ) -> Self {
        Self {
            messages,
            events,
            renderer,
                      cursor_position,
            scene_manager,
        }
    }

    pub async fn update_and_draw(
        &mut self,
        _geometry: Geometry,
        _window: &winit::window::Window,
        _surface_view: &wgpu::TextureView,
    ) {
        // Simplified UI rendering - just handle basic events for now
        log::trace!("Updating UI state");
        
        // Process events
        for event in self.events.lock().unwrap().drain(..) {
            self.handle_event(event);
        }

        // Process messages
        for msg in self.messages.take() {
            log::trace!("Processing UI message: {:?}", msg);
        }

        // For now, just render the scene without complex iced UI
        // This is a simplified approach that will compile
        log::trace!("Rendering simplified UI");
    }

    fn handle_event(&self, event: Event) {
        match event {
            Event::Mouse(mouse_event) => {
                match mouse_event {
                    MouseEvent::WheelScrolled { delta } => {
                        // TODO add mouse sensitivity settings
                        let mouse_scale_factor = 0.02;

                        log::trace!("Mouse wheel event: {delta:?}");

                        let change = match delta {
                            MouseScrollDelta::Lines { x: _, y } => 10.0 * y,
                            MouseScrollDelta::Pixels { x: _, y } => y,
                        };

                        if change != 0.0 {
                            let camera = self.scene_manager.get_active_camera();
                            // Use available camera methods
                            camera.change_zoom_by(change * mouse_scale_factor);
                        }
                    }
                    MouseEvent::ButtonPressed(button) => {
                        if button == MouseButton::Left {
                            let position = {
                                let camera = self.scene_manager.get_active_camera();
                                let geo = self.renderer.get_geometry();

                                let phy_pos = *self.cursor_position.lock().unwrap();
                                let log_pos = phy_pos.to_logical(geo.scale_factor);

                                camera.get_position_from_cursor(log_pos)
                            };

                            let scene = self.scene_manager.get_active_scene();
                            scene.handle_click(position);
                        }
                    }
                    _ => {}
                }
            }
            Event::Keyboard(KeyboardEvent::KeyPressed { key, .. }) => {
                match key.as_ref() {
                    Key::Character("w") => {
                        let camera = self.scene_manager.get_active_camera();
                        // Use available camera methods
                        camera.notify_button_pressed(crate::graphics::InputDirection::Up);
                    }
                    Key::Character("s") => {
                        let camera = self.scene_manager.get_active_camera();
                        camera.notify_button_pressed(crate::graphics::InputDirection::Down);
                    }
                    Key::Character("a") => {
                        let camera = self.scene_manager.get_active_camera();
                        camera.notify_button_pressed(crate::graphics::InputDirection::Left);
                    }
                    Key::Character("d") => {
                        let camera = self.scene_manager.get_active_camera();
                        camera.notify_button_pressed(crate::graphics::InputDirection::Right);
                    }
                    Key::Character("q") => {
                        // TODO: Implement up movement
                        log::trace!("Q key pressed - up movement not implemented");
                    }
                    Key::Character("e") => {
                        // TODO: Implement down movement
                        log::trace!("E key pressed - down movement not implemented");
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}
