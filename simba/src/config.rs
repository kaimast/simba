use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::metrics::{ChainMetricType, MetricType};
use crate::node::{Location, NodeIndex};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Connectivity {
    Full,
    Sparse { min_conns_per_node: u32 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workload {
    pub num_clients: u32,
    /// How far should clients be spread out initially (in seconds)
    /// E.g., if startup interval is 1 second and there are 20 clients,
    /// there is a 50ms gap between each client's start
    pub client_startup_interval: u64,
    /// Should clients pause between transaction commit and issuing a new transaction?
    pub transaction_interval: u64,
}

impl Default for Workload {
    fn default() -> Self {
        Self {
            num_clients: 100,
            client_startup_interval: 1,
            transaction_interval: 1000,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum NakamotoBlockGenerationConfig {
    ProofOfWork {
        // Target block interval (in seconds)
        target_block_interval: u64,
        initial_difficulty: Difficulty,
        difficulty_adjustment: DifficultyAdjustment,
    },
    Ouroboros {
        // Slot length (in milliseconds)
        slot_length: u64,
        // Epoch length (in slots)
        epoch_length: u64,
    },
}

impl Default for NakamotoBlockGenerationConfig {
    fn default() -> Self {
        Self::ProofOfWork {
            initial_difficulty: 10_000,
            target_block_interval: 14,
            difficulty_adjustment: Default::default(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ProtocolConfiguration {
    NakamotoConsensus {
        block_generation: NakamotoBlockGenerationConfig,
        #[allow(dead_code)] //TODO
        use_ghost: bool,
        max_block_size: u32,
        /// How many blocks until a transaction is confirmed?
        commit_delay: u64,
    },
    PracticalBFT {
        max_block_size: u32,
        /// Maximum interval between blocks (in milliseconds)
        max_block_interval: u64,
    },
    SpeedTest {
        /// Send speed in Mbit/s
        send_speed: u64,
    },
    Gossip {
        /// When to try fetching data from another peer (in milliseconds)
        retry_delay: u32,
        block_size: u32,
    },
    Snowball {
        /// Number of consecutive rounds for it to be accepted: beta
        acceptance_threshold: u32,
        /// Number of nodes to sample when querying: k/n
        sample_size_weighted: f64,
        /// Number of sampled nodes to form quorum in each epoch: alpha/k
        query_threshold_weighted: f64,
    },
}

impl Default for ProtocolConfiguration {
    fn default() -> Self {
        Self::NakamotoConsensus {
            block_generation: Default::default(),
            use_ghost: false,
            commit_delay: 6,
            max_block_size: 1024 * 1024,
        }
    }
}

impl ProtocolConfiguration {
    pub fn set(&mut self, parameter: &ParameterType, value: ParameterValue) {
        match self {
            &mut Self::NakamotoConsensus {
                ref mut max_block_size,
                ..
            } => match parameter {
                ParameterType::MaxBlockSize => {
                    *max_block_size = value.try_into().unwrap();
                }
                ParameterType::NumMiningNodes
                | ParameterType::NumNonMiningNodes
                | ParameterType::NumClients => {}
                _ => panic!("Parameter not supported"),
            },
            &mut Self::PracticalBFT {
                ref mut max_block_size,
                ..
            } => match parameter {
                ParameterType::MaxBlockSize => {
                    *max_block_size = value.try_into().unwrap();
                }
                ParameterType::NumMiningNodes
                | ParameterType::NumNonMiningNodes
                | ParameterType::NumClients => {}
                _ => panic!("Parameter not supported"),
            },
            &mut Self::Gossip {
                ref mut retry_delay,
                ref mut block_size,
            } => match parameter {
                ParameterType::GossipRetryDelay => {
                    *retry_delay = value.try_into().unwrap();
                }
                ParameterType::BlockSize => {
                    *block_size = value.try_into().unwrap();
                }
                _ => panic!("Parameter not supported"),
            },
            &mut Self::SpeedTest { .. } => unimplemented!(),
            &mut Self::Snowball {
                ref mut acceptance_threshold,
                ..
            } => match parameter {
                ParameterType::MaxBlockSize => unimplemented!(),
                ParameterType::NumMiningNodes
                | ParameterType::NumNonMiningNodes
                | ParameterType::NumClients => {}
                ParameterType::AcceptanceThreshold => {
                    *acceptance_threshold = value.try_into().unwrap();
                }
                _ => panic!("Parameter not supported"),
            },
        }
    }
}

impl NetworkConfiguration {
    pub fn num_nodes(&self) -> u32 {
        match self {
            Self::Random {
                num_mining_nodes,
                num_non_mining_nodes,
                ..
            } => *num_mining_nodes + *num_non_mining_nodes,
            Self::PreDefined { nodes, .. } => nodes.len() as u32,
        }
    }

    pub fn set(&mut self, parameter: &ParameterType, value: ParameterValue) {
        match self {
            &mut Self::Random {
                ref mut num_mining_nodes,
                ref mut num_non_mining_nodes,
                ref mut workload,
                ..
            } => match parameter {
                ParameterType::BlockSize
                | ParameterType::MaxBlockSize
                | ParameterType::GossipRetryDelay
                | ParameterType::AcceptanceThreshold => {}
                ParameterType::NumMiningNodes => {
                    *num_mining_nodes = value
                        .try_into()
                        .expect("Invalid parameter value for \"NumMiningNodes\"");
                }
                ParameterType::NumNonMiningNodes => {
                    *num_non_mining_nodes = value
                        .try_into()
                        .expect("Invalid parameter value for \"NumNonMiningNodes\"");
                }
                ParameterType::NumClients => {
                    workload.num_clients = value
                        .try_into()
                        .expect("Invalid parameter value for \"NumClients\"");
                }
            },
            &mut Self::PreDefined { .. } => match parameter {
                ParameterType::BlockSize
                | ParameterType::MaxBlockSize
                | ParameterType::GossipRetryDelay
                | ParameterType::AcceptanceThreshold => {}
                ParameterType::NumMiningNodes
                | ParameterType::NumNonMiningNodes
                | ParameterType::NumClients => {
                    panic!("Cannot set parameters of pre-defined network");
                }
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    pub location: Location,
    pub bandwidth: u64,
    pub is_mining: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkConfig {
    pub node1: NodeIndex,
    pub node2: NodeIndex,

    pub bandwidth: Option<u64>,
    pub latency: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    pub node: NodeIndex,
    pub transaction_interval: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkConfiguration {
    Random {
        num_mining_nodes: u32,
        num_non_mining_nodes: u32,
        workload: Workload,
        link_latency: u64,
        link_bandwidth: Option<u64>,
        node_bandwidth: u64,
        connectivity: Connectivity,
    },
    PreDefined {
        nodes: Vec<NodeConfig>,
        links: Vec<LinkConfig>,
        clients: Vec<ClientConfig>,
    },
}

impl Default for NetworkConfiguration {
    fn default() -> Self {
        Self::Random {
            num_mining_nodes: 10,
            num_non_mining_nodes: 5,
            workload: Default::default(),
            node_bandwidth: 5 * 1024 * 1024,
            link_bandwidth: None,
            link_latency: 100,
            connectivity: Connectivity::Sparse {
                min_conns_per_node: 5,
            },
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Constraint {
    InRange { min: f64, max: f64 },
    GreaterThan(f64),
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    derive_more::Display,
    derive_more::FromStr,
    Serialize,
    Deserialize,
)]
pub enum ParameterType {
    /// The exact block size in bytes
    /// (only used by gossip)
    BlockSize,
    /// The maximum number of transaction in a single block
    MaxBlockSize,
    /// How many of the nodes are creating new blocks?
    NumMiningNodes,
    NumNonMiningNodes,
    NumClients,
    /// For snowball
    AcceptanceThreshold,
    /// After what time should we try fetching data from another peer
    GossipRetryDelay,
}

impl TryFrom<&str> for ParameterType {
    type Error = derive_more::FromStrError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Self::from_str(s)
    }
}

/// An inclusive interval of integers or floating point numbers
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Interval {
    LinearFloat {
        start: f64,
        end: f64,
        step_size: f64,
    },
    LinearInt {
        start: i64,
        end: i64,
        step_size: i64,
    },
}

pub type Difficulty = u64;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum IncrementalDifficultyAdjustment {
    EthereumHomestead,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum DifficultyAdjustment {
    PeriodBased { window_size: u64 },
    Incremental(IncrementalDifficultyAdjustment),
}

impl Default for DifficultyAdjustment {
    fn default() -> Self {
        Self::Incremental(IncrementalDifficultyAdjustment::EthereumHomestead)
    }
}

#[derive(
    derive_more::Display, Clone, Copy, Debug, PartialOrd, PartialEq, Serialize, Deserialize,
)]
pub enum ParameterValue {
    Float(f64),
    Int(i64),
}

impl TryFrom<&str> for ParameterValue {
    type Error = ();

    fn try_from(s: &str) -> Result<Self, ()> {
        if let Ok(val) = i64::from_str(s) {
            Ok(Self::Int(val))
        } else if let Ok(val) = f64::from_str(s) {
            Ok(Self::Float(val))
        } else {
            Err(())
        }
    }
}

impl TryInto<f64> for ParameterValue {
    type Error = ();

    fn try_into(self) -> Result<f64, ()> {
        if let Self::Float(f) = self {
            Ok(f)
        } else {
            Err(())
        }
    }
}

impl TryInto<usize> for ParameterValue {
    type Error = ();

    fn try_into(self) -> Result<usize, ()> {
        if let Self::Int(i) = self {
            if i >= 0 {
                return Ok(i as usize);
            }
        }

        Err(())
    }
}

impl TryInto<u32> for ParameterValue {
    type Error = ();

    fn try_into(self) -> Result<u32, ()> {
        if let Self::Int(i) = self {
            if i >= 0 {
                return Ok(i as u32);
            }
        }

        Err(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TimeoutConfig {
    Seconds { warmup: u64, runtime: u64 },
    Blocks { warmup: u64, runtime: u64 },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Assert {
    pub metric: MetricType,
    pub constraint: Constraint,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FailureConfig {
    pub faulty_nodes: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExperimentConfiguration {
    pub protocol: String,
    pub network: String,

    pub timeout: TimeoutConfig,

    pub failures: Option<FailureConfig>,

    // We use a vec here to make sure parameters stay in the specified order
    pub data_ranges: Vec<(ParameterType, Interval)>,
    pub metrics: Vec<ChainMetricType>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TestConfiguration {
    pub protocol: String,
    pub network: String,
    pub timeout: TimeoutConfig,
    pub asserts: Vec<Assert>,
}

impl ExperimentConfiguration {
    pub fn num_steps(&self) -> usize {
        let mut result = 1;
        for (_, interval) in self.data_ranges.iter() {
            result *= interval.num_steps()
        }
        result
    }
}

impl Interval {
    pub fn is_valid(&self) -> bool {
        match self {
            Interval::LinearInt {
                start,
                step_size,
                end,
            } => start <= end && *step_size > 0,
            Interval::LinearFloat {
                start,
                step_size,
                end,
            } => start <= end && *step_size > 0.0,
        }
    }

    pub fn num_steps(&self) -> usize {
        match self {
            Self::LinearFloat {
                start,
                end,
                step_size,
            } => {
                let range = end - start;
                (range / step_size) as usize + 1
            }
            Self::LinearInt {
                start,
                end,
                step_size,
            } => {
                let range = end - start;
                (range / step_size) as usize + 1
            }
        }
    }

    pub fn get_step(&self, index: usize) -> Option<ParameterValue> {
        match self {
            Interval::LinearInt {
                start,
                step_size,
                end,
            } => {
                let value = start + (index as i64) * step_size;

                if value <= *end {
                    Some(ParameterValue::Int(value))
                } else {
                    None
                }
            }
            Interval::LinearFloat {
                start,
                step_size,
                end,
            } => {
                let value = start + (index as f64) * step_size;

                if value <= *end {
                    Some(ParameterValue::Float(value))
                } else {
                    None
                }
            }
        }
    }
}
