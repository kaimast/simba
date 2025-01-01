use std::sync::{mpsc, OnceLock};

use crate::config::TimeoutConfig;
use crate::logic::BlockId;
use crate::message::MessageType;
use crate::node::NodeIndex;
use crate::object::ObjectId;
use crate::{ChainMetrics, GlobalStatistics, Location, NetworkMetricType, NodeStatistics};

use asim::time::Time;

#[derive(PartialEq, Eq, Debug)]
pub enum OpRequest {
    ChainMetrics(TimeoutConfig),
    NetworkMetric(NetworkMetricType),
    NodeLocation(NodeIndex),
    NodeStatistics(NodeIndex),
    NodeIdentifier(NodeIndex),
    GlobalStatistics,
    CurrentTime,
}

#[derive(PartialEq, Debug)]
pub enum OpResult {
    ChainMetrics(ChainMetrics),
    NetworkMetric(f64),
    NodeLocation(Location),
    NodeIdentifier(ObjectId),
    CurrentTime(Time),
    NodeStatistics(NodeStatistics),
    GlobalStatistics(GlobalStatistics),
}

#[derive(PartialEq, Eq, Debug)]
pub enum LinkEvent {
    Created { node1: NodeIndex, node2: NodeIndex },
    Active,
    Inactive,
}

#[derive(PartialEq, Eq, Debug)]
pub enum NodeEvent {
    Created(ObjectId),
    StatisticsUpdated,
}

#[derive(PartialEq, Eq, Debug)]
pub enum StatisticsEvent {
    Updated,
}

#[derive(PartialEq, Eq, Debug)]
pub enum BlockEvent {
    Created {
        height: u64,
        parent: BlockId,
        uncles: Vec<BlockId>,
        num_transactions: usize,
    },
}

#[derive(PartialEq, Debug)]
pub enum Event {
    TimeoutElapsed,
    SimulationStopped,
    SimulationDestroyed,
    MessageSent {
        source: ObjectId,
        target: ObjectId,
        msg_type: MessageType,
    },
    OpResult {
        op_id: u64,
        result: OpResult,
    },
    Link {
        identifier: ObjectId,
        event: LinkEvent,
    },
    Node {
        index: NodeIndex,
        event: NodeEvent,
    },
    Block {
        identifier: BlockId,
        event: BlockEvent,
    },
    Statistics(StatisticsEvent),
}

#[derive(PartialEq, Eq, Debug)]
pub enum Command {
    SetTimeout(TimeoutConfig),
    EnableEvents,
    OpRequest { op_id: u64, request: OpRequest },
    Destroy,
}

type EventSender = mpsc::Sender<(Time, Event)>;

thread_local! {
    /// The handler for all non-essential events
    /// This is disabled by default to improve performance
    pub static EVENT_HANDLER: OnceLock<(Time, EventSender)> = OnceLock::default();
}

#[macro_export]
macro_rules! emit_event {
    ($event:expr) => {
        $crate::events::EVENT_HANDLER.with(|h| {
            if let Some((time, handler)) = &h.get() {
                if let Err(err) = handler.send((*time, $event)) {
                    log::warn!("Emitting event failed with error={err:?}. Are we shutting down?");
                }
            }
        })
    };
}
