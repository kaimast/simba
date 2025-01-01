use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet};
use std::rc::Rc;

use crate::clients::Client;
use crate::config::{Connectivity, TimeoutConfig};
use crate::ledger::{ConventionalBlock, ConventionalGlobalLedger, SlotNumber};
use crate::link::Link;
use crate::logic::{Block, GlobalLogic, NodeLogic, Transaction, GENESIS_BLOCK, SIGNATURE_SIZE};
use crate::message::MessageType;
use crate::metrics::ChainMetrics;
use crate::node::NodeIndex;
use crate::object::ObjectId;
use crate::RcCell;

use asim::time::{Duration, Time};

mod node;
pub use node::PbftNodeLogic;

#[derive(Clone, Debug)]
pub enum PbftMessage {
    SendTransaction(Rc<Transaction>),
    PrePrepare { block: Rc<ConventionalBlock> },
    Prepare { slot: SlotNumber },
    Commit { slot: SlotNumber },
}

impl PbftMessage {
    pub fn get_size(&self) -> u64 {
        let body_size = match self {
            Self::SendTransaction(_) => 0,
            Self::PrePrepare { block } => block.get_size(),
            Self::Prepare { .. } | Self::Commit { .. } => std::mem::size_of::<SlotNumber>() as u64,
        };

        body_size + SIGNATURE_SIZE
    }

    fn get_slot(&self) -> Option<SlotNumber> {
        match self {
            Self::PrePrepare { block } => Some(block.get_slot_number()),
            Self::Prepare { slot } | Self::Commit { slot } => Some(*slot),
            Self::SendTransaction(_) => None,
        }
    }

    pub fn get_type(&self) -> MessageType {
        match self {
            Self::SendTransaction(_) => MessageType::Transaction,
            _ => MessageType::Other,
        }
    }
}

pub struct PbftGlobalLogic {
    global_ledger: RcCell<ConventionalGlobalLedger>,

    //Parameters
    max_block_size: u32,
    quorum_size: u32,
    max_block_interval: Duration,
}

/// Keeps track of the state of a single consensus round
#[derive(Default)]
struct RoundState {
    block: Option<Rc<ConventionalBlock>>,
    prepared_nodes: HashSet<ObjectId>,
    committed_nodes: HashSet<ObjectId>,
}

#[derive(Clone, Copy, Debug, PartialEq, derive_more::Display)]
enum PbftRole {
    Leader,
    Replica,
}

impl PbftGlobalLogic {
    pub fn instantiate(
        num_nodes: u32,
        max_block_size: u32,
        max_block_interval: u64,
    ) -> Rc<dyn GlobalLogic> {
        let f = (num_nodes - 1) / 3;
        let quorum_size = num_nodes - f;
        let global_ledger = Rc::new(RefCell::new(ConventionalGlobalLedger::new()));
        let max_block_interval = Duration::from_millis(max_block_interval);

        log::info!("PBFT set up to tolerate {f} failures for a total of {num_nodes} nodes");

        Rc::new(Self {
            quorum_size,
            max_block_size,
            max_block_interval,
            global_ledger,
        })
    }
}

#[async_trait::async_trait(?Send)]
impl GlobalLogic for PbftGlobalLogic {
    fn new_node_logic(&self, node_id: NodeIndex) -> Rc<dyn NodeLogic> {
        Rc::new(PbftNodeLogic::new(
            self.global_ledger.clone(),
            self.quorum_size,
            self.max_block_size,
            self.max_block_interval,
            node_id,
        ))
    }

    fn get_metrics(
        &self,
        timeout: TimeoutConfig,
        clients: &[Rc<Client>],
        links: &BTreeMap<ObjectId, Rc<Link>>,
    ) -> ChainMetrics {
        let global_ledger = self.global_ledger.borrow_mut();

        let latest_commit = global_ledger.get_latest_commit();

        let mut end_block = global_ledger.get_block(&latest_commit).expect("No blocks");
        loop {
            match timeout {
                TimeoutConfig::Seconds { warmup, runtime } => {
                    let end = Time::from_seconds(warmup + runtime);
                    if end_block.get_creation_time() <= end {
                        break;
                    }
                }
                TimeoutConfig::Blocks { warmup, runtime } => {
                    if end_block.get_height() <= warmup + runtime {
                        break;
                    }
                }
            }

            end_block = global_ledger
                .get_block(end_block.get_parent_id())
                .expect("No parent block");
        }

        let mut blocks_in_interval = 0;
        let mut num_transactions = 0;
        let mut total_size = 0;

        let end_time = end_block.get_creation_time();
        let mut next_block = end_block;

        loop {
            match timeout {
                TimeoutConfig::Seconds { warmup, .. } => {
                    let start = Time::from_seconds(warmup);
                    if next_block.get_creation_time() < start {
                        break;
                    }
                }
                TimeoutConfig::Blocks { warmup, .. } => {
                    if next_block.get_height() < warmup {
                        break;
                    }
                }
            }

            blocks_in_interval += 1;

            num_transactions += next_block.num_transactions() as u64;
            total_size += next_block.get_size();

            if next_block.get_parent_id() == &GENESIS_BLOCK {
                break;
            } else {
                next_block = global_ledger
                    .get_block(next_block.get_parent_id())
                    .expect("No parent block");
            }
        }

        let elapsed = end_time - next_block.get_creation_time();

        // FIXME this also counts blocks in the warmup period
        let avg_block_interval = elapsed.as_seconds_f64() / (global_ledger.num_blocks() as f64);

        let avg_block_size = (total_size as f64) / (blocks_in_interval as f64);

        let mut latencies = vec![];
        for client in clients {
            latencies.append(&mut client.get_latencies().clone());
        }

        // FIXME latencies also contains transactions during warmup period
        // assert_eq!(latencies.len(), num_transactions as usize);

        let avg_latency =
            latencies.iter().map(|t| t.as_millis_f64()).sum::<f64>() / (num_transactions as f64);

        let mut num_network_messages = 0;
        for link in links.values() {
            num_network_messages += link.num_total_messages();
        }

        ChainMetrics {
            total_blocks_mined: blocks_in_interval,
            num_network_messages,
            total_blocks_accepted: blocks_in_interval,
            longest_chain_length: global_ledger.num_blocks() as u64,
            avg_latency,
            avg_block_interval,
            avg_block_propagation: 0.0, //TODO
            num_transactions,
            elapsed,
            avg_block_size,
        }
    }

    fn is_compatible_with_connectivity(&self, connectivity: &Connectivity) -> bool {
        match connectivity {
            Connectivity::Sparse { .. } => false,
            Connectivity::Full => true,
        }
    }

    async fn wait_for_blocks(&self, _blocks: u64) {
        unimplemented!();
    }
}
