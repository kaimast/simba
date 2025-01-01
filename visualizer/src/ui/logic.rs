use crate::scene::{SceneManager, ViewType};
use crate::ui::{ObjectPropertyMap, Statistics, UiMessage, UiMessages};

use std::sync::Arc;

use tokio::sync::mpsc;

use iced::alignment;
use iced::widget::pick_list;
use iced::widget::{Button, Column, Row, Space, Text};
use iced::{Length, Theme};
//use iced_aw::Card;
use iced_runtime::program::Program;

use simba::{GlobalStatistics, Simulation, StatisticsEvent};

use crate::spawn_task;

type UiElement<'a> = iced::Element<'a, UiMessage, Theme, iced_wgpu::Renderer>;

struct SelectedObject {
    name: String,
    properties: ObjectPropertyMap,
}

pub struct UiLogic {
    simulation: Arc<Simulation>,
    scene_manager: Arc<SceneManager>,

    /// State
    selected_view: Option<ViewType>,
    selected_object: Option<SelectedObject>,
    global_stats: GlobalStatistics,
}

impl UiLogic {
    pub fn new(
        simulation: Arc<Simulation>,
        scene_manager: Arc<SceneManager>,
        ui_messages: Arc<UiMessages>,
    ) -> Self {
        let stats_observer = Arc::new(Statistics::new(ui_messages, simulation.clone()));

        let (stats_event_sender, mut stats_event_receiver) = mpsc::unbounded_channel();

        spawn_task(async move {
            while let Some(event) = stats_event_receiver.recv().await {
                assert_eq!(event, StatisticsEvent::Updated);
                stats_observer.notify_updated();
            }
        });

        simulation.set_stats_event_callback(Box::new(move |event| {
            if let Err(err) = stats_event_sender.send(event) {
                log::error!("Failed to forward stats event: {err:?}");
            }
        }));

        Self {
            simulation,
            selected_view: Some(scene_manager.get_active_scene_type()),
            scene_manager,
            global_stats: Default::default(),
            selected_object: None,
        }
    }
}

impl Program for UiLogic {
    type Renderer = iced_wgpu::Renderer;
    type Message = UiMessage;
    type Theme = Theme;

    fn view(&self) -> UiElement {
        log::trace!("Creating new UI View");

        let time = self.simulation.get_current_time();

        // Allows switching between views
        let view_picker = {
            let pick_list = pick_list::PickList::new(
                &ViewType::ALL[..],
                self.selected_view,
                UiMessage::ViewSelected,
            );

            //Card::new(Text::new("View"), pick_list).width(Length::Fixed(150.0))

            Column::new().push(Text::new("View")).push(pick_list)
        };

        // Allows changing simulation speed
        let speed_controls = {
            let time_text =
                Text::new(format!("Elapsed Time: {time}")).align_y(alignment::Vertical::Center);
            let speed = if let Some(rate_limit) = self.simulation.get_rate_limit_f64() {
                format!("{rate_limit}x")
            } else {
                "max".to_string()
            };
            let speed_text = Text::new(speed).align_y(alignment::Vertical::Center);
            let slower_button = Button::new("<")
                .width(Length::Fixed(30.0))
                .padding(0)
                .on_press(UiMessage::DecreaseSpeed);
            let faster_button = Button::new(">")
                .width(Length::Fixed(30.0))
                .padding(0)
                .on_press(UiMessage::IncreaseSpeed);

            let controls = Row::new()
                .spacing(5)
                .push(Text::new("Speed: "))
                .push(slower_button)
                .push(speed_text)
                .push(faster_button);
            let content = Column::new().spacing(5).push(time_text).push(controls);

            //Card::new(Text::new("Simulation"), content)

            Column::new().push(Text::new("Simulation")).push(content)
        };

        let global_stats = {
            let header = Text::new("Global Statistics");

            let stats = &self.global_stats;
            let content = Text::new(format!(
                "Bandwidth Usage {:.3} Mbit/s",
                (stats.network_traffic as f64) / (1024.0 * 1024.0)
            ));

            Column::new().push(header).push(content)
            //Card::new(header, content)
        };

        // The UI elements on the right showing more info
        let cards = Column::new()
            .spacing(10)
            .width(Length::Fixed(400.0))
            .push(speed_controls)
            .push(global_stats);

        // Add info about the selected object (if any)
        let cards = if let Some(SelectedObject { name, properties }) = &self.selected_object {
            let mut content = Column::new();
            for (name, (value, unit)) in properties {
                if let Some(unit) = unit {
                    content =
                        content.push(Text::new(format!("{name} = {value} {}", unit.get_suffix())));
                } else {
                    content = content.push(Text::new(format!("{name} = {value}")));
                }
            }

            let selected_card = Column::new().push(Text::new(name)).push(content);
            //Card::new(Text::new(name), content).on_close(UiMessage::ObjectUnselected);
            cards.push(selected_card)
        } else {
            cards
        };

        Row::new()
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(10)
            .spacing(10)
            .push(view_picker)
            .push(Space::with_width(Length::Fill))
            .push(cards)
            .into()
    }

    fn update(&mut self, message: UiMessage) -> iced::Task<UiMessage> {
        log::trace!("Handling UiMessage: {message:?}");

        match message {
            UiMessage::ViewSelected(view_type) => {
                let scene_manager = self.scene_manager.clone();
                scene_manager.set_active_scene(view_type);
                self.selected_view = Some(view_type);
            }
            UiMessage::ObjectSelected { name, properties } => {
                self.selected_object = Some(SelectedObject { name, properties });
            }
            UiMessage::UpdateSelectedObject { properties } => {
                if let Some(obj) = self.selected_object.as_mut() {
                    obj.properties = properties;
                } else {
                    panic!("no object selected");
                }
            }
            UiMessage::ObjectUnselected => {
                self.selected_object = None;
            }
            UiMessage::UpdateGlobalStatistics(stats) => {
                self.global_stats = stats;
            }
            UiMessage::IncreaseSpeed => {
                let rate_limit = if let Some(current) = self.simulation.get_rate_limit() {
                    if current < 1000 {
                        current + 100
                    } else {
                        current * 2
                    }
                } else {
                    100
                };

                self.simulation.set_rate_limit(rate_limit);
            }
            UiMessage::DecreaseSpeed => {
                let rate_limit = if let Some(current) = self.simulation.get_rate_limit() {
                    if current <= 100 {
                        0
                    } else if current < 1000 {
                        current - 100
                    } else {
                        current / 2
                    }
                } else {
                    100
                };

                self.simulation.set_rate_limit(rate_limit);
            }
        }

        iced::Task::none()
    }
}
