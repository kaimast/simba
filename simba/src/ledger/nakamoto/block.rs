use std::cell::RefCell;
use std::sync::atomic::{AtomicU32, Ordering as AtomicOrdering};

use asim::time::{Duration, Time};

use cow_tree::FrozenCowTree;

use derivative::Derivative;

use crate::config::Difficulty;
use crate::logic::{
    AccountId, AccountState, Block, BlockId, TransactionId, HASH_SIZE, NUM_SIZE, SIGNATURE_SIZE,
};

#[derive(Derivative)]
#[derivative(Debug)]
pub struct NakamotoBlock {
    pub(super) identifier: BlockId,
    #[allow(dead_code)] //TODO use for metrics
    mined_by: AccountId,
    parent: BlockId,
    uncles: Vec<BlockId>,
    height: u64,
    /// How many nodes have seen this block?
    seen_by: AtomicU32,
    /// Creation time in seconds
    creation_time: Time,
    /// Time it was seen by all nodes
    full_propagation_time: RefCell<Option<Time>>,
    /// What was the difficulty for this block set to?
    /// TODO move difficulty tracking somewhere else
    difficulty: Difficulty,

    num_nodes: u32,

    #[derivative(Debug = "ignore")]
    transactions: Vec<TransactionId>,
    #[derivative(Debug = "ignore")]
    state: FrozenCowTree<AccountState>,
}

impl NakamotoBlock {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        mined_by: AccountId,
        parent: BlockId,
        uncles: Vec<BlockId>,
        height: u64,
        num_nodes: u32,
        difficulty: Difficulty,
        transactions: Vec<TransactionId>,
        state: FrozenCowTree<AccountState>,
    ) -> Self {
        Self::new_with_id(
            rand::random(),
            mined_by,
            parent,
            uncles,
            height,
            num_nodes,
            difficulty,
            transactions,
            state,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn new_with_id(
        identifier: BlockId,
        mined_by: AccountId,
        parent: BlockId,
        uncles: Vec<BlockId>,
        height: u64,
        num_nodes: u32,
        difficulty: Difficulty,
        transactions: Vec<TransactionId>,
        state: FrozenCowTree<AccountState>,
    ) -> Self {
        log::trace!(
            "Node {mined_by} found a new block with id {identifier:#X} and height {height}"
        );

        Self {
            num_nodes,
            mined_by,
            identifier,
            parent,
            uncles,
            height,
            transactions,
            creation_time: asim::time::now(),
            difficulty,
            state,
            seen_by: AtomicU32::new(0),
            full_propagation_time: RefCell::new(None),
        }
    }

    pub fn get_miner(&self) -> AccountId {
        self.mined_by
    }

    pub fn get_creation_time(&self) -> Time {
        self.creation_time
    }

    pub fn has_uncle(&self, id: &BlockId) -> bool {
        for uncle_id in self.uncles.iter() {
            if uncle_id == id {
                return true;
            }
        }

        false
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

    pub fn mark_as_seen(&self) {
        let prev = self.seen_by.fetch_add(1, AtomicOrdering::SeqCst);
        if prev + 1 == self.num_nodes {
            let _ = self
                .full_propagation_time
                .borrow_mut()
                .insert(asim::time::now());
        }
    }

    pub fn get_difficulty(&self) -> &Difficulty {
        &self.difficulty
    }

    /// Get block data size (in bytes)
    pub fn get_size(&self) -> u64 {
        SIGNATURE_SIZE
    }

    /// Get block size including all transaction data
    pub fn get_total_size(&self) -> u64 {
        self.get_size() + (self.transactions.len() as u64) * self.get_transaction_size()
    }

    /// Get size of a size of a transaction
    /// TODO support variable size transactions
    fn get_transaction_size(&self) -> u64 {
        2 * HASH_SIZE + 5 * NUM_SIZE + SIGNATURE_SIZE
    }

    pub fn get_transactions(&self) -> &[TransactionId] {
        &self.transactions
    }
}

impl Block for NakamotoBlock {
    fn get_identifier(&self) -> &BlockId {
        &self.identifier
    }

    fn num_transactions(&self) -> usize {
        self.transactions.len()
    }

    fn get_parent_id(&self) -> &BlockId {
        &self.parent
    }

    fn get_uncle_ids(&self) -> &[BlockId] {
        &self.uncles
    }

    fn get_height(&self) -> u64 {
        self.height
    }

    fn get_state(&self) -> &FrozenCowTree<AccountState> {
        &self.state
    }
}
