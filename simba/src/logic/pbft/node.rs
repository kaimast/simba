use crate::ledger::{
    ConventionalBlock, ConventionalGlobalLedger, ConventionalNodeLedger, SlotNumber,
};
use crate::logic::{Block, NodeLogic, Transaction, GENESIS_BLOCK};
use crate::node::{Node, NodeIndex};
use crate::object::{Object, ObjectId};
use crate::{Message, RcCell};

use std::cell::RefCell;
use std::cmp::Ordering;

use asim::time::{Duration, Time};

use cow_tree::CowTree;

use super::{PbftMessage, PbftRole, RoundState};

use std::collections::HashMap;
use std::rc::Rc;

use asim::sync::Notify;

struct NodeState {
    role: PbftRole,
    rounds: HashMap<SlotNumber, RoundState>,
    pending_messages: HashMap<SlotNumber, Vec<(ObjectId, PbftMessage)>>,
    current_round: SlotNumber,

    local_ledger: ConventionalNodeLedger,

    last_block_time: Time,
    last_proposed_round: Option<SlotNumber>,
}

pub struct PbftNodeLogic {
    state: RefCell<NodeState>,
    global_ledger: RcCell<ConventionalGlobalLedger>,
    propose_notify: Notify,

    //Parameters
    max_block_size: u32,
    quorum_size: u32,
    max_block_interval: Duration,
}

impl NodeState {
    fn add_transaction(
        &mut self,
        node: &Node,
        transaction: Rc<Transaction>,
        source: Option<ObjectId>,
        propose_notify: &Notify,
        max_block_size: u32,
    ) {
        if !self.local_ledger.add_transaction(transaction.clone()) {
            return;
        }

        // Forward to other nodes?
        if source.is_none() {
            let message = PbftMessage::SendTransaction(transaction);
            node.broadcast(message.into(), None);
        }

        if self.should_propose_block() {
            let pool_size = self.local_ledger.get_mempool_size();

            // If this is the first transaction, wake up the leader
            // to start proposal timer
            // Similarly, wake up the leader if the mempool is full
            //
            // Note: We don't need to worry about race conditions
            // because there is no await between adding the transaction
            // and here
            if pool_size >= max_block_size || pool_size == 1 {
                propose_notify.notify_one();
            }
        }
    }

    /// Are we the leader and is there currently no outstanding block
    fn should_propose_block(&self) -> bool {
        if self.role == PbftRole::Leader {
            match self.last_proposed_round {
                Some(num) => {
                    assert!(num <= self.current_round);
                    num < self.current_round
                }
                None => true,
            }
        } else {
            false
        }
    }

    fn maybe_commit(
        &mut self,
        node: &Node,
        quorum_size: u32,
        max_block_size: u32,
        global_ledger: &RcCell<ConventionalGlobalLedger>,
        propose_notify: &Notify,
    ) {
        let round = self.rounds.get_mut(&self.current_round).unwrap();

        // Only send commit once we have prepared ourselves!
        // Also, only send commit message once
        if (round.prepared_nodes.len() as u32) >= quorum_size
            && round.prepared_nodes.contains(&node.get_identifier())
            && !round.committed_nodes.contains(&node.get_identifier())
        {
            round.committed_nodes.insert(node.get_identifier());

            let message = PbftMessage::Commit {
                slot: self.current_round,
            };
            node.broadcast(message.into(), None);

            if self.role == PbftRole::Leader {
                log::debug!("Leader committed block for slot #{}", self.current_round);
            } else {
                log::trace!(
                    "Replica #{} committed block for slot #{}",
                    node.get_index(),
                    self.current_round
                );
            }

            // Other nodes might already have committed
            self.maybe_finalize(
                node,
                quorum_size,
                max_block_size,
                global_ledger,
                propose_notify,
            );
        }
    }

    fn maybe_finalize(
        &mut self,
        node: &Node,
        quorum_size: u32,
        max_block_size: u32,
        global_ledger: &RcCell<ConventionalGlobalLedger>,
        propose_notify: &Notify,
    ) {
        let round = self.rounds.get_mut(&self.current_round).unwrap();

        // Only finish round once we have committed ourselves
        if (round.committed_nodes.len() as u32) >= quorum_size
            && round.committed_nodes.contains(&node.get_identifier())
        {
            let block = round.block.as_ref().unwrap();
            block.mark_as_accepted();

            for txn in block.get_transactions().iter() {
                if let Some(client) = node.get_client(txn.get_source()) {
                    client.notify_transaction_commit();
                }
            }

            if self.role == PbftRole::Leader {
                global_ledger
                    .borrow_mut()
                    .set_latest_commit(*block.get_identifier());

                log::debug!("Leader finalized block for slot #{}", self.current_round);
                propose_notify.notify_one();
            } else {
                log::trace!(
                    "Replica #{} finalized block for slot #{}",
                    node.get_index(),
                    self.current_round
                );
            }

            self.current_round += 1;
            self.rounds
                .insert(self.current_round, RoundState::default());

            if let Some(mut messages) = self.pending_messages.remove(&self.current_round) {
                for (source, message) in messages.drain(..) {
                    self.handle_message(
                        node,
                        source,
                        message,
                        quorum_size,
                        max_block_size,
                        global_ledger,
                        propose_notify,
                    );
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_message(
        &mut self,
        node: &Node,
        source: ObjectId,
        message: PbftMessage,
        quorum_size: u32,
        max_block_size: u32,
        global_ledger: &RcCell<ConventionalGlobalLedger>,
        propose_notify: &Notify,
    ) {
        if let PbftMessage::SendTransaction(txn) = message {
            self.add_transaction(node, txn, Some(source), propose_notify, max_block_size);
            return;
        }

        let round_num = message.get_slot().expect("Message does not have a slot");

        match self.current_round.cmp(&round_num) {
            Ordering::Greater => {
                // discard?
                log::trace!("Got message for past round");
            }
            Ordering::Less => {
                self.pending_messages
                    .entry(round_num)
                    .or_default()
                    .push((source, message));
                log::trace!("Got message for future round");
                return;
            }
            Ordering::Equal => {}
        }

        let round = self.rounds.get_mut(&round_num).unwrap();

        match message {
            PbftMessage::PrePrepare { block } => {
                if round.block.is_some() {
                    panic!("Got pre-prepare more than once");
                }

                round.block = Some(block);
                round.prepared_nodes.insert(node.get_identifier());

                if self.role == PbftRole::Leader {
                    log::debug!("Leader prepared block for slot #{round_num}");
                } else {
                    log::trace!(
                        "Replica #{} prepared block for slot #{round_num}",
                        node.get_index()
                    );
                }

                let message = PbftMessage::Prepare { slot: round_num };
                node.broadcast(message.into(), None);

                self.maybe_commit(
                    node,
                    quorum_size,
                    max_block_size,
                    global_ledger,
                    propose_notify,
                );
            }
            PbftMessage::Prepare { .. } => {
                round.prepared_nodes.insert(source);
                self.maybe_commit(
                    node,
                    quorum_size,
                    max_block_size,
                    global_ledger,
                    propose_notify,
                );
            }
            PbftMessage::Commit { .. } => {
                round.committed_nodes.insert(source);
                self.maybe_finalize(
                    node,
                    quorum_size,
                    max_block_size,
                    global_ledger,
                    propose_notify,
                );
            }
            PbftMessage::SendTransaction(_) => {
                panic!("Invalid state");
            }
        }
    }

    fn propose_block(
        &mut self,
        node: &Node,
        global_ledger: &RcCell<ConventionalGlobalLedger>,
        quorum_size: u32,
        max_block_size: u32,
        propose_notify: &Notify,
    ) {
        log::debug!("Proposing block for slot #{}", self.current_round);
        self.last_block_time = asim::time::now();
        self.last_proposed_round = Some(self.current_round);

        let parent = if self.current_round > 1 {
            let prev_round = self.current_round - 1;
            *self
                .rounds
                .get(&prev_round)
                .expect("No such round")
                .block
                .as_ref()
                .unwrap()
                .get_identifier()
        } else {
            GENESIS_BLOCK
        };

        let block_id = rand::random();
        let creation_time = asim::time::now();

        let transactions = self
            .local_ledger
            .get_transactions_from_mempool(max_block_size);
        assert!(!transactions.is_empty());

        //FIXME
        let block_state = CowTree::default().freeze();

        let block = Rc::new(ConventionalBlock::new(
            block_id,
            parent,
            node.get_index(),
            transactions,
            creation_time,
            self.current_round,
            block_state,
        ));

        global_ledger
            .borrow_mut()
            .add_block(block_id, block.clone());

        let message = PbftMessage::PrePrepare { block };

        node.broadcast(message.clone().into(), None);

        // Leader is also a replica
        self.handle_message(
            node,
            node.get_identifier(),
            message,
            quorum_size,
            max_block_size,
            global_ledger,
            propose_notify,
        );
    }

    /// Do we have enough pending transactions or did enough time elapse?
    fn can_propose_block(
        &self,
        _node: &Node,
        max_block_interval: Duration,
        max_block_size: u32,
    ) -> Result<(), Option<Duration>> {
        let elapsed = asim::time::now() - self.last_block_time;
        let mempool_size = self.local_ledger.get_mempool_size();

        if mempool_size == 0 {
            log::trace!("Cannot propose yet: no transactions");
            return Err(None);
        }

        if elapsed >= max_block_interval {
            log::trace!("Can propose: max block interval reached");
            Ok(())
        } else if mempool_size >= max_block_size {
            log::trace!("Can propose: max block size reached");
            Ok(())
        } else {
            let wait_time = max_block_interval - elapsed;
            Err(Some(wait_time))
        }
    }
}

#[async_trait::async_trait(?Send)]
impl NodeLogic for PbftNodeLogic {
    fn init(&self, _node: Rc<Node>) {}

    async fn run(&self, node: Rc<Node>, _is_mining: bool) {
        loop {
            let node_role = self.state.borrow().role;

            match node_role {
                PbftRole::Leader => {
                    let mut state = self.state.borrow_mut();
                    let should_propose = state.should_propose_block();
                    if should_propose {
                        match state.can_propose_block(
                            &node,
                            self.max_block_interval,
                            self.max_block_size,
                        ) {
                            Ok(()) => {
                                state.propose_block(
                                    &node,
                                    &self.global_ledger,
                                    self.quorum_size,
                                    self.max_block_size,
                                    &self.propose_notify,
                                );
                            }
                            Err(Some(wait_time)) => {
                                drop(state);

                                let time_fut = asim::time::sleep(wait_time);
                                let notify_fut = self.propose_notify.notified();

                                // Wait for either more transactions or the timer to elapse
                                tokio::select! {
                                    _ = time_fut => {},
                                    _ = notify_fut => {},
                                }
                            }
                            Err(None) => {
                                drop(state);
                                self.propose_notify.notified().await;
                            }
                        }
                    } else {
                        drop(state);
                        self.propose_notify.notified().await;
                    }
                }
                PbftRole::Replica => {
                    //TODO maybe do view change?
                    return;
                }
            }
        }
    }

    fn add_transaction(&self, node: &Node, transaction: Rc<Transaction>, source: Option<ObjectId>) {
        let mut state = self.state.borrow_mut();
        state.add_transaction(
            node,
            transaction,
            source,
            &self.propose_notify,
            self.max_block_size,
        );
    }

    fn handle_message(&self, node: &Rc<Node>, source: ObjectId, message: Message) {
        let message: PbftMessage = message.try_into().expect("Not a PBFT message");
        let mut state = self.state.borrow_mut();

        state.handle_message(
            node,
            source,
            message,
            self.quorum_size,
            self.max_block_size,
            &self.global_ledger,
            &self.propose_notify,
        );
    }
}

impl PbftNodeLogic {
    pub(super) fn new(
        global_ledger: RcCell<ConventionalGlobalLedger>,
        quorum_size: u32,
        max_block_size: u32,
        max_block_interval: Duration,
        node_id: NodeIndex,
    ) -> Self {
        let role = if node_id == 0 {
            PbftRole::Leader
        } else {
            PbftRole::Replica
        };

        log::debug!("Created PBFT node with role {role}");

        let current_round = 1;
        let last_proposed_round = None;
        let mut rounds = HashMap::new();
        let pending_messages = Default::default();
        let last_block_time = Time::from_millis(0);

        let local_ledger = ConventionalNodeLedger::new();

        rounds.insert(current_round, RoundState::default());

        let state = RefCell::new(NodeState {
            role,
            current_round,
            rounds,
            pending_messages,
            local_ledger,
            last_proposed_round,
            last_block_time,
        });

        let propose_notify = Notify::new();

        Self {
            global_ledger,
            quorum_size,
            max_block_interval,
            state,
            max_block_size,
            propose_notify,
        }
    }
}
