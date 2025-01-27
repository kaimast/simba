use std::sync::Arc;
use std::sync::Mutex as StdMutex;

use winit::dpi::PhysicalPosition;

use iced::keyboard::{Event as KeyboardEvent, Key, key};
use iced::mouse::{
    Button as MouseButton, Cursor, Event as MouseEvent, ScrollDelta as MouseScrollDelta,
};
use iced::{Event, Font, Pixels};
use iced_runtime::{Debug, program};
use iced_wgpu::graphics::Viewport;
use iced_winit::conversion;

use simba::Simulation;

use crate::graphics::Geometry;
use crate::graphics::{InputDirection, Renderer};
use crate::scene::SceneManager;
use crate::ui::{CursorPosition, UiEvents, UiLogic, UiMessages};

pub struct UiRenderLoop {
    renderer: Arc<Renderer>,
    messages: Arc<UiMessages>,
    events: Arc<UiEvents>,
    state: program::State<UiLogic>,
    cursor_position: Arc<StdMutex<PhysicalPosition<f64>>>,
    ui_renderer: iced_wgpu::Renderer,
    clipboard: iced_winit::Clipboard,
    scene_manager: Arc<SceneManager>,
    engine: iced_wgpu::Engine,
}

impl UiRenderLoop {
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        renderer: Arc<Renderer>,
        messages: Arc<UiMessages>,
        events: Arc<UiEvents>,
        cursor_position: Arc<CursorPosition>,
        window: Arc<winit::window::Window>,
        simulation: Arc<Simulation>,
        scene_manager: Arc<SceneManager>,
    ) -> Self {
        let clipboard = iced_winit::Clipboard::connect(window);
        let viewport = {
            let geometry = renderer.get_geometry();
            let iced_size =
                iced::Size::new(geometry.window_size.width, geometry.window_size.height);
            Viewport::with_physical_size(iced_size, geometry.scale_factor)
        };

        let device = renderer.get_device();

        let engine = iced_wgpu::Engine::new(
            renderer.get_adapter(),
            device,
            renderer.get_render_queue(),
            renderer.get_texture_format(),
            None,
        );

        let mut ui_renderer =
            iced_wgpu::Renderer::new(device, &engine, Font::with_name("Fira Sans"), Pixels(16.0));

        let mut debug = Debug::new();

        let ui_logic = UiLogic::new(simulation, scene_manager.clone(), messages.clone());

        let state = program::State::new(
            ui_logic,
            viewport.logical_size(),
            &mut ui_renderer,
            &mut debug,
        );

        Self {
            messages,
            events,
            renderer,
            clipboard,
            ui_renderer,
            cursor_position,
            state,
            engine,
            scene_manager,
        }
    }

    pub async fn update_and_draw(
        &mut self,
        geometry: Geometry,
        window: &winit::window::Window,
        surface_view: &wgpu::TextureView,
    ) {
        let mut debug = Debug::new();

        let (uncaught_events, viewport) = {
            let viewport = {
                let size =
                    iced::Size::<u32>::new(geometry.window_size.width, geometry.window_size.height);
                Viewport::with_physical_size(size, geometry.scale_factor)
            };

            log::trace!("Updating UI state");
            for event in self.events.lock().unwrap().drain(..) {
                self.state.queue_event(event);
            }

            for msg in self.messages.take() {
                self.state.queue_message(msg);
            }

            let cursor_position = *self.cursor_position.lock().unwrap();

            let (uncaught_events, _) = self.state.update(
                viewport.logical_size(),
                Cursor::Available(conversion::cursor_position(
                    cursor_position,
                    geometry.scale_factor,
                )),
                &mut self.ui_renderer,
                &iced::Theme::Light,
                &iced_core::renderer::Style {
                    text_color: iced::Color::BLACK,
                },
                &mut self.clipboard,
                &mut debug,
            );

            (uncaught_events, viewport)
        };

        for event in uncaught_events {
            self.handle_event(event);
        }

        // Draw UI
        log::trace!("Rendering UI");
        let device = self.renderer.get_device();
        let queue = self.renderer.get_render_queue();
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        self.ui_renderer.present(
            &mut self.engine,
            device,
            queue,
            &mut encoder,
            None,
            self.renderer.get_texture_format(),
            surface_view,
            &viewport,
            &debug.overlay(),
        );

        log::trace!("Finishing UI");

        self.engine.submit(queue, encoder);

        window.set_cursor(iced_winit::conversion::mouse_interaction(
            self.state.mouse_interaction(),
        ));
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
            Event::Keyboard(keyboard_event) => match keyboard_event {
                KeyboardEvent::KeyPressed { key, .. } => {
                    if let Some(dir) = Self::to_direction(&key) {
                        let camera = self.scene_manager.get_active_camera();
                        camera.notify_button_pressed(dir);
                    }
                }
                KeyboardEvent::KeyReleased { key, .. } => {
                    if let Some(dir) = Self::to_direction(&key) {
                        let camera = self.scene_manager.get_active_camera();
                        camera.notify_button_released(dir);
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn to_direction(key: &Key) -> Option<InputDirection> {
        match key {
            Key::Character(c) => match c.as_str() {
                "w" => Some(InputDirection::Up),
                "a" => Some(InputDirection::Left),
                "s" => Some(InputDirection::Down),
                "d" => Some(InputDirection::Right),
                _ => None,
            },
            Key::Named(key::Named::ArrowUp) => Some(InputDirection::Up),
            Key::Named(key::Named::ArrowLeft) => Some(InputDirection::Left),
            Key::Named(key::Named::ArrowDown) => Some(InputDirection::Down),
            Key::Named(key::Named::ArrowRight) => Some(InputDirection::Right),
            _ => None,
        }
    }
}
