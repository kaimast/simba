use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering as AtomicOrdering};

use asim::sync::{Condvar, Mutex};
use asim::time::{Duration, Time};

use derivative::Derivative;

use crate::Connectivity;
use crate::logic::{BlockId, Client, GlobalLogic, Link, NodeLogic, TimeoutConfig};
use crate::message::MessageType;
use crate::metrics::ChainMetrics;
use crate::node::NodeIndex;
use crate::object::ObjectId;

mod node;
pub use node::GossipNodeLogic;

#[derive(Default)]
struct BlockCounter {
    count: Mutex<u32>,
    cond: Condvar,
}

#[derive(Derivative)]
#[derivative(Debug)]
pub struct GossipBlock {
    identifier: BlockId,
    #[derivative(Debug = "ignore")]
    payload: Vec<u8>,
    #[derivative(Debug = "ignore")]
    num_nodes: u32,
    #[derivative(Debug = "ignore")]
    block_counter: Rc<BlockCounter>,
    /// How many nodes have seen this block?
    seen_by: AtomicU32,
    /// Creation time in seconds
    creation_time: Time,
    /// Time it was seen by all nodes
    full_propagation_time: RefCell<Option<Time>>,
}

impl GossipBlock {
    fn new(payload: Vec<u8>, num_nodes: u32, block_counter: Rc<BlockCounter>) -> Self {
        Self {
            payload,
            num_nodes,
            identifier: rand::random(),
            block_counter,
            full_propagation_time: RefCell::new(None),
            seen_by: AtomicU32::new(0),
            creation_time: asim::time::now(),
        }
    }

    pub fn get_identifier(&self) -> BlockId {
        self.identifier
    }

    pub fn get_size(&self) -> u64 {
        self.payload.len() as u64
    }

    /// How long did it take for all (correct) nodes to see this block?
    /// Returns None, if the block has not fully propagated yet
    pub fn get_full_propagation_delay(&self) -> Option<Duration> {
        #[allow(clippy::manual_map)]
        if let Some(seen_time) = *self.full_propagation_time.borrow() {
            Some(seen_time - self.creation_time)
        } else {
            None
        }
    }

    fn mark_as_seen(&self) {
        let prev = self.seen_by.fetch_add(1, AtomicOrdering::SeqCst);
        if prev + 1 == self.num_nodes {
            let _ = self
                .full_propagation_time
                .borrow_mut()
                .insert(asim::time::now());

            let block_counter = self.block_counter.clone();
            asim::spawn(async move {
                *block_counter.count.lock().await += 1;
                block_counter.cond.notify_all();
            });
        }
    }
}

#[allow(clippy::enum_variant_names)]
#[derive(Clone, Debug)]
pub enum GossipMessage {
    NotifyNewBlock(BlockId),
    GetBlock(BlockId),
    SendBlock(Rc<GossipBlock>),
}

impl GossipMessage {
    pub fn get_size(&self) -> u64 {
        match self {
            Self::NotifyNewBlock(_) | Self::GetBlock(_) => std::mem::size_of::<BlockId>() as u64,
            Self::SendBlock(block) => block.get_size(),
        }
    }

    pub fn get_type(&self) -> MessageType {
        match self {
            Self::SendBlock(_) => MessageType::Block,
            _ => MessageType::Other,
        }
    }
}

pub struct GossipGlobalLogic {
    block_size: u32,
    retry_delay: u32,
    num_nodes: u32,
    all_blocks: Rc<RefCell<HashMap<BlockId, Rc<GossipBlock>>>>,
    block_counter: Rc<BlockCounter>,
}

impl GossipGlobalLogic {
    pub fn instantiate(block_size: u32, retry_delay: u32, num_nodes: u32) -> Rc<dyn GlobalLogic> {
        Rc::new(Self {
            block_counter: Default::default(),
            all_blocks: Default::default(),
            block_size,
            num_nodes,
            retry_delay,
        })
    }
}

#[async_trait::async_trait(?Send)]
impl GlobalLogic for GossipGlobalLogic {
    fn new_node_logic(&self, _node_idx: NodeIndex) -> Rc<dyn NodeLogic> {
        Rc::new(GossipNodeLogic::new(
            self.block_size,
            self.retry_delay,
            self.num_nodes,
            self.all_blocks.clone(),
            self.block_counter.clone(),
        ))
    }

    fn get_metrics(
        &self,
        _timeout: TimeoutConfig,
        _clients: &[Rc<Client>],
        links: &BTreeMap<ObjectId, Rc<Link>>,
    ) -> ChainMetrics {
        let mut total_block_propagation = Duration::ZERO;
        let mut propagated_block_count = 0;

        for (_, block) in self.all_blocks.borrow().iter() {
            if let Some(delay) = block.get_full_propagation_delay() {
                total_block_propagation += delay;
                propagated_block_count += 1;
            }
        }

        assert!(propagated_block_count > 0);

        let avg_block_propagation =
            total_block_propagation.as_millis_f64() / (propagated_block_count as f64);

        let mut num_network_messages = 0;
        for link in links.values() {
            num_network_messages += link.num_total_messages();
        }

        ChainMetrics {
            avg_block_propagation,
            avg_block_size: 0.0,
            avg_block_interval: 0.0,
            avg_latency: 0.0,
            elapsed: Duration::ZERO,
            num_transactions: 0,
            num_network_messages,
            total_blocks_accepted: propagated_block_count,
            longest_chain_length: 0,
            total_blocks_mined: 0,
        }
    }

    fn is_compatible_with_connectivity(&self, _connectivity: &Connectivity) -> bool {
        true
    }

    async fn wait_for_blocks(&self, blocks: u64) {
        let mut count = self.block_counter.count.lock().await;
        while (*count as u64) < blocks {
            count = self.block_counter.cond.wait(count).await;
        }
    }
}
