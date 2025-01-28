use crate::clients::Client;
use crate::events::{Event, LinkEvent, NodeEvent};
use crate::link::Link;
use crate::node::{Node, NodeIndex};
use crate::object::{Object, ObjectId, ObjectMap};
use crate::{RcCell, emit_event};

use std::cell::Ref;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

pub struct Scene {
    clients: RefCell<Vec<Rc<Client>>>,
    objects: RcCell<ObjectMap>,
    links: RefCell<BTreeMap<ObjectId, Rc<Link>>>,
    nodes: RefCell<BTreeMap<NodeIndex, Rc<Node>>>,
}

impl Default for Scene {
    fn default() -> Self {
        let objects = Rc::new(RefCell::new(ObjectMap::default()));

        Self {
            clients: RefCell::new(Default::default()),
            objects,
            links: RefCell::new(Default::default()),
            nodes: RefCell::new(Default::default()),
        }
    }
}

impl Scene {
    pub(crate) fn add_node(&self, node_idx: NodeIndex, node: Rc<Node>) {
        emit_event!(Event::Node {
            index: node_idx,
            event: NodeEvent::Created(node.get_identifier()),
        });

        self.objects
            .borrow_mut()
            .insert(node.get_identifier(), node.clone());
        self.nodes.borrow_mut().insert(node_idx, node);
    }

    pub(crate) fn add_link(&self, link_id: ObjectId, link: Rc<Link>) {
        let (node1, node2) = {
            let (node1, node2) = link.get_nodes();
            (node1.get_index(), node2.get_index())
        };

        self.objects.borrow_mut().insert(link_id, link.clone());
        self.links.borrow_mut().insert(link_id, link);

        emit_event!(Event::Link {
            identifier: link_id,
            event: LinkEvent::Created { node1, node2 },
        });
    }

    pub(crate) fn add_client(&self, client_id: ObjectId, client: Rc<Client>) {
        self.objects.borrow_mut().insert(client_id, client.clone());
        self.clients.borrow_mut().push(client);
    }

    pub fn get_links(&self) -> Ref<BTreeMap<ObjectId, Rc<Link>>> {
        self.links.borrow()
    }

    pub fn get_nodes(&self) -> Ref<BTreeMap<NodeIndex, Rc<Node>>> {
        self.nodes.borrow()
    }

    pub fn get_clients(&self) -> Ref<Vec<Rc<Client>>> {
        self.clients.borrow()
    }

    pub fn get_node_by_index(&self, idx: &NodeIndex) -> Option<Rc<Node>> {
        self.nodes.borrow().get(idx).cloned()
    }

    #[allow(dead_code)]
    pub fn get_node(&self, id: &ObjectId) -> Option<Rc<Node>> {
        for (_, node) in self.nodes.borrow().iter() {
            if node.get_identifier() == *id {
                return Some(node.clone());
            }
        }

        None
    }

    pub fn destroy(&self) {
        for (_, obj) in self.objects.borrow_mut().drain() {
            obj.destroy();
        }
    }
}
