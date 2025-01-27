use std::cell::RefCell;
use std::fs::File;
use std::rc::Rc;

use crate::emit_event;
use crate::events::{Event, StatisticsEvent};
use crate::scene::Scene;

use asim::time::Duration;

use struct_iterable::Iterable as StructIterable;

#[derive(
    PartialEq, Eq, Clone, Debug, Default, StructIterable, derive_more::AddAssign, derive_more::Div,
)]
#[iterable(std::fmt::Display)]
pub struct NodeStatistics {
    /// Incoming data in bytes/s
    pub incoming_data: u64,
}

#[derive(PartialEq, Eq, Clone, Debug, Default, StructIterable)]
#[iterable(std::fmt::Display)]
pub struct GlobalStatistics {
    /// Total network traffic in bytes/s
    pub network_traffic: u64,
}

impl std::ops::AddAssign<NodeStatistics> for GlobalStatistics {
    fn add_assign(&mut self, node_stats: NodeStatistics) {
        self.network_traffic += node_stats.incoming_data;
    }
}

#[derive(Default)]
pub struct NodeStatsCollector {
    pending: NodeStatistics,
    data_points: Vec<NodeStatistics>,
}

impl NodeStatsCollector {
    pub fn update(&mut self) {
        let mut data_point = NodeStatistics::default();
        std::mem::swap(&mut data_point, &mut self.pending);
        self.data_points.push(data_point);
    }

    pub fn get_latest_data_point(&self) -> NodeStatistics {
        self.data_points.last().expect("No data collected").clone()
    }

    pub fn get_average_data(&self) -> NodeStatistics {
        let mut sum = NodeStatistics::default();
        for entry in self.data_points.iter() {
            sum += entry.clone();
        }

        sum / (self.data_points.len() as u64)
    }

    pub fn record_incoming_data(&mut self, bytes: u64) {
        self.pending.incoming_data += bytes;
    }

    fn reset(&mut self) {
        self.data_points.clear();
    }
}

pub struct Statistics {
    stats_file: RefCell<Option<csv::Writer<File>>>,
    data_points: RefCell<Vec<GlobalStatistics>>,
    scene: Rc<Scene>,
}

impl Statistics {
    pub fn new(scene: Rc<Scene>, stats_file: Option<csv::Writer<File>>) -> Self {
        Self {
            scene,
            stats_file: RefCell::new(stats_file),
            data_points: RefCell::new(Default::default()),
        }
    }

    /// Will update statistics every second
    pub async fn run(&self, warmup_time: Duration) {
        if !warmup_time.is_zero() {
            asim::time::sleep(warmup_time);
        }

        log::debug!("Started statistics collection");
        let mut stats_file = self.stats_file.borrow_mut().take();

        // Create CSV header
        if let &mut Some(ref mut stats_file) = &mut stats_file {
            log::debug!("Writing statistics to file");

            let global_stats = GlobalStatistics::default();
            let mut keys = vec!["time".to_string()];

            for (key, _) in global_stats.iter() {
                keys.push(format!("network.{key}"));
            }

            for idx in 0..self.scene.get_nodes().len() {
                let node_stats = NodeStatistics::default();
                for (key, _) in node_stats.iter() {
                    keys.push(format!("nodes.{idx}.{key}"));
                }
            }

            stats_file.write_record(keys).unwrap();
        }

        loop {
            log::trace!("Updating statistics");
            let mut global_stats = GlobalStatistics::default();

            for (_, node) in self.scene.get_nodes().iter() {
                let data = {
                    let mut node_stats = node.get_data().get_statistics();
                    node_stats.update();
                    node_stats.get_latest_data_point()
                };

                global_stats += data;
            }

            if let &mut Some(ref mut stats_file) = &mut stats_file {
                let global_stats = GlobalStatistics::default();
                let mut values = vec![asim::time::now().to_millis().to_string()];

                for (_, val) in global_stats.iter() {
                    values.push(val.to_string());
                }

                for (_, node) in self.scene.get_nodes().iter() {
                    let node_stats = node.get_data().get_statistics().get_latest_data_point();

                    for (_, val) in node_stats.iter() {
                        values.push(val.to_string());
                    }
                }

                stats_file.write_record(values).unwrap();
                stats_file.flush().unwrap();
            }

            emit_event!(Event::Statistics(StatisticsEvent::Updated));
            self.data_points.borrow_mut().push(global_stats);
            asim::time::sleep(Duration::from_seconds(1)).await;
        }
    }

    /// Reset statistics
    /// Used, for example, after warmup
    pub fn reset(&self) {
        for (_, node) in self.scene.get_nodes().iter() {
            node.get_data().get_statistics().reset();
        }

        self.data_points.borrow_mut().clear();
    }

    pub fn get_latest_data_point(&self) -> GlobalStatistics {
        self.data_points
            .borrow()
            .last()
            .expect("Got no statistics")
            .clone()
    }
}
