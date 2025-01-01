use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use asim::time::{Duration, Time};

use crate::clients::Client;
use crate::config::{Connectivity, NakamotoBlockGenerationConfig, TimeoutConfig};
use crate::ledger::{NakamotoBlock, NakamotoGlobalLedger};
use crate::link::Link;
use crate::logic::{
    Block, BlockId, GlobalLogic, NodeLogic, Transaction, TransactionId, GENESIS_BLOCK, HASH_SIZE,
    NUM_SIZE, SIGNATURE_SIZE,
};
use crate::message::MessageType;
use crate::metrics::ChainMetrics;
use crate::node::NodeIndex;
use crate::object::ObjectId;
use crate::RcCell;

mod node;
pub use node::NakamotoNodeLogic;

mod block_generator;
use block_generator::{make_block_generator, BlockGenerator};

#[derive(Clone, Debug)]
pub enum NakamotoMessage {
    NotifyNewBlock(BlockId),
    NotifyNewTransaction(TransactionId),
    GetTransaction(TransactionId),
    SendTransaction(Rc<Transaction>),
    GetBlock(BlockId),
    SendBlock(Rc<NakamotoBlock>),
}

impl NakamotoMessage {
    pub fn get_size(&self) -> u64 {
        match self {
            Self::NotifyNewBlock(_) | Self::GetBlock(_) => std::mem::size_of::<BlockId>() as u64,
            Self::NotifyNewTransaction(_) | Self::GetTransaction(_) => {
                std::mem::size_of::<TransactionId>() as u64
            }
            Self::SendTransaction(_) => 2 * HASH_SIZE + 5 * NUM_SIZE + SIGNATURE_SIZE,
            Self::SendBlock(block) => block.get_size(),
        }
    }

    pub fn get_type(&self) -> MessageType {
        match self {
            Self::SendTransaction(_) => MessageType::Transaction,
            Self::SendBlock(_) => MessageType::Block,
            _ => MessageType::Other,
        }
    }
}

pub struct NakamotoGlobalLogic {
    global_ledger: RcCell<NakamotoGlobalLedger>,
    max_block_size: u32,
    commit_delay: u64,
    use_ghost: bool,
    num_block_generators: u32,
    block_generation_config: NakamotoBlockGenerationConfig,
}

impl NakamotoGlobalLogic {
    pub fn instantiate(
        block_generation_config: NakamotoBlockGenerationConfig,
        num_block_generators: u32,
        max_block_size: u32,
        commit_delay: u64,
        use_ghost: bool,
    ) -> Rc<dyn GlobalLogic> {
        let global_ledger = Rc::new(RefCell::new(NakamotoGlobalLedger::new(
            num_block_generators,
        )));

        Rc::new(Self {
            block_generation_config,
            global_ledger,
            num_block_generators,
            max_block_size,
            commit_delay,
            use_ghost,
        })
    }
}

#[async_trait::async_trait(?Send)]
impl GlobalLogic for NakamotoGlobalLogic {
    fn new_node_logic(&self, _node_idx: NodeIndex) -> Rc<dyn NodeLogic> {
        Rc::new(NakamotoNodeLogic::new(
            &self.block_generation_config,
            self.global_ledger.clone(),
            self.max_block_size,
            self.num_block_generators,
            self.commit_delay,
            self.use_ghost,
        ))
    }

    fn get_metrics(
        &self,
        timeout: TimeoutConfig,
        clients: &[Rc<Client>],
        links: &BTreeMap<ObjectId, Rc<Link>>,
    ) -> ChainMetrics {
        let blockchain = self.global_ledger.borrow_mut();
        let (latest_block, _height) = blockchain.get_longest_chain();

        let mut end_block = blockchain.get_block(&latest_block).expect("No blocks");
        loop {
            match timeout {
                TimeoutConfig::Seconds { runtime, warmup } => {
                    let end = Time::from_seconds(runtime + warmup);
                    if end_block.get_creation_time() <= end {
                        break;
                    }
                }
                TimeoutConfig::Blocks { runtime, warmup } => {
                    if end_block.get_height() <= runtime + warmup {
                        break;
                    }
                }
            }

            end_block = blockchain
                .get_block(end_block.get_parent_id())
                .expect("No parent block");
        }

        let mut blocks_in_interval = 0;
        let mut num_transactions = 0;
        let mut total_size = 0;

        let mut total_propagated_blocks = 0;
        let mut total_block_propagation = Duration::ZERO;

        let end_time = end_block.get_creation_time();
        let longest_chain_length = end_block.get_height();

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
            num_transactions += next_block.get_transactions().len() as u64;
            total_size += next_block.get_total_size();

            if let Some(prop_time) = next_block.get_full_propagation_delay() {
                total_block_propagation += prop_time;
                total_propagated_blocks += 1;
            }

            if next_block.get_parent_id() == &GENESIS_BLOCK {
                // This should only happen if start time is set to (or close to) zero
                break;
            } else {
                next_block = blockchain
                    .get_block(next_block.get_parent_id())
                    .expect("No parent block");
            }
        }

        let start_time = next_block.get_creation_time();
        let elapsed = end_time - start_time;

        let total_blocks_mined = blockchain.get_total_blocks_mined(start_time, end_time);

        let mut latencies = vec![];
        for client in clients {
            latencies.append(&mut client.get_latencies().clone());
        }

        // num_transactions contains applied but uncommitted transactions as well
        // FIXME also contains transactions during warmup period
        // assert_eq!(latencies.len(), num_transactions as usize);

        let avg_latency =
            latencies.iter().map(|t| t.as_millis_f64()).sum::<f64>() / (latencies.len() as f64);

        let avg_block_size = (total_size as f64) / elapsed.as_seconds_f64();
        let avg_block_interval = elapsed.as_seconds_f64() / (blocks_in_interval as f64);

        let mut num_network_messages = 0;
        for link in links.values() {
            num_network_messages += link.num_total_messages();
        }

        ChainMetrics {
            total_blocks_mined,
            longest_chain_length,
            avg_block_interval,
            avg_block_size,
            avg_latency,
            num_transactions,
            num_network_messages,
            avg_block_propagation: total_block_propagation.as_millis_f64()
                / (total_propagated_blocks as f64),
            total_blocks_accepted: blocks_in_interval,
            elapsed,
        }
    }

    fn is_compatible_with_connectivity(&self, _connectivity: &Connectivity) -> bool {
        true
    }

    async fn wait_for_blocks(&self, _blocks: u64) {
        unimplemented!();
    }
}
