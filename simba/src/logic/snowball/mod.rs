use std::collections::BTreeMap;
use std::rc::Rc;

use tokio::sync::Semaphore;

use asim::time::{START_TIME, Duration};

use crate::clients::Client;
use crate::config::{Connectivity, TimeoutConfig};
use crate::link::Link;
use crate::logic::{GlobalLogic, NodeLogic};
use crate::message::MessageType;
use crate::metrics::ChainMetrics;
use crate::node::NodeIndex;
use crate::object::ObjectId;

mod node;

#[cfg(test)]
mod tests;

pub use node::{Color, SnowballNodeLogic};

#[derive(Clone, Debug)]
pub enum SnowballMessage {
    Query(Color),
    QueryResponse(Color),
    // New message types for enhanced functionality
    Heartbeat,
    HeartbeatResponse,
}

impl SnowballMessage {
    pub fn get_size(&self) -> u64 {
        match self {
            Self::Query(_) | Self::QueryResponse(_) => std::mem::size_of::<Color>() as u64,
            Self::Heartbeat | Self::HeartbeatResponse => 1, // Minimal overhead
        }
    }

    pub fn get_type(&self) -> MessageType {
        match self {
            Self::Query(_) | Self::QueryResponse(_) => MessageType::Other,
            Self::Heartbeat | Self::HeartbeatResponse => MessageType::Other,
        }
    }
}

pub struct SnowballGlobalLogic {
    acceptance_threshold: u32,
    sample_size: u32,
    query_threshold: u32,
    num_nodes: u32,
    accept_sem: Rc<Semaphore>,
    // Enhanced configuration options
    heartbeat_interval: Duration,
    max_rounds: u32,
}

impl SnowballGlobalLogic {
    pub fn instantiate(
        num_nodes: u32,
        acceptance_threshold: u32,
        sample_size_weighted: f64,
        query_threshold_weighted: f64,
        heartbeat_interval: u64,
        max_rounds: u32,
    ) -> Rc<dyn GlobalLogic> {
        let sample_size = (num_nodes as f64 * sample_size_weighted).ceil() as u32;
        let query_threshold = (sample_size as f64 * query_threshold_weighted).ceil() as u32;
        let accept_sem = Rc::new(Semaphore::new(0));

        assert!(sample_size <= num_nodes);
        assert!(query_threshold <= sample_size);
        assert!(acceptance_threshold > 0);
        
        Rc::new(Self {
            acceptance_threshold,
            sample_size,
            query_threshold,
            num_nodes,
            accept_sem,
            heartbeat_interval: Duration::from_millis(heartbeat_interval),
            max_rounds,
        })
    }
}

#[async_trait::async_trait(?Send)]
impl GlobalLogic for SnowballGlobalLogic {
    fn new_node_logic(&self, _node_id: NodeIndex) -> Rc<dyn NodeLogic> {
        Rc::new(SnowballNodeLogic::new(
            self.acceptance_threshold,
            self.sample_size,
            self.query_threshold,
            self.accept_sem.clone(),
            self.heartbeat_interval,
            self.max_rounds,
        ))
    }

    fn get_metrics(
        &self,
        _timeout: TimeoutConfig,
        _clients: &[Rc<Client>],
        links: &BTreeMap<ObjectId, Rc<Link>>,
    ) -> ChainMetrics {
        let mut num_network_messages = 0;
        
        for link in links.values() {
            num_network_messages += link.num_total_messages();
        }

        let elapsed = asim::time::now() - START_TIME;

        ChainMetrics {
            total_blocks_mined: 0,
            num_network_messages,
            total_blocks_accepted: 0,
            longest_chain_length: 0,
            avg_latency: 0.0, // TODO: Implement proper latency calculation
            avg_block_propagation: 0.0, // TODO: Implement block propagation tracking
            avg_block_interval: 0.0,
            num_transactions: 1,
            elapsed,
            avg_block_size: 1.0,
        }
    }

    fn is_compatible_with_connectivity(&self, connectivity: &Connectivity) -> bool {
        match connectivity {
            Connectivity::Sparse { .. } => {
                // Snowball can work with sparse connectivity but with reduced performance
                // We'll allow it but log a warning
                log::warn!("Snowball running on sparse connectivity - performance may be degraded");
                true
            }
            Connectivity::Full => true,
        }
    }

    async fn wait_for_blocks(&self, blocks: u64) {
        assert_eq!(blocks, 1);

        // This blocks until all nodes have accepted a color
        self.accept_sem
            .acquire_many(self.num_nodes)
            .await
            .expect("Waiting for accept failed")
            .forget();
    }
}
