use crate::scene::{SceneManager, ViewType};
use crate::ui::{ObjectPropertyMap, Statistics, UiMessage, UiMessages};

use std::sync::Arc;

use tokio::sync::mpsc;

use iced::alignment;
use iced::widget::pick_list;
use iced::widget::{Button, Column, Row, Space, Text};
use iced::{Length, Element};
//use iced_aw::Card;

use simba::{GlobalStatistics, Simulation, StatisticsEvent};

use crate::spawn_task;

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

    // Simplified update method that can be called directly
    pub fn update(&mut self, message: UiMessage) {
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
    }

    // Simplified view method that can be called directly
    pub fn view(&self) -> Element<'_, UiMessage> {
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
            let speed_text = Text::new(format!("Speed: {speed}")).align_y(alignment::Vertical::Center);

            let increase_button = Button::new(Text::new("+")).on_press(UiMessage::IncreaseSpeed);
            let decrease_button = Button::new(Text::new("-")).on_press(UiMessage::DecreaseSpeed);

            Row::new()
                .push(time_text)
                .push(Space::with_width(Length::Fixed(20.0)))
                .push(speed_text)
                .push(Space::with_width(Length::Fixed(20.0)))
                .push(decrease_button)
                .push(increase_button)
        };

        // Shows selected object properties
        let object_properties = {
            if let Some(selected_object) = &self.selected_object {
                let mut column = Column::new().push(Text::new(format!("Selected: {}", selected_object.name)));

                for (key, value) in &selected_object.properties {
                    // Format the value properly
                    let value_str = match value {
                        (val, Some(unit)) => format!("{} {:?}", val, unit),
                        (val, None) => format!("{}", val),
                    };
                    column = column.push(Text::new(format!("{}: {}", key, value_str)));
                }

                column
            } else {
                Column::new().push(Text::new("No object selected"))
            }
        };

        // Shows global statistics
        let global_statistics = {
            let mut column = Column::new().push(Text::new("Global Statistics"));

            // Use the available network_traffic field
            let network_traffic = self.global_stats.network_traffic;
            column = column.push(Text::new(format!("Network Traffic: {:.3} Mbit/s", 
                (network_traffic as f64) / (1024.0 * 1024.0))));

            column
        };

        // Combine all UI elements
        Column::new()
            .push(view_picker)
            .push(Space::with_height(Length::Fixed(20.0)))
            .push(speed_controls)
            .push(Space::with_height(Length::Fixed(20.0)))
            .push(object_properties)
            .push(Space::with_height(Length::Fixed(20.0)))
            .push(global_statistics)
            .into()
    }
}
