use simba::Simulation;

use super::{UiMessage, UiMessages};

use std::sync::Arc;

pub struct Statistics {
    ui_messages: Arc<UiMessages>,
    simulation: Arc<Simulation>,
}

impl Statistics {
    pub fn new(ui_messages: Arc<UiMessages>, simulation: Arc<Simulation>) -> Self {
        Self {
            ui_messages,
            simulation,
        }
    }

    pub fn notify_updated(&self) {
        let data_point = self.simulation.get_global_statistics();
        let msg = UiMessage::UpdateGlobalStatistics(data_point);
        self.ui_messages.push(msg);
    }
}
