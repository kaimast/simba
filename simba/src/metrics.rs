use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::node::NodeIndex;

use asim::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq, derive_more::Display, Serialize, Deserialize)]
pub enum MetricType {
    Chain(ChainMetricType),
    Network(NetworkMetricType),
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    derive_more::Display,
    derive_more::FromStr,
    Serialize,
    Deserialize,
)]
pub enum ChainMetricType {
    /// The average time between blocks (in seconds)
    BlockInterval,
    /// How many blocks are accepted/finalized by the network?
    WinRate,
    /// How many blocks are rejected/abandoned by the network?
    OrphanRate,
    /// Throughput (in txns per second)
    Throughput,
    /// Average Latency (in milliseconds)
    /// Captures the time from a transaction being issued until it is accepted by the network
    Latency,
    /// How long does it take for a block to have reached all (correct) nodes in the network?
    BlockPropagationDelay,
    BlockSize,
    NumNetworkMessages,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetworkMetricType {
    /// The bandwidth used by this node in bits/s
    NodeBandwidth(NodeIndex),
    /// How many other nodes a node is connected to
    NodePeerCount(NodeIndex),
    /// How many nodes are there in total?
    NumMiningNodes,
    NumNonMiningNodes,
    /// How many links are there in total?
    NumLinks,
}

impl fmt::Display for NetworkMetricType {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::NodeBandwidth(idx) => write!(fmt, "Bandwidth of Node #{idx}"),
            Self::NodePeerCount(idx) => write!(fmt, "Peer Count of Node #{idx}"),
            Self::NumMiningNodes => write!(fmt, "Number of Mining Nodes"),
            Self::NumNonMiningNodes => write!(fmt, "Number of Non-Mining Nodes"),
            Self::NumLinks => write!(fmt, "Number of Network Links"),
        }
    }
}

/// Metrics about the blockchain with respect to a specified start and end type
#[derive(Default, Debug, PartialEq, Clone)]
pub struct ChainMetrics {
    /// Total blocks mined (includes blocks before and after the measurement interval)
    pub total_blocks_mined: u64,
    /// Total block accepted ( excludes blocks that are orphaned)
    pub total_blocks_accepted: u64,
    /// The total height of longest chain (includes blocks before the measurement interval)
    pub longest_chain_length: u64,
    /// Average time between blocks (in seconds)
    pub avg_block_interval: f64,
    /// Total number of transactions (excluding forks)
    pub num_transactions: u64,
    pub avg_latency: f64,           //TODO generate a histogram here
    pub avg_block_propagation: f64, //TODO generate a histogram here
    //TODO    pub leader_distribution: u64,
    /// Elapsed time
    pub elapsed: Duration,
    pub avg_block_size: f64,
    pub num_network_messages: u64,
}

impl ChainMetrics {
    pub fn get_win_rate(&self) -> f64 {
        (self.longest_chain_length as f64) / self.elapsed.as_seconds_f64()
    }

    pub fn get_block_rate(&self) -> f64 {
        (self.total_blocks_mined as f64) / self.elapsed.as_seconds_f64()
    }

    pub fn get_orphan_rate(&self) -> f64 {
        assert!(self.total_blocks_mined >= self.total_blocks_accepted);
        ((self.total_blocks_mined - self.total_blocks_accepted) as f64)
            / self.elapsed.as_seconds_f64()
    }

    pub fn get_throughput(&self) -> f64 {
        (self.num_transactions as f64) / self.elapsed.as_seconds_f64()
    }

    pub fn get(&self, metric: &ChainMetricType) -> f64 {
        match metric {
            ChainMetricType::Throughput => self.get_throughput(),
            ChainMetricType::WinRate => self.get_block_rate(),
            ChainMetricType::BlockSize => self.avg_block_size,
            ChainMetricType::OrphanRate => self.get_orphan_rate(),
            ChainMetricType::BlockInterval => self.avg_block_interval,
            ChainMetricType::BlockPropagationDelay => self.avg_block_propagation,
            ChainMetricType::Latency => self.avg_latency,
            ChainMetricType::NumNetworkMessages => self.num_network_messages as f64,
        }
    }
}

impl TryFrom<&str> for ChainMetricType {
    type Error = derive_more::FromStrError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Self::from_str(s)
    }
}
