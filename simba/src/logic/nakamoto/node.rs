use crate::config::NakamotoBlockGenerationConfig;
use crate::ledger::{NakamotoBlock, NakamotoGlobalLedger, NakamotoNodeLedger};
use crate::logic::{
    AccountId, Block, BlockId, NodeLogic, Transaction, TransactionId, GENESIS_BLOCK,
};
use crate::node::Node;
use crate::object::ObjectId;
use crate::{Message, RcCell};

use cow_tree::CowTree;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use super::NakamotoMessage;
use super::{make_block_generator, BlockGenerator};

struct NodeState {
    local_ledger: NakamotoNodeLedger,

    requested_blocks: HashSet<BlockId>,
    requested_transactions: HashSet<TransactionId>,

    /// NakamotoBlocks for which we do not have a parent yet
    pending_blocks_ancestors: HashMap<BlockId, Vec<(ObjectId, Rc<NakamotoBlock>)>>,

    /// NakamotoBlocks for which we do not have all transactions yet
    pending_blocks_transactions: HashMap<TransactionId, Vec<(ObjectId, Rc<NakamotoBlock>)>>,

    block_generator: Box<dyn BlockGenerator>,
}

pub struct NakamotoNodeLogic {
    state: RefCell<NodeState>,
    global_ledger: RcCell<NakamotoGlobalLedger>,

    /// Parameters
    max_block_size: u32,
    commit_delay: u64,
    use_ghost: bool,
}

impl NodeState {
    fn add_transaction(
        &mut self,
        node: &Node,
        transaction: Rc<Transaction>,
        source: Option<ObjectId>,
        commit_delay: u64,
    ) {
        let txn_id = *transaction.get_identifier();

        if !self.local_ledger.add_transaction(transaction) {
            return;
        }

        if let Some(mut blocks) = self.pending_blocks_transactions.remove(&txn_id) {
            for (id, block) in blocks.drain(..) {
                self.add_new_block(node, block, Some(id), commit_delay);
            }
        }

        let message = NakamotoMessage::NotifyNewTransaction(txn_id);
        node.broadcast(message.into(), source);
    }

    fn add_new_block(
        &mut self,
        node: &Node,
        block: Rc<NakamotoBlock>,
        received_from: Option<ObjectId>,
        commit_delay: u64,
    ) {
        let mut missing_txn = None;
        let parent_id = *block.get_parent_id();
        let block_id = *block.get_identifier();

        // See if we are missing a transaction
        for txn_id in block.get_transactions() {
            if !self.local_ledger.knows_transaction(txn_id) {
                missing_txn = Some(txn_id);

                // Only request if we have not requested it yet
                if self.requested_transactions.insert(*txn_id) {
                    let message = NakamotoMessage::GetTransaction(*txn_id);
                    let source = received_from
                        .expect("Got transaction from self, but do not know all transactions");
                    node.send_to(&source, message);
                }
            }
        }

        if let Some(missing_txn) = missing_txn {
            let idx = received_from.unwrap();
            self.pending_blocks_transactions
                .entry(*missing_txn)
                .or_default()
                .push((idx, block));
            return;
        }

        // Don't add the block if we do not have the parent or uncle (yet)
        let mut missing_ancestors = vec![];

        if parent_id != GENESIS_BLOCK && !self.local_ledger.has_block(&parent_id) {
            missing_ancestors.push(parent_id);
        }

        for uncle_id in block.get_uncle_ids() {
            if !self.local_ledger.has_block(uncle_id) {
                missing_ancestors.push(*uncle_id);
            }
        }

        if !missing_ancestors.is_empty() {
            let source = received_from.expect("Cannot get block without parent from ourselves");

            self.pending_blocks_ancestors
                .entry(missing_ancestors[0])
                .or_default()
                .push((source, block));

            for ancestor_id in missing_ancestors {
                if self.requested_blocks.insert(ancestor_id) {
                    let message = NakamotoMessage::GetBlock(ancestor_id);
                    node.send_to(&source, message);
                }
            }
            return;
        }

        let (is_new_block, new_head) = self.local_ledger.add_new_block(block, commit_delay);

        // This might return false due to concurrency
        // (we received the same block multiple times at once)
        if !is_new_block {
            return;
        }

        log::trace!(
            "Node {} got a new block with index {:#X}",
            node.get_index(),
            block_id
        );
        node.broadcast(
            NakamotoMessage::NotifyNewBlock(block_id).into(),
            received_from,
        );

        if let Some(new_head) = new_head {
            let parent_id = new_head.get_parent_id();

            if parent_id == &GENESIS_BLOCK {
                self.block_generator.update_chain_head(&new_head, None);
            } else {
                let parent = self.local_ledger.get_block(parent_id).unwrap();
                self.block_generator
                    .update_chain_head(&new_head, Some(&parent));
            }
        }

        if let Some(mut blocks) = self.pending_blocks_ancestors.remove(&block_id) {
            for (idx, block) in blocks.drain(..) {
                self.add_new_block(node, block, Some(idx), commit_delay);
            }
        }
    }

    #[tracing::instrument(skip(self, node, message))]
    fn handle_message(
        &mut self,
        node: &Node,
        source: ObjectId,
        message: Message,
        commit_delay: u64,
    ) {
        let message: NakamotoMessage = message.try_into().expect("Invalid message type");

        match message {
            NakamotoMessage::NotifyNewBlock(identifier) => {
                if !self.local_ledger.has_block(&identifier)
                    && !self.requested_blocks.contains(&identifier)
                {
                    self.requested_blocks.insert(identifier);
                    node.send_to(&source, NakamotoMessage::GetBlock(identifier));
                }
            }
            NakamotoMessage::GetBlock(identifier) => {
                let block = self
                    .local_ledger
                    .get_block(&identifier)
                    .expect("No such block");

                node.send_to(&source, NakamotoMessage::SendBlock(block));
            }
            NakamotoMessage::SendBlock(block) => {
                if !self.requested_blocks.remove(block.get_identifier()) {
                    log::error!("Got block we did not ask for");
                }
                self.add_new_block(node, block, Some(source), commit_delay);
            }
            NakamotoMessage::GetTransaction(txn_id) => {
                let txn = self
                    .local_ledger
                    .get_transaction(&txn_id)
                    .expect("No such transaction");

                let msg = NakamotoMessage::SendTransaction(txn);
                node.send_to(&source, msg);
            }
            NakamotoMessage::NotifyNewTransaction(txn_id) => {
                if !self.local_ledger.knows_transaction(&txn_id)
                    && !self.requested_transactions.contains(&txn_id)
                {
                    let msg = NakamotoMessage::GetTransaction(txn_id);
                    node.send_to(&source, msg);
                    self.requested_transactions.insert(txn_id);
                }
            }
            NakamotoMessage::SendTransaction(txn) => {
                //TODO check nonce and discard old transactions

                if !self.requested_transactions.remove(txn.get_identifier()) {
                    log::error!("Got transaction we did not ask for");
                }

                self.add_transaction(node, txn, Some(source), commit_delay);
            }
        }
    }

    #[tracing::instrument(skip(self, node, global_chain))]
    pub fn generate_block(
        &mut self,
        node: &Node,
        global_chain: &RcCell<NakamotoGlobalLedger>,
        max_block_size: u32,
        commit_delay: u64,
        use_ghost: bool,
    ) {
        let (parent_id, height) = self.local_ledger.get_longest_chain();
        let difficulty = self.block_generator.get_difficulty();
        let transactions = self
            .local_ledger
            .get_transactions_from_mempool(max_block_size);

        let block = {
            let mut uncles = vec![];
            let mut blockchain = global_chain.borrow_mut();

            let state = if parent_id == GENESIS_BLOCK {
                CowTree::default().freeze()
            } else {
                //TODO actually modify state
                let parent = blockchain.get_block(&parent_id).unwrap();

                // Reference all blocks not referenced by the parent
                if use_ghost {
                    for (uncle_id, _) in self.local_ledger.get_forks().iter() {
                        if *uncle_id != parent_id && !self.local_ledger.is_marked_as_uncle(uncle_id)
                        {
                            uncles.push(*uncle_id);
                        }
                    }
                }

                parent.get_state().deep_clone().freeze()
            };

            blockchain.generate_block(
                node.get_account_id(),
                parent_id,
                uncles,
                height + 1,
                difficulty,
                transactions,
                state,
            )
        };

        self.add_new_block(node, block, None, commit_delay);
    }
}

impl NakamotoNodeLogic {
    pub(super) fn new(
        block_generation_config: &NakamotoBlockGenerationConfig,
        global_ledger: RcCell<NakamotoGlobalLedger>,
        max_block_size: u32,
        num_block_generators: u32,
        commit_delay: u64,
        use_ghost: bool,
    ) -> Self {
        let requested_blocks = Default::default();
        let requested_transactions = Default::default();
        let pending_blocks_ancestors = Default::default();
        let pending_blocks_transactions = Default::default();

        let block_generator = make_block_generator(num_block_generators, block_generation_config);
        let local_ledger = NakamotoNodeLedger::new();

        let state = NodeState {
            requested_blocks,
            requested_transactions,
            block_generator,
            pending_blocks_ancestors,
            pending_blocks_transactions,
            local_ledger,
        };

        Self {
            commit_delay,
            state: RefCell::new(state),
            global_ledger,
            max_block_size,
            use_ghost,
        }
    }
}

#[async_trait::async_trait(?Send)]
impl NodeLogic for NakamotoNodeLogic {
    fn init(&self, node: Rc<Node>) {
        // Avoid cyclic dependencies between node and logic
        let node = Rc::downgrade(&node);

        let notify_commit_fn = {
            Box::new(move |source: &AccountId, _txn_id: &TransactionId| {
                let node = node.upgrade().unwrap();
                if let Some(client) = node.get_client(source) {
                    client.notify_transaction_commit();
                }
            })
        };

        let mut state = self.state.borrow_mut();
        state
            .local_ledger
            .set_notify_transaction_commit_fn(notify_commit_fn);
    }

    #[tracing::instrument(skip(self, node))]
    async fn run(&self, node: Rc<Node>, is_mining: bool) {
        if !is_mining {
            return;
        }

        let block_generation_resolution = { self.state.borrow().block_generator.get_resolution() };

        loop {
            {
                let mut state = self.state.borrow_mut();
                if state.block_generator.should_create_block(node.get_index()) {
                    state.generate_block(
                        &node,
                        &self.global_ledger,
                        self.max_block_size,
                        self.commit_delay,
                        self.use_ghost,
                    );
                }
            }
            asim::time::sleep(block_generation_resolution).await;
        }
    }

    fn add_transaction(&self, node: &Node, transaction: Rc<Transaction>, source: Option<ObjectId>) {
        let mut state = self.state.borrow_mut();
        state.add_transaction(node, transaction, source, self.commit_delay);
    }

    #[tracing::instrument(skip(self, node, message))]
    fn handle_message(&self, node: &Rc<Node>, source: ObjectId, message: Message) {
        let mut state = self.state.borrow_mut();
        state.handle_message(node, source, message, self.commit_delay);
    }
}
