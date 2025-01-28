use crate::events::{Event, LinkEvent};
use crate::node::{Node, NodeData};
use crate::object::{Object, ObjectId};
use crate::{Message, emit_event};

use std::rc::Rc;

pub use asim::network::{Bandwidth, Latency};

pub type Link = asim::network::Link<Message, NodeData>;

/// Listens for changes to the link and emits events
#[derive(Default)]
struct LinkCallback {}

impl asim::network::LinkCallback<Message, NodeData> for LinkCallback {
    fn message_sent(&self, source: &ObjectId, destination: &ObjectId, message: &Message) {
        emit_event!(Event::MessageSent {
            source: *source,
            target: *destination,
            msg_type: message.get_type(),
        });
    }

    fn link_became_active(&self, link: &Link) {
        emit_event!(Event::Link {
            identifier: link.get_identifier(),
            event: LinkEvent::Active
        });
    }

    fn link_became_inactive(&self, link: &Link) {
        emit_event!(Event::Link {
            identifier: link.get_identifier(),
            event: LinkEvent::Inactive
        });
    }
}

pub(super) fn create_link(
    node1: Rc<Node>,
    node2: Rc<Node>,
    _bandwidth: Option<Bandwidth>,
    latency: Latency,
) -> Rc<Link> {
    Node::connect(node1, node2, latency, Box::new(LinkCallback::default()))
}
