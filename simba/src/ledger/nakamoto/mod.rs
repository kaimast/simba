use std::cmp::Ordering;
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;

use asim::time::Time;

use cow_tree::FrozenCowTree;

use rand::prelude::SliceRandom;

use crate::config::Difficulty;
use crate::emit_event;
use crate::events::{BlockEvent, Event};
use crate::logic::{
    AccountId, AccountState, Block, BlockId, Transaction, TransactionId, GENESIS_BLOCK,
    GENESIS_HEIGHT,
};

mod block;
pub use block::NakamotoBlock;

use super::{GlobalLedger, NodeLedger};

uint::construct_uint! {
    pub struct DiffTarget(4);
}

pub type NotifyCommitFn = Box<dyn Fn(&AccountId, &TransactionId)>;

pub const MAX_DIFF_TARGET: DiffTarget = DiffTarget([u64::MAX, u64::MAX, u64::MAX, u64::MAX]);

pub struct NakamotoGlobalLedger {
    num_nodes: u32,
    all_blocks: HashMap<BlockId, Rc<NakamotoBlock>>,
    longest_chain: (BlockId, u64),
}

pub struct NakamotoNodeLedger {
    blocks: HashMap<BlockId, Rc<NakamotoBlock>>,

    /// Keeps track of the head of all forks
    forks: HashMap<BlockId, u64>,

    ///The longest chain we picked to mine on
    longest_chain: (BlockId, u64),

    /// Keeps track of which blocks are marked as uncle by the main chain
    marked_as_uncle: HashSet<BlockId>,

    /// Transaction data
    applied_transactions: HashSet<TransactionId>,
    mempool: HashSet<TransactionId>,
    known_transactions: HashMap<TransactionId, Rc<Transaction>>,

    /// Callbacks
    notify_transaction_commit_fn: Option<NotifyCommitFn>,
}

impl GlobalLedger for NakamotoGlobalLedger {}

impl NakamotoGlobalLedger {
    pub fn new(num_nodes: u32) -> Self {
        let all_blocks = Default::default();
        let longest_chain = (GENESIS_BLOCK, GENESIS_HEIGHT);

        Self {
            num_nodes,
            all_blocks,
            longest_chain,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn generate_block(
        &mut self,
        mined_by: AccountId,
        parent: BlockId,
        uncles: Vec<BlockId>,
        height: u64,
        difficulty: Difficulty,
        transactions: Vec<TransactionId>,
        state: FrozenCowTree<AccountState>,
    ) -> Rc<NakamotoBlock> {
        let block = Rc::new(NakamotoBlock::new(
            mined_by,
            parent,
            uncles,
            height,
            self.num_nodes,
            difficulty,
            transactions,
            state,
        ));

        let block_id = *block.get_identifier();

        self.all_blocks.insert(block_id, block.clone());

        emit_event!(Event::Block {
            identifier: block_id,
            event: BlockEvent::Created {
                height: block.get_height(),
                parent: *block.get_parent_id(),
                uncles: block.get_uncle_ids().to_vec(),
                num_transactions: block.num_transactions(),
            }
        });

        if height > self.longest_chain.1 {
            self.longest_chain = (block_id, height);
            log::debug!(
                "New longest chain head is block #{:#X} with height {} at time {}",
                block.get_identifier(),
                block.get_height(),
                block.get_creation_time().to_seconds()
            );
        }

        block
    }

    pub fn get_total_blocks_mined(&self, start: Time, end: Time) -> u64 {
        let mut count: u64 = 0;

        for (_, block) in self.all_blocks.iter() {
            let creation_time = block.get_creation_time();
            if creation_time >= start && creation_time <= end {
                count += 1;
            }
        }

        count
    }

    pub fn get_longest_chain(&self) -> (BlockId, u64) {
        self.longest_chain
    }

    pub fn get_block(&self, block_id: &BlockId) -> Option<Rc<NakamotoBlock>> {
        self.all_blocks.get(block_id).cloned()
    }
}

impl NodeLedger for NakamotoNodeLedger {}

impl NakamotoNodeLedger {
    pub fn new() -> Self {
        // genesis block does not contain any data and does not contribute to chain length
        let longest_chain = (GENESIS_BLOCK, 0);

        let blocks = Default::default();
        let forks = Default::default();
        let known_transactions = Default::default();
        let applied_transactions = Default::default();
        let mempool = Default::default();
        let marked_as_uncle = Default::default();
        let notify_transaction_commit_fn = None;

        Self {
            longest_chain,
            blocks,
            forks,
            known_transactions,
            marked_as_uncle,
            applied_transactions,
            mempool,
            notify_transaction_commit_fn,
        }
    }

    pub fn set_notify_transaction_commit_fn(&mut self, func: NotifyCommitFn) {
        self.notify_transaction_commit_fn = Some(func);
    }

    pub fn get_longest_chain(&self) -> (BlockId, u64) {
        self.longest_chain
    }

    pub fn is_marked_as_uncle(&self, block_id: &BlockId) -> bool {
        self.marked_as_uncle.contains(block_id)
    }

    pub fn get_transactions_from_mempool(&self, max_block_size: u32) -> Vec<TransactionId> {
        let mut transactions = vec![];

        for txn_id in self.mempool.iter() {
            if (transactions.len() as u32) >= max_block_size {
                break;
            }

            transactions.push(*txn_id);
        }

        transactions
    }

    /// Check if a transaction does not only exist but is currently
    /// also considered part of the longest chain
    // Only used for testing right now
    // TODO expose via node?
    #[allow(dead_code)]
    pub fn is_transaction_applied(&self, txn_id: &TransactionId) -> bool {
        self.applied_transactions.contains(txn_id)
    }

    pub fn knows_transaction(&self, txn_id: &TransactionId) -> bool {
        self.known_transactions.contains_key(txn_id)
    }

    pub fn get_transaction(&self, txn_id: &TransactionId) -> Option<Rc<Transaction>> {
        self.known_transactions.get(txn_id).cloned()
    }

    pub fn has_block(&self, block_id: &BlockId) -> bool {
        self.blocks.contains_key(block_id)
    }

    pub fn get_block(&self, block_id: &BlockId) -> Option<Rc<NakamotoBlock>> {
        self.blocks.get(block_id).cloned()
    }

    /// Adds a new block to the ledger
    /// Returns true if this block is actually new
    /// The second part of the tuple contains the new chain head; if the chain head changed
    #[tracing::instrument(skip(self, block))]
    pub fn add_new_block(
        &mut self,
        block: Rc<NakamotoBlock>,
        commit_delay: u64,
    ) -> (bool, Option<Rc<NakamotoBlock>>) {
        let block_id = block.get_identifier();
        let parent_id = *block.get_parent_id();
        let height = block.get_height();

        let prev = self.blocks.insert(*block.get_identifier(), block.clone());
        if prev.is_some() {
            log::trace!("Got same block more than once");
            return (false, None);
        };

        self.forks.remove(&parent_id);
        self.forks.insert(*block_id, height);

        block.mark_as_seen();

        let mut chain_head = None;

        if height >= self.longest_chain.1 {
            if self.longest_chain.0 == GENESIS_BLOCK {
                self.longest_chain = self.pick_fork();
                assert_ne!(self.longest_chain.0, GENESIS_BLOCK);

                let new_head = self.blocks.get(&self.longest_chain.0).unwrap().clone();
                self.update_chain_head(None, &new_head, commit_delay);
                chain_head = Some(new_head);
            } else {
                // Tied or longer than current chain
                let old_head = self.blocks.get(&self.longest_chain.0).unwrap().clone();
                self.longest_chain = self.pick_fork();
                let new_head = self.blocks.get(&self.longest_chain.0).unwrap().clone();

                if old_head.get_identifier() != new_head.get_identifier() {
                    self.update_chain_head(Some(&old_head), &new_head, commit_delay);
                    chain_head = Some(new_head);
                }
            }
        }

        (true, chain_head)
    }

    pub fn update_chain_head(
        &mut self,
        old_head: Option<&Rc<NakamotoBlock>>,
        new_head: &Rc<NakamotoBlock>,
        commit_delay: u64,
    ) {
        let mut new_chain = VecDeque::new();

        // This walks back the old forks and then walks forward on the new fork
        if let Some(old_head) = old_head {
            assert!(old_head.get_height() <= new_head.get_height());

            let mut old_ancestor = old_head;
            let mut new_ancestor = new_head;

            while new_ancestor.get_height() > old_ancestor.get_height() {
                new_chain.push_back(new_ancestor);
                new_ancestor = self.blocks.get(new_ancestor.get_parent_id()).unwrap();
            }

            let mut walk_back_count = 0;

            while new_ancestor.get_identifier() != old_ancestor.get_identifier() {
                walk_back_count += 1;

                // This can happen due to long network delays
                if walk_back_count >= commit_delay {
                    log::warn!("Undid a committed block!");
                }

                // Undo old block
                for txn_id in old_ancestor.get_transactions() {
                    self.mempool.insert(*txn_id);
                    self.applied_transactions.remove(txn_id);
                }

                for uncle_id in old_ancestor.get_uncle_ids() {
                    if !self.marked_as_uncle.remove(uncle_id) {
                        panic!("Block was never marked as uncle");
                    }
                }

                new_chain.push_back(new_ancestor);

                let next_id = new_ancestor.get_parent_id();

                if *next_id == GENESIS_BLOCK {
                    // Common ancestor is the genesis block
                    // No need to process it (it's empty)
                    break;
                } else {
                    new_ancestor = self.blocks.get(next_id).unwrap();
                    old_ancestor = self.blocks.get(old_ancestor.get_parent_id()).unwrap();
                }
            }
        } else {
            new_chain.push_back(new_head)
        }

        // Apply new block(s)
        while let Some(new_block) = new_chain.pop_back() {
            for uncle_id in new_block.get_uncle_ids() {
                if !self.marked_as_uncle.insert(*uncle_id) {
                    panic!("Block was marked as uncle twice");
                }
            }

            for txn_id in new_block.get_transactions() {
                self.mempool.remove(txn_id);
                self.applied_transactions.insert(*txn_id);
            }
        }

        // After the new fork has been applied, we can check for commits
        if let Some(old_head) = old_head {
            if new_head.get_height() > old_head.get_height() && new_head.get_height() > commit_delay
            {
                let mut committed_block = new_head;

                for _ in 0..commit_delay {
                    committed_block = self
                        .blocks
                        .get(committed_block.get_parent_id())
                        .expect("Failed to get committed block; this should not happen");
                }

                for txn_id in committed_block.get_transactions() {
                    //TODO store older state in a more efficient way
                    let txn = self
                        .known_transactions
                        .get(txn_id)
                        .expect("block contained unknown transaction");
                    if !self.applied_transactions.contains(txn_id) {
                        panic!("Committed transaction was never applied");
                    }

                    if let Some(func) = &self.notify_transaction_commit_fn {
                        func(txn.get_source(), txn_id);
                    }
                }
            }
        }
    }

    /// Picks the longest chain
    /// If there a tie it will randomly pick one of the longest forks
    fn pick_fork(&self) -> (BlockId, u64) {
        let mut longest_forks = vec![GENESIS_BLOCK];
        let mut max_length = 0;

        for (block_id, length) in self.forks.iter() {
            match length.cmp(&max_length) {
                Ordering::Greater => {
                    max_length = *length;
                    longest_forks = vec![*block_id];
                }
                Ordering::Equal => {
                    longest_forks.push(*block_id);
                }
                Ordering::Less => {}
            }
        }

        let mut rng = rand::thread_rng();
        let block = longest_forks.choose(&mut rng).unwrap();

        (*block, max_length)
    }

    pub fn get_forks(&self) -> &HashMap<BlockId, u64> {
        &self.forks
    }

    pub fn add_transaction(&mut self, transaction: Rc<Transaction>) -> bool {
        let txn_id = *transaction.get_identifier();

        let prev = self.known_transactions.insert(txn_id, transaction);

        if prev.is_some() {
            log::trace!("Got the same transaction more than once");
            return false;
        }

        self.mempool.insert(txn_id);

        if self.mempool.len() > 1_000_000 {
            log::warn!("Mempool size is very large");
        }

        true
    }
}

#[cfg(test)]
mod tests;
