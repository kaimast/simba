use std::cell::RefCell;
/// Logic that can be used to test the network speed
use std::collections::{BTreeMap, HashSet};
use std::rc::Rc;

use crate::clients::Client;
use crate::config::{Connectivity, TimeoutConfig};
use crate::link::Link;
use crate::logic::{ChainMetrics, GlobalLogic, NodeLogic, Transaction};
use crate::message::Message;
use crate::node::{Node, NodeIndex};
use crate::object::ObjectId;

use asim::time::Duration;

#[derive(Clone, Debug)]
pub struct SpeedTestMessage {
    uid: u64,
}

//TODO use gossip logic and remove this...
pub struct SpeedTestGlobalLogic {
    send_speed: u64,
}

pub struct SpeedTestNodeLogic {
    send_speed: u64,
    known_messages: RefCell<HashSet<u64>>,
}

impl SpeedTestMessage {
    pub fn get_uid(&self) -> u64 {
        self.uid
    }

    /// Every message is 1kb
    pub fn get_size(&self) -> u64 {
        1024
    }
}

impl Default for SpeedTestMessage {
    fn default() -> Self {
        Self {
            uid: rand::random(),
        }
    }
}

impl SpeedTestGlobalLogic {
    pub fn instantiate(send_speed: u64) -> Rc<dyn GlobalLogic> {
        Rc::new(Self { send_speed })
    }
}

#[async_trait::async_trait(?Send)]
impl GlobalLogic for SpeedTestGlobalLogic {
    fn new_node_logic(&self, _node_index: NodeIndex) -> Rc<dyn NodeLogic> {
        Rc::new(SpeedTestNodeLogic {
            send_speed: self.send_speed,
            known_messages: Default::default(),
        })
    }

    fn get_metrics(
        &self,
        _timeout: TimeoutConfig,
        _clients: &[Rc<Client>],
        _links: &BTreeMap<ObjectId, Rc<Link>>,
    ) -> ChainMetrics {
        ChainMetrics::default()
    }

    fn is_compatible_with_connectivity(&self, _connectivity: &Connectivity) -> bool {
        true
    }

    async fn wait_for_blocks(&self, _blocks: u64) {
        unimplemented!();
    }
}

#[async_trait::async_trait(?Send)]
impl NodeLogic for SpeedTestNodeLogic {
    async fn run(&self, node: Rc<Node>, _is_mining: bool) {
        // Run sender logic?
        if node.get_index() == 0 {
            // How many 1kbyte packet per second?
            let send_speed = self.send_speed * 1024;
            let send_delay = Duration::from_micros(1_000_000 / send_speed);
            log::debug!("Sending {send_speed} 1kb packets per second. Send delay is {send_delay}.");

            loop {
                node.broadcast(SpeedTestMessage::default().into(), None);
                asim::time::sleep(send_delay).await;
            }
        }
    }

    fn init(&self, _node: Rc<Node>) {}

    fn handle_message(&self, node: &Rc<Node>, source: ObjectId, message: Message) {
        // Forward to all peers
        let message: SpeedTestMessage = message.try_into().unwrap();
        if self.known_messages.borrow_mut().insert(message.get_uid()) {
            node.broadcast(message.into(), Some(source));
        }
    }

    fn add_transaction(
        &self,
        _node: &Node,
        _transction: Rc<Transaction>,
        _source: Option<ObjectId>,
    ) {
        unimplemented!();
    }
}
