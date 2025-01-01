use std::cell::{RefCell, RefMut};
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use asim::network::NetworkMessage;

use serde::{Deserialize, Serialize};

use crate::clients::Client;
use crate::link::Bandwidth;
use crate::logic::{AccountId, NodeLogic, Transaction};
use crate::object::ObjectId;
use crate::stats::NodeStatsCollector;
use crate::Message;

pub type NodeIndex = u32;

pub struct NodeCallback {
    inner: Rc<dyn NodeLogic>,
}

impl NodeCallback {
    pub fn get_logic(&self) -> &dyn NodeLogic {
        &*self.inner
    }
}

#[async_trait::async_trait(?Send)]
impl asim::network::NodeCallback<Message, NodeData> for NodeCallback {
    async fn handle_message(&self, node: &Rc<Node>, source: ObjectId, message: Message) {
        node.get_data()
            .statistics
            .borrow_mut()
            .record_incoming_data(message.get_size());
        self.inner.handle_message(node, source, message);
    }

    fn peer_disconnected(&self, _node: &Node, _peer: ObjectId) {}
}

pub fn get_node_logic(node: &Node) -> &dyn NodeLogic {
    let callback: &NodeCallback = node.get_callback_as();
    callback.get_logic()
}

pub type Node = asim::network::Node<Message, NodeData>;

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Location {
    pub longitude: i16,
    pub latitude: i16,
}

impl Location {
    pub const MAX_LONGITUDE: i16 = 180;
    pub const MIN_LONGITUDE: i16 = -180;
    pub const MAX_LATITUDE: i16 = 90;
    pub const MIN_LATITUDE: i16 = -90;

    pub fn new(longitude: i16, latitude: i16) -> Self {
        assert!(
            (Self::MIN_LONGITUDE..Self::MAX_LONGITUDE).contains(&longitude),
            "Invalid longitude {longitude}"
        );
        assert!(
            (Self::MIN_LATITUDE..Self::MAX_LATITUDE).contains(&latitude),
            "Invalid latitude {latitude}"
        );

        Self {
            longitude,
            latitude,
        }
    }

    pub fn new_random() -> Self {
        // Modulo with negative values does not work as expected
        let longitude = (rand::random::<u32>() % 360) as i16 - 180;
        let latitude = (rand::random::<u32>() % 180) as i16 - 90;

        Self::new(longitude, latitude)
    }

    pub fn distance(&self, other: &Location) -> f32 {
        // TODO wrap around
        let lat = (self.latitude - other.latitude) as f32;
        let long = (self.longitude - other.longitude) as f32;

        // TODO This is not that accurate...
        (lat * lat + long * long).sqrt()
    }
}

pub struct NodeData {
    index: NodeIndex,
    account_id: AccountId,
    location: Location,
    clients: RefCell<HashMap<AccountId, Weak<Client>>>,
    statistics: RefCell<NodeStatsCollector>,
}

impl asim::network::NodeData for NodeData {}

pub fn create_node(
    index: NodeIndex,
    location: Location,
    bandwidth: Bandwidth,
    logic: Rc<dyn NodeLogic>,
    is_mining: bool,
    faulty: bool,
) -> Rc<Node> {
    let callback = NodeCallback { inner: logic };

    let account_id = rand::random::<u128>();

    let data = NodeData {
        account_id,
        index,
        location,
        clients: RefCell::new(Default::default()),
        statistics: RefCell::new(Default::default()),
    };

    let obj = asim::network::Node::new(bandwidth, data, Box::new(callback));

    get_node_logic(&obj).init(obj.clone());

    // Only non-faulty nodes do something
    // TODO add proper Byzantine behavior
    if !faulty {
        let obj = obj.clone();
        let obj_ptr = obj.clone();
        asim::spawn(async move {
            get_node_logic(&obj).run(obj_ptr, is_mining).await;
        });
    }

    obj
}

impl NodeData {
    pub fn get_statistics(&self) -> RefMut<NodeStatsCollector> {
        self.statistics.borrow_mut()
    }

    pub(crate) fn add_client(&self, client: &Rc<Client>) {
        let account_id = *client.get_account_id();
        let mut clients = self.clients.borrow_mut();
        clients.insert(account_id, Rc::downgrade(client));
    }

    pub fn get_client(&self, account_id: &AccountId) -> Option<Rc<Client>> {
        let clients = self.clients.borrow();
        clients
            .get(account_id)
            .map(|client| client.upgrade().unwrap())
    }

    #[allow(dead_code)]
    pub fn add_transaction(self_ptr: &Rc<Node>, transaction: Rc<Transaction>) {
        get_node_logic(self_ptr).add_transaction(self_ptr, transaction, None);
    }

    pub fn get_location(&self) -> &Location {
        &self.location
    }

    pub fn get_index(&self) -> NodeIndex {
        self.index
    }

    pub fn get_account_id(&self) -> AccountId {
        self.account_id
    }
}
