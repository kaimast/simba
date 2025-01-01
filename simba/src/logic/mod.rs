use crate::clients::Client;
use crate::config::TimeoutConfig;
use crate::link::Link;
use crate::metrics::ChainMetrics;
use crate::node::{Node, NodeIndex};
use crate::object::ObjectId;
use crate::{Connectivity, Message};

use cow_tree::FrozenCowTree;

use std::collections::BTreeMap;
use std::rc::Rc;

mod speed_test;
pub use speed_test::*;

mod gossip;
pub use gossip::*;

mod nakamoto;
pub use nakamoto::*;

mod pbft;
pub use pbft::*;

mod snowball;
pub use snowball::*;

mod ethereum2;
//pub use ethereum2::*;

#[derive(Default, Debug, Clone)]
pub struct DummyLogic {}

pub type BlockId = u128;
pub type TransactionId = u128;
pub type AccountId = u128;

/// The id of the genesis block
pub const GENESIS_BLOCK: BlockId = 0;
/// The height (in blocks) of the genesis block
pub const GENESIS_HEIGHT: u64 = 0;

/// Bitcoin signatures are 7 bytes
/// TODO support other protocols
pub const SIGNATURE_SIZE: u64 = 7;

/// A 256-bit hash of a transaction or block
pub const HASH_SIZE: u64 = 16;

/// Size of an integer
pub const NUM_SIZE: u64 = 4;

pub struct AccountState {
    #[allow(dead_code)]
    balance: u64,
}

#[derive(Debug)]
pub struct Transaction {
    identifier: TransactionId,
    // TODO support UTXO model as well
    source: AccountId,
    nonce: u64,
}

pub trait Block {
    fn get_identifier(&self) -> &BlockId;
    fn num_transactions(&self) -> usize;
    fn get_parent_id(&self) -> &BlockId;
    fn get_uncle_ids(&self) -> &[BlockId];
    fn get_height(&self) -> u64;
    fn get_state(&self) -> &FrozenCowTree<AccountState>;
}

impl Transaction {
    pub(crate) fn new(source: AccountId, nonce: u64) -> Self {
        let identifier = rand::random::<TransactionId>();
        Self {
            identifier,
            source,
            nonce,
        }
    }

    pub fn get_identifier(&self) -> &TransactionId {
        &self.identifier
    }

    pub fn get_source(&self) -> &AccountId {
        &self.source
    }

    pub fn get_nonce(&self) -> u64 {
        self.nonce
    }
}

#[async_trait::async_trait(?Send)]
pub trait NodeLogic {
    async fn run(&self, node: Rc<Node>, _is_mining: bool);
    fn init(&self, _node: Rc<Node>);
    fn handle_message(&self, node: &Rc<Node>, source: ObjectId, message: Message);
    fn add_transaction(&self, node: &Node, transction: Rc<Transaction>, source: Option<ObjectId>);
}

#[async_trait::async_trait(?Send)]
pub trait GlobalLogic {
    fn new_node_logic(&self, node_index: NodeIndex) -> Rc<dyn NodeLogic>;
    fn get_metrics(
        &self,
        timeout: TimeoutConfig,
        clients: &[Rc<Client>],
        links: &BTreeMap<ObjectId, Rc<Link>>,
    ) -> ChainMetrics;
    fn is_compatible_with_connectivity(&self, connectivity: &Connectivity) -> bool;
    async fn wait_for_blocks(&self, blocks: u64);
}

#[async_trait::async_trait(?Send)]
impl NodeLogic for DummyLogic {
    async fn run(&self, _node: Rc<Node>, _is_mining: bool) {}
    fn init(&self, _node: Rc<Node>) {}
    fn handle_message(&self, _node: &Rc<Node>, _source: ObjectId, _message: Message) {}
    fn add_transaction(
        &self,
        _node: &Node,
        _transction: Rc<Transaction>,
        _source: Option<ObjectId>,
    ) {
    }
}
