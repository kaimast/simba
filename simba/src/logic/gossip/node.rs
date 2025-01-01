use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use asim::sync::{SyncCondvar, SyncMutex};
use asim::time::Duration;

use crate::logic::{NodeLogic, Transaction};
use crate::node::Node;
use crate::object::ObjectId;
use crate::{BlockId, Message};

use super::{BlockCounter, GossipBlock, GossipMessage};

pub struct GossipNodeLogic {
    requested_blocks: Rc<SyncMutex<HashSet<BlockId>>>,
    known_blocks: Rc<SyncMutex<HashMap<BlockId, Rc<GossipBlock>>>>,
    block_cond: Rc<SyncCondvar>,
    block_counter: Rc<BlockCounter>,
    all_blocks: Rc<RefCell<HashMap<BlockId, Rc<GossipBlock>>>>,
    num_nodes: u32,
    block_size: u32,
    retry_delay: Duration,
}

impl GossipNodeLogic {
    pub(super) fn new(
        block_size: u32,
        retry_delay: u32,
        num_nodes: u32,
        all_blocks: Rc<RefCell<HashMap<BlockId, Rc<GossipBlock>>>>,
        block_counter: Rc<BlockCounter>,
    ) -> Self {
        Self {
            requested_blocks: Default::default(),
            known_blocks: Default::default(),
            block_cond: Default::default(),
            block_size,
            retry_delay: Duration::from_millis(retry_delay as u64),
            num_nodes,
            all_blocks,
            block_counter,
        }
    }

    /// Record a new block that we received
    fn add_block(&self, block: Rc<GossipBlock>, node: &Node, source: Option<ObjectId>) {
        let block_id = block.get_identifier();
        log::trace!("Got new block with id={block_id}");

        block.mark_as_seen();
        self.known_blocks
            .lock()
            .insert(block.get_identifier(), block);
        self.block_cond.notify_all();
        node.broadcast(GossipMessage::NotifyNewBlock(block_id).into(), source);
    }

    /// Create a new block and send it
    fn generate_block(
        &self,
        node: &Node,
        payload: Vec<u8>,
        num_nodes: u32,
        all_blocks: &RefCell<HashMap<BlockId, Rc<GossipBlock>>>,
        block_counter: Rc<BlockCounter>,
    ) {
        let block = Rc::new(GossipBlock::new(payload, num_nodes, block_counter));
        log::debug!("Created new block with id={}", block.get_identifier());
        all_blocks
            .borrow_mut()
            .insert(block.get_identifier(), block.clone());
        self.add_block(block, node, None);
    }

    /// We heard of a new block; request it
    fn request_new_block(&self, node: Rc<Node>, source: ObjectId, block_id: BlockId) {
        let node = node.clone();
        let known_blocks = self.known_blocks.clone();
        let block_cond = self.block_cond.clone();
        let retry_delay = self.retry_delay;

        // ensure the source is the first node we contact
        let peers = {
            let mut peers = vec![source];
            for peer in node.get_peers() {
                if peer != source {
                    peers.push(peer);
                }
            }
            peers
        };

        asim::spawn(async move {
            let mut known_blocks = known_blocks.lock();
            let mut pos = 0;
            while !known_blocks.contains_key(&block_id) {
                // If we already contacted all peers,
                // just keep trying...
                if pos >= peers.len() {
                    log::debug!(
                        "Node #{} contacted all peers without success",
                        node.get_index()
                    );
                    pos = 0;
                }

                let success = node.send_to(&peers[pos], GossipMessage::GetBlock(block_id));
                if !success {
                    panic!("Failed to send message to peer");
                }

                known_blocks = block_cond
                    .wait_with_timeout(known_blocks, retry_delay)
                    .await;
                pos += 1;
            }
        });
    }
}

#[async_trait::async_trait(?Send)]
impl NodeLogic for GossipNodeLogic {
    fn init(&self, _node: Rc<Node>) {}

    #[tracing::instrument(skip(self, node))]
    async fn run(&self, node: Rc<Node>, _is_mining: bool) {
        if node.get_index() == 0 {
            let payload = vec![0u8; self.block_size as usize];
            self.generate_block(
                &node,
                payload,
                self.num_nodes,
                &self.all_blocks,
                self.block_counter.clone(),
            );
        }
    }

    fn add_transaction(
        &self,
        _node: &Node,
        _transaction: Rc<Transaction>,
        _source: Option<ObjectId>,
    ) {
        // Do nothing
    }

    #[tracing::instrument(skip(self, node, message))]
    fn handle_message(&self, node: &Rc<Node>, source: ObjectId, message: Message) {
        let message: GossipMessage = message.try_into().unwrap();
        match message {
            GossipMessage::NotifyNewBlock(block_id) => {
                let is_new = !self.known_blocks.lock().contains_key(&block_id)
                    && self.requested_blocks.lock().insert(block_id);

                if is_new {
                    self.request_new_block(node.clone(), source, block_id);
                }
            }
            GossipMessage::GetBlock(block_id) => {
                // TODO return error if we don't have this block?
                if let Some(block) = self.known_blocks.lock().get(&block_id) {
                    node.send_to(&source, GossipMessage::SendBlock(block.clone()));
                }
            }
            GossipMessage::SendBlock(block) => {
                if self.requested_blocks.lock().remove(&block.get_identifier()) {
                    self.add_block(block, node, Some(source))
                }
            }
        }
    }
}
