use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};

use super::{GlobalLedger, NodeLedger};

use cow_tree::FrozenCowTree;

use derivative::Derivative;

use crate::emit_event;
use crate::events::{BlockEvent, Event};
use crate::logic::{AccountState, Block, BlockId, Transaction, TransactionId, SIGNATURE_SIZE};
use crate::node::NodeIndex;

use asim::time::Time;

pub type SlotNumber = u64;

pub struct ConventionalGlobalLedger {
    all_blocks: RefCell<HashMap<BlockId, Rc<ConventionalBlock>>>,
    latest_commit: RefCell<Option<BlockId>>,
}

pub struct ConventionalNodeLedger {
    mempool: HashMap<TransactionId, Rc<Transaction>>,
}

#[derive(Derivative)]
#[derivative(Debug)]
pub struct ConventionalBlock {
    identifier: BlockId,
    parent: BlockId,
    slot: SlotNumber,
    creation_time: Time,

    /// How many nodes have accepted this block?
    accept_count: AtomicU32,

    #[allow(dead_code)] //TODO use for metrics
    created_by: NodeIndex,

    #[derivative(Debug = "ignore")]
    transactions: Vec<Rc<Transaction>>,
    #[derivative(Debug = "ignore")]
    state: FrozenCowTree<AccountState>,
}

impl Block for ConventionalBlock {
    fn get_identifier(&self) -> &BlockId {
        &self.identifier
    }

    fn get_parent_id(&self) -> &BlockId {
        &self.parent
    }

    fn num_transactions(&self) -> usize {
        self.transactions.len()
    }

    fn get_height(&self) -> u64 {
        self.slot
    }

    fn get_state(&self) -> &FrozenCowTree<AccountState> {
        &self.state
    }

    fn get_uncle_ids(&self) -> &[BlockId] {
        &[]
    }
}

impl ConventionalBlock {
    pub fn new(
        identifier: BlockId,
        parent: BlockId,
        created_by: NodeIndex,
        transactions: Vec<Rc<Transaction>>,
        creation_time: Time,
        slot: SlotNumber,
        state: FrozenCowTree<AccountState>,
    ) -> Self {
        Self {
            identifier,
            parent,
            created_by,
            accept_count: AtomicU32::new(0),
            transactions,
            creation_time,
            slot,
            state,
        }
    }

    pub fn mark_as_accepted(&self) {
        self.accept_count.fetch_add(1, Ordering::SeqCst);
    }

    pub fn get_creation_time(&self) -> Time {
        self.creation_time
    }

    pub fn get_slot_number(&self) -> SlotNumber {
        self.slot
    }

    /// Get block size including all transaction data
    pub fn get_size(&self) -> u64 {
        (self.transactions.len() as u64) * SIGNATURE_SIZE
    }

    pub fn num_transactions(&self) -> usize {
        self.transactions.len()
    }

    pub fn get_transactions(&self) -> &[Rc<Transaction>] {
        &self.transactions
    }
}

impl GlobalLedger for ConventionalGlobalLedger {}

impl ConventionalGlobalLedger {
    pub fn new() -> Self {
        Self {
            all_blocks: Default::default(),
            latest_commit: RefCell::new(None),
        }
    }

    pub fn get_latest_commit(&self) -> BlockId {
        self.latest_commit.borrow().expect("No block committed")
    }

    pub fn get_block(&self, block_id: &BlockId) -> Option<Rc<ConventionalBlock>> {
        self.all_blocks.borrow().get(block_id).cloned()
    }

    pub fn num_blocks(&self) -> usize {
        self.all_blocks.borrow().len()
    }

    pub fn set_latest_commit(&self, block_id: BlockId) {
        let mut lock = self.latest_commit.borrow_mut();
        *lock = Some(block_id);
    }

    pub fn add_block(&self, block_id: BlockId, block: Rc<ConventionalBlock>) {
        let parent = *block.get_parent_id();
        let height = block.get_height();
        let uncles = block.get_uncle_ids().to_vec();
        let num_transactions = block.num_transactions();

        self.all_blocks.borrow_mut().insert(block_id, block);
        emit_event!(Event::Block {
            identifier: block_id,
            event: BlockEvent::Created {
                height,
                parent,
                uncles,
                num_transactions,
            },
        });
    }
}

impl NodeLedger for ConventionalNodeLedger {}

impl ConventionalNodeLedger {
    pub fn new() -> Self {
        let mempool = Default::default();
        Self { mempool }
    }

    // Add a new transaction; returns true if the txn was not known
    pub fn add_transaction(&mut self, transaction: Rc<Transaction>) -> bool {
        let prev = self
            .mempool
            .insert(*transaction.get_identifier(), transaction);
        prev.is_none()
    }

    pub fn get_mempool_size(&self) -> u32 {
        self.mempool.len() as u32
    }

    pub fn get_transactions_from_mempool(&mut self, max_block_size: u32) -> Vec<Rc<Transaction>> {
        let mut transactions = vec![];

        for (_, txn) in self.mempool.drain() {
            if (transactions.len() as u32) >= max_block_size {
                break;
            }

            transactions.push(txn);
        }

        transactions
    }
}
