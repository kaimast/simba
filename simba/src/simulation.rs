use std::cmp::Ordering;
use std::fs::File;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::sync::{Arc, OnceLock, mpsc};

use dashmap::DashMap;

use instant::Instant;

use asim::time::{Duration, START_TIME, Time};

use parking_lot::{Condvar, Mutex};

use crate::clients::Client;
use crate::config::{Connectivity, NetworkConfiguration, ProtocolConfiguration, TimeoutConfig};
use crate::events::{
    BlockEvent, Command, EVENT_HANDLER, Event, LinkEvent, NodeEvent, OpRequest, OpResult,
    StatisticsEvent,
};
use crate::failures::Failures;
use crate::link::create_link;
use crate::link::{Bandwidth, Link};
use crate::logic::{
    BlockId, GlobalLogic, GossipGlobalLogic, NakamotoGlobalLogic, PbftGlobalLogic,
    SnowballGlobalLogic, SpeedTestGlobalLogic,
};
use crate::message::MessageType;
use crate::node::{Node, NodeIndex, create_node};
use crate::object::{Object, ObjectId};
use crate::scene::Scene;
use crate::stats::{GlobalStatistics, NodeStatistics, Statistics};
use crate::{ChainMetrics, Location, NetworkMetricType};

pub type EventCallback<I, T> = Box<dyn Fn(I, T) + Send + Sync>;
pub type StatsEventCallback = Box<dyn Fn(StatisticsEvent) + Send + Sync>;
pub type MessageSentEventCallback =
    Box<dyn Fn(Time, ObjectId, ObjectId, MessageType) + Send + Sync>;

struct PendingOp {
    result: Mutex<Option<OpResult>>,
    cond: Condvar,
}

/// The different states the simulation can be in
#[derive(Debug, PartialEq, Eq)]
enum State {
    /// The user is setting up the simulation
    /// e.g., installing event callbacks
    SettingUp,
    /// The network is being set up simulation is starting
    Starting,
    Running,
    Stopping,
    /// The simulation is stopped but can still be inspected
    Stopped,
    /// The simulation is being destroyed and removed from memory
    Destroyed,
}

pub struct Simulation {
    worker_thread: Mutex<Option<std::thread::JoinHandle<()>>>,
    handler_thread: Mutex<Option<std::thread::JoinHandle<()>>>,
    state: Arc<Mutex<State>>,
    state_cond: Arc<Condvar>,
    command_queue: Arc<Mutex<Vec<Command>>>,
    command_cond: Arc<Condvar>,
    rate_limit: Arc<Mutex<Option<u32>>>,
    rate_limit_cond: Arc<Condvar>,
    pending_operations: Arc<DashMap<u64, Arc<PendingOp>>>,
    next_op_id: AtomicU64,
    msg_sent_event_callback: Arc<OnceLock<MessageSentEventCallback>>,
    block_event_callback: Arc<OnceLock<EventCallback<BlockId, BlockEvent>>>,
    link_event_callback: Arc<OnceLock<EventCallback<ObjectId, LinkEvent>>>,
    node_event_callback: Arc<OnceLock<EventCallback<NodeIndex, NodeEvent>>>,
    stats_event_callback: Arc<OnceLock<StatsEventCallback>>,
}

pub struct SimulationInner {
    scene: Rc<Scene>,
    protocol_config: ProtocolConfiguration,
    network_config: NetworkConfiguration,
    failures: Failures,
    rate_limit: Arc<Mutex<Option<u32>>>,
    rate_limit_cond: Arc<Condvar>,
    asim: Rc<asim::Runtime>,
    statistics: Rc<Statistics>,
    command_queue: Arc<Mutex<Vec<Command>>>,
    command_cond: Arc<Condvar>,
    event_sender: mpsc::Sender<(Time, Event)>,
    state: Arc<Mutex<State>>,
    state_cond: Arc<Condvar>,
}

impl PendingOp {
    fn wait(&self) -> OpResult {
        let mut lock = self.result.lock();

        while lock.is_none() {
            self.cond.wait(&mut lock);
        }

        lock.take().unwrap()
    }
}

impl Simulation {
    pub fn new(
        protocol_config: ProtocolConfiguration,
        network_config: NetworkConfiguration,
        failures: Failures,
        stats_file: Option<String>,
    ) -> anyhow::Result<Self> {
        log::debug!("Setting up simulation");

        let rate_limit = Arc::new(Mutex::new(None));
        let rate_limit_cond = Arc::new(Condvar::new());
        let state = Arc::new(Mutex::new(State::SettingUp));
        let state_cond = Arc::new(Condvar::new());
        let (event_sender, event_receiver) = mpsc::channel();
        let command_queue = Arc::new(Mutex::new(vec![]));
        let command_cond = Arc::new(Condvar::new());
        let pending_operations = Arc::new(DashMap::new());

        let msg_sent_event_callback = Arc::new(OnceLock::new());
        let block_event_callback = Arc::new(OnceLock::new());
        let node_event_callback = Arc::new(OnceLock::new());
        let link_event_callback = Arc::new(OnceLock::new());
        let stats_event_callback = Arc::new(OnceLock::new());

        let stats_file = if let Some(path) = stats_file {
            Some(csv::Writer::from_path(path)?)
        } else {
            None
        };

        let worker_thread = {
            log::debug!("Starting simulation worker thread");

            let rate_limit = rate_limit.clone();
            let rate_limit_cond = rate_limit_cond.clone();
            let state = state.clone();
            let state_cond = state_cond.clone();
            let command_queue = command_queue.clone();
            let command_cond = command_cond.clone();

            std::thread::spawn(move || {
                let inner = SimulationInner::new(
                    protocol_config,
                    network_config,
                    rate_limit,
                    rate_limit_cond,
                    failures,
                    command_queue,
                    command_cond,
                    event_sender,
                    state,
                    state_cond,
                    stats_file,
                );
                inner.run();
            })
        };

        let handler_thread = {
            let pending_operations = pending_operations.clone();

            let msg_sent_event_callback = msg_sent_event_callback.clone();
            let block_event_callback = block_event_callback.clone();
            let link_event_callback = link_event_callback.clone();
            let node_event_callback = node_event_callback.clone();
            let stats_event_callback = stats_event_callback.clone();

            let state = state.clone();
            let state_cond = state_cond.clone();

            std::thread::spawn(move || {
                Self::event_handler(
                    event_receiver,
                    pending_operations,
                    msg_sent_event_callback,
                    block_event_callback,
                    link_event_callback,
                    node_event_callback,
                    stats_event_callback,
                    state,
                    state_cond,
                );
            })
        };

        Ok(Self {
            worker_thread: Mutex::new(Some(worker_thread)),
            handler_thread: Mutex::new(Some(handler_thread)),
            rate_limit,
            rate_limit_cond,
            state,
            state_cond,
            msg_sent_event_callback,
            block_event_callback,
            link_event_callback,
            node_event_callback,
            stats_event_callback,
            command_queue,
            command_cond,
            pending_operations,
            next_op_id: AtomicU64::new(1),
        })
    }

    pub fn stop(&self) {
        {
            *self.state.lock() = State::Stopping;
            self.state_cond.notify_all();
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn event_handler(
        event_receiver: mpsc::Receiver<(Time, Event)>,
        pending_operations: Arc<DashMap<u64, Arc<PendingOp>>>,
        msg_sent_event_callback: Arc<OnceLock<MessageSentEventCallback>>,
        block_event_callback: Arc<OnceLock<EventCallback<BlockId, BlockEvent>>>,
        link_event_callback: Arc<OnceLock<EventCallback<ObjectId, LinkEvent>>>,
        node_event_callback: Arc<OnceLock<EventCallback<NodeIndex, NodeEvent>>>,
        stats_event_callback: Arc<OnceLock<StatsEventCallback>>,
        state: Arc<Mutex<State>>,
        state_cond: Arc<Condvar>,
    ) {
        while let Ok((time, event)) = event_receiver.recv() {
            log::trace!("Got event: {event:?}");

            match event {
                Event::OpResult { op_id, result } => {
                    let (_, hdl) = pending_operations
                        .remove(&op_id)
                        .expect("No such pending operation");
                    *hdl.result.lock() = Some(result);
                    hdl.cond.notify_all();
                }
                Event::SimulationStopped => {}
                Event::SimulationDestroyed => return,
                Event::TimeoutElapsed => {
                    *state.lock() = State::Stopping;
                    state_cond.notify_all();
                }
                Event::Link { identifier, event } => {
                    if let Some(handler) = link_event_callback.get() {
                        handler(identifier, event);
                    }
                }
                Event::Node { index, event } => {
                    if let Some(handler) = node_event_callback.get() {
                        handler(index, event);
                    }
                }
                Event::Block { identifier, event } => {
                    if let Some(handler) = block_event_callback.get() {
                        handler(identifier, event);
                    }
                }
                Event::Statistics(event) => {
                    if let Some(handler) = stats_event_callback.get() {
                        handler(event);
                    }
                }
                Event::MessageSent {
                    source,
                    target,
                    msg_type,
                } => {
                    if let Some(handler) = msg_sent_event_callback.get() {
                        handler(time, source, target, msg_type);
                    }
                }
            }
        }
        log::debug!("Event handler finished");
    }

    /// How long should each simulation step take?
    /// - This is divided by 1000 to support fractional settings
    ///   e.g., 500 is 0.5x speed and 2000 is 2x speed
    /// - Set to 0 to pause simulation
    pub fn set_rate_limit(&self, rate_limit: u32) {
        *self.rate_limit.lock() = Some(rate_limit);
        self.rate_limit_cond.notify_all();
    }

    pub fn remove_rate_limit(&self) {
        *self.rate_limit.lock() = None;
        self.rate_limit_cond.notify_all();
    }

    /// Returns the rate limit (if any) as a factor
    /// E.g., 2.0 for 2x speed
    pub fn get_rate_limit_f64(&self) -> Option<f64> {
        self.rate_limit
            .lock()
            .map(|rate_limit| (rate_limit as f64) / 1000.0)
    }

    pub fn get_rate_limit(&self) -> Option<u32> {
        *self.rate_limit.lock()
    }

    pub fn start(&self) {
        let mut state = self.state.lock();
        assert_eq!(*state, State::SettingUp);
        *state = State::Starting;
        self.state_cond.notify_all();

        // Wait for simulation to start up
        while *state == State::Starting {
            self.state_cond.wait(&mut state);
        }
    }

    /// Runs until the specified timeout
    pub fn run_until(&self, timeout: TimeoutConfig) {
        self.issue_command(Command::SetTimeout(timeout));
        self.start();
        self.wait_for_stop();
    }

    pub fn set_block_event_callback(&self, callback: EventCallback<BlockId, BlockEvent>) {
        self.block_event_callback
            .set(callback)
            .unwrap_or_else(|_| panic!("Event callback already set"));
        self.issue_command(Command::EnableEvents);
    }

    pub fn set_message_sent_event_callback(&self, callback: MessageSentEventCallback) {
        self.msg_sent_event_callback
            .set(callback)
            .unwrap_or_else(|_| panic!("Event callback already set"));
        self.issue_command(Command::EnableEvents);
    }

    pub fn set_node_event_callback(&self, callback: EventCallback<NodeIndex, NodeEvent>) {
        self.node_event_callback
            .set(callback)
            .unwrap_or_else(|_| panic!("Event callback already set"));
        self.issue_command(Command::EnableEvents);
    }

    pub fn set_link_event_callback(&self, callback: EventCallback<ObjectId, LinkEvent>) {
        self.link_event_callback
            .set(callback)
            .unwrap_or_else(|_| panic!("Event callback already set"));
        self.issue_command(Command::EnableEvents);
    }

    pub fn set_stats_event_callback(&self, callback: StatsEventCallback) {
        self.stats_event_callback
            .set(callback)
            .unwrap_or_else(|_| panic!("Event callback already set"));
        self.issue_command(Command::EnableEvents);
    }

    pub fn get_current_time(&self) -> Time {
        let result = self.issue_operation(OpRequest::CurrentTime);

        if let OpResult::CurrentTime(value) = result {
            value
        } else {
            panic!("Got unexpected op result");
        }
    }

    pub fn get_node_location(&self, node_index: NodeIndex) -> Location {
        let result = self.issue_operation(OpRequest::NodeLocation(node_index));

        if let OpResult::NodeLocation(value) = result {
            value
        } else {
            panic!("Got unexpected op result");
        }
    }

    pub fn get_node_identifier(&self, node_index: NodeIndex) -> ObjectId {
        let result = self.issue_operation(OpRequest::NodeIdentifier(node_index));

        if let OpResult::NodeIdentifier(value) = result {
            value
        } else {
            panic!("Got unexpected op result");
        }
    }

    pub fn get_node_statistics(&self, node_index: NodeIndex) -> NodeStatistics {
        let result = self.issue_operation(OpRequest::NodeStatistics(node_index));

        if let OpResult::NodeStatistics(value) = result {
            value
        } else {
            panic!("Got unexpected op result");
        }
    }

    pub fn get_global_statistics(&self) -> GlobalStatistics {
        let result = self.issue_operation(OpRequest::GlobalStatistics);

        if let OpResult::GlobalStatistics(value) = result {
            value
        } else {
            panic!("Got unexpected op result");
        }
    }

    fn issue_operation(&self, request: OpRequest) -> OpResult {
        let op_id = self.next_op_id.fetch_add(1, AtomicOrdering::SeqCst);
        let pending_op = Arc::new(PendingOp {
            result: Mutex::new(None),
            cond: Condvar::default(),
        });
        self.pending_operations.insert(op_id, pending_op.clone());

        let request = Command::OpRequest { op_id, request };
        self.issue_command(request);

        pending_op.wait()
    }

    /// Get metrics about the network as a whole or a node/link in the network
    /// Node this can only be called while the simulation is runnning
    pub fn get_network_metric(&self, nmetric: NetworkMetricType) -> f64 {
        let result = self.issue_operation(OpRequest::NetworkMetric(nmetric));

        if let OpResult::NetworkMetric(value) = result {
            value
        } else {
            panic!("Got unexpected op result");
        }
    }

    pub fn wait_for_stop(&self) {
        let mut state = self.state.lock();

        while *state != State::Stopped {
            self.state_cond.wait(&mut state);
        }
    }

    fn issue_command(&self, command: Command) {
        // We might need to wake up the rate limiter here
        // to process commands
        let _rate_limit = self.rate_limit.lock();
        self.command_queue.lock().push(command);
        self.command_cond.notify_all();
        self.rate_limit_cond.notify_all();
    }

    pub fn get_chain_metrics(&self, timeout: TimeoutConfig) -> ChainMetrics {
        let result = self.issue_operation(OpRequest::ChainMetrics(timeout));

        if let OpResult::ChainMetrics(metrics) = result {
            metrics
        } else {
            panic!("Got unexpected op result");
        }
    }
}

impl SimulationInner {
    #[allow(clippy::too_many_arguments)]
    fn new(
        protocol_config: ProtocolConfiguration,
        network_config: NetworkConfiguration,
        rate_limit: Arc<Mutex<Option<u32>>>,
        rate_limit_cond: Arc<Condvar>,
        failures: Failures,
        command_queue: Arc<Mutex<Vec<Command>>>,
        command_cond: Arc<Condvar>,
        event_sender: mpsc::Sender<(Time, Event)>,
        state: Arc<Mutex<State>>,
        state_cond: Arc<Condvar>,
        stats_file: Option<csv::Writer<File>>,
    ) -> Self {
        let scene = Rc::new(Scene::default());
        let asim = Rc::new(asim::Runtime::default());
        let statistics = Rc::new(Statistics::new(scene.clone(), stats_file));

        Self {
            rate_limit,
            rate_limit_cond,
            statistics,
            asim,
            scene,
            state,
            failures,
            state_cond,
            event_sender,
            command_queue,
            command_cond,
            protocol_config,
            network_config,
        }
    }

    /// Set up the protocol-specific global logic
    fn initialize_logic(&self, failures: &Failures) -> Rc<dyn GlobalLogic> {
        match self.protocol_config {
            ProtocolConfiguration::NakamotoConsensus {
                ref block_generation,
                use_ghost,
                commit_delay,
                max_block_size,
            } => NakamotoGlobalLogic::instantiate(
                block_generation.clone(),
                max_block_size,
                failures.num_correct_nodes(),
                commit_delay,
                use_ghost,
            ),
            ProtocolConfiguration::PracticalBFT {
                max_block_size,
                max_block_interval,
            } => PbftGlobalLogic::instantiate(
                failures.num_correct_nodes(),
                max_block_size,
                max_block_interval,
            ),
            ProtocolConfiguration::SpeedTest { send_speed } => {
                SpeedTestGlobalLogic::instantiate(send_speed)
            }
            ProtocolConfiguration::Gossip {
                block_size,
                retry_delay,
            } => GossipGlobalLogic::instantiate(
                block_size,
                retry_delay,
                failures.num_correct_nodes(),
            ),
            ProtocolConfiguration::Snowball {
                acceptance_threshold,
                sample_size_weighted,
                query_threshold_weighted,
            } => SnowballGlobalLogic::instantiate(
                failures.num_correct_nodes(),
                acceptance_threshold,
                sample_size_weighted,
                query_threshold_weighted,
            ),
        }
    }

    fn generate_node(
        &self,
        global_logic: &dyn GlobalLogic,
        failures: &Failures,
        node_index: NodeIndex,
        location: Location,
        bandwidth: u64,
        mining: bool,
    ) -> Rc<Node> {
        let logic = global_logic.new_node_logic(node_index);
        let bandwidth = Bandwidth::from_megabits_per_second(bandwidth);

        let node = create_node(
            node_index,
            location,
            bandwidth,
            logic.clone(),
            mining,
            failures.is_faulty(&node_index),
        );

        logic.init(node.clone());

        self.scene.add_node(node_index, node.clone());
        node
    }

    fn build_scene(&self, global_logic: &dyn GlobalLogic) {
        let start = Instant::now();

        log::debug!("Generating nodes");

        let mut mining_nodes = vec![];

        match &self.network_config {
            NetworkConfiguration::Random {
                num_mining_nodes,
                num_non_mining_nodes,
                connectivity,
                workload,
                node_bandwidth,
                link_latency,
                link_bandwidth,
            } => {
                for node_index in 0..*num_mining_nodes {
                    let node = self.generate_node(
                        global_logic,
                        &self.failures,
                        node_index,
                        Location::new_random(),
                        *node_bandwidth,
                        true,
                    );
                    mining_nodes.push(node);
                }

                for node_index in *num_mining_nodes..(*num_non_mining_nodes + *num_mining_nodes) {
                    let node = self.generate_node(
                        global_logic,
                        &self.failures,
                        node_index,
                        Location::new_random(),
                        *node_bandwidth,
                        false,
                    );
                    mining_nodes.push(node);
                }

                if !global_logic.is_compatible_with_connectivity(connectivity) {
                    panic!(
                        "Logic {:?} not compatible with connectivity {connectivity:?}",
                        self.protocol_config
                    );
                }

                // TODO move this to a separate method
                log::debug!("Generating network links");
                match connectivity {
                    Connectivity::Full => {
                        for idx1 in 0..mining_nodes.len() {
                            for idx2 in idx1 + 1..mining_nodes.len() {
                                let node1 = &mining_nodes[idx1];
                                let node2 = &mining_nodes[idx2];

                                self.build_connection(node1, node2, *link_bandwidth, *link_latency);
                            }
                        }
                    }
                    Connectivity::Sparse { min_conns_per_node } => {
                        assert!(
                            *min_conns_per_node > 1,
                            "Need at least two connections per node"
                        );

                        let mut conns_per_nodes = vec![0; mining_nodes.len()];
                        let mut known_links = std::collections::HashSet::new();

                        for idx1 in 0..mining_nodes.len() {
                            // Find the closest nodes
                            let mut sorted_nodes = vec![];
                            for idx2 in 0..mining_nodes.len() {
                                if idx1 == idx2 {
                                    continue;
                                }

                                let src = &mining_nodes[idx1];
                                let dst = &mining_nodes[idx2];
                                let distance = src.get_location().distance(dst.get_location());

                                sorted_nodes.push((distance, idx2));
                            }

                            sorted_nodes.sort_by(|(dist_a, _), (dist_b, _)| {
                                dist_a
                                    .partial_cmp(dist_b)
                                    .expect("Failed to compare node locations")
                            });

                            for (_, idx2) in sorted_nodes.drain(..) {
                                // Done?
                                if conns_per_nodes[idx1] >= *min_conns_per_node {
                                    break;
                                }

                                let key = match idx1.cmp(&idx2) {
                                    Ordering::Less => (idx1, idx2),
                                    Ordering::Greater => (idx2, idx1),
                                    Ordering::Equal => continue,
                                };

                                // Don't add the same connection twice
                                if known_links.contains(&key) {
                                    continue;
                                } else {
                                    known_links.insert(key);
                                }

                                let node1 = &mining_nodes[idx1];
                                let node2 = &mining_nodes[idx2];

                                self.build_connection(node1, node2, *link_bandwidth, *link_latency);

                                conns_per_nodes[idx1] += 1;
                                conns_per_nodes[idx2] += 1;
                            }
                        }
                    }
                }

                log::debug!("Generating client workload");
                let client_spacing =
                    workload.client_startup_interval * 1000 * 1000 / (workload.num_clients as u64);

                log::debug!(
                    "Client startup interval is {} seconds; client spacing is {client_spacing} us",
                    workload.client_startup_interval
                );

                for client_idx in 0..workload.num_clients {
                    // pick a random node
                    let node_idx =
                        rand::random::<u32>() % (num_mining_nodes + num_non_mining_nodes);
                    let node = &mining_nodes[node_idx as usize];

                    let start_delay = Duration::from_micros(client_spacing * (client_idx as u64));

                    // place client on same queue as node for better concurrency
                    let transaction_interval = Duration::from_millis(workload.transaction_interval);

                    let client =
                        Rc::new(Client::new(start_delay, transaction_interval, node.clone()));

                    {
                        let client = client.clone();
                        self.asim.spawn(async move { client.run().await });
                    }

                    node.add_client(&client);
                    self.scene.add_client(client.get_identifier(), client);
                }
            }
            NetworkConfiguration::PreDefined {
                clients: client_cfgs,
                nodes: node_cfgs,
                links: link_cfgs,
            } => {
                for (node_index, node_cfg) in node_cfgs.iter().enumerate() {
                    let node = self.generate_node(
                        global_logic,
                        &self.failures,
                        node_index as NodeIndex,
                        node_cfg.location.clone(),
                        node_cfg.bandwidth,
                        true,
                    );
                    mining_nodes.push(node);
                }

                for link_cfg in link_cfgs {
                    let node1 = mining_nodes
                        .get(link_cfg.node1 as usize)
                        .expect("invalid node index specified");
                    let node2 = mining_nodes
                        .get(link_cfg.node2 as usize)
                        .expect("invalid node index specified");

                    self.build_connection(node1, node2, link_cfg.bandwidth, link_cfg.latency);
                }

                for client_cfg in client_cfgs {
                    let node_idx = client_cfg.node as usize;
                    let node = &mining_nodes[node_idx];

                    // Not supported yet
                    let start_delay = Duration::from_micros(0);

                    // place client on same queue as node for better concurrency
                    let transaction_interval =
                        Duration::from_millis(client_cfg.transaction_interval);

                    let client =
                        Rc::new(Client::new(start_delay, transaction_interval, node.clone()));

                    {
                        let client = client.clone();
                        self.asim.spawn(async move { client.run().await });
                    }

                    node.add_client(&client);
                    self.scene.add_client(client.get_identifier(), client);
                }
            }
        }

        let elapsed = (Instant::now() - start).as_secs_f64();

        log::info!(
            "Simulation started with {} nodes, {} clients, and {} network links",
            mining_nodes.len(),
            self.scene.get_clients().len(),
            self.scene.get_links().len(),
        );
        log::debug!("It took {elapsed} seconds to build the network");
    }

    /// Create a connection between two nodes
    fn build_connection(
        &self,
        node1: &Rc<Node>,
        node2: &Rc<Node>,
        bandwidth: Option<u64>,
        latency: u64,
    ) -> Rc<Link> {
        let bandwidth = bandwidth.map(Bandwidth::from_megabits_per_second);
        let latency = Duration::from_millis(latency);

        let link = create_link(node1.clone(), node2.clone(), bandwidth, latency);
        self.scene.add_link(link.get_identifier(), link.clone());

        link
    }

    /// Processes all pending commands. Return true if there were any.
    /// Setting blocking to true will make this function wait until there are commands to process.
    fn process_commands(&self, global_logic: &Rc<dyn GlobalLogic>, blocking: bool) -> bool {
        let cmds = {
            let mut lock = self.command_queue.lock();

            if blocking {
                while lock.is_empty() {
                    log::trace!("Blocking until we receive a command.");
                    self.command_cond.wait(&mut lock);
                }
            }

            std::mem::take(&mut *lock)
        };

        if cmds.is_empty() {
            return false;
        }

        for cmd in cmds {
            log::trace!("Processing command: {cmd:?}");

            match cmd {
                Command::SetTimeout(timeout) => {
                    // Start a special timer thread here
                    let sender = self.event_sender.clone();
                    let statistics = self.statistics.clone();

                    match timeout {
                        TimeoutConfig::Seconds { warmup, runtime } => {
                            self.asim.spawn(async move {
                                let warmup_time = Time::from_seconds(warmup);
                                let now = asim::time::now();
                                if warmup_time > now {
                                    asim::time::sleep(warmup_time - now).await;
                                }

                                // Reset statistics after warmup
                                statistics.reset();

                                let end_time = Time::from_seconds(warmup + runtime);
                                let now = asim::time::now();
                                if end_time > now {
                                    asim::time::sleep(end_time - now).await;
                                }

                                sender
                                    .send((asim::time::now(), Event::TimeoutElapsed))
                                    .unwrap();
                            });
                        }
                        TimeoutConfig::Blocks { warmup, runtime } => {
                            let global_logic = global_logic.clone();
                            self.asim.spawn(async move {
                                global_logic.wait_for_blocks(warmup).await;

                                statistics.reset();

                                global_logic.wait_for_blocks(warmup + runtime).await;

                                sender
                                    .send((asim::time::now(), Event::TimeoutElapsed))
                                    .unwrap();
                            });
                        }
                    }
                }
                Command::EnableEvents => {
                    EVENT_HANDLER.with(|hdl| {
                        if hdl
                            .set((self.asim.get_timer().now(), self.event_sender.clone()))
                            .is_err()
                        {
                            log::warn!("Events were already enabled");
                        }
                    });
                }
                Command::OpRequest { op_id, request } => {
                    let result = match request {
                        OpRequest::NodeLocation(idx) => {
                            let node = self.scene.get_node_by_index(&idx).expect("No such node");
                            OpResult::NodeLocation(node.get_location().clone())
                        }
                        OpRequest::NodeIdentifier(idx) => {
                            let node = self.scene.get_node_by_index(&idx).expect("No such node");
                            OpResult::NodeIdentifier(node.get_identifier())
                        }
                        OpRequest::ChainMetrics(timeout) => {
                            let links = self.scene.get_links();
                            let metrics = global_logic.get_metrics(
                                timeout,
                                &self.scene.get_clients(),
                                &links,
                            );

                            OpResult::ChainMetrics(metrics)
                        }
                        OpRequest::NetworkMetric(nmetric) => {
                            log::trace!("Got network metric request {nmetric:?}");

                            let value = match nmetric {
                                NetworkMetricType::NodeBandwidth(node_idx) => {
                                    let data_point = self
                                        .scene
                                        .get_node_by_index(&node_idx)
                                        .expect("no such node")
                                        .get_statistics()
                                        .get_average_data();

                                    (data_point.incoming_data * 8) as f64
                                }
                                NetworkMetricType::NodePeerCount(node_idx) => {
                                    let count = self
                                        .scene
                                        .get_node_by_index(&node_idx)
                                        .unwrap()
                                        .num_peers();
                                    count as f64
                                }
                                NetworkMetricType::NumMiningNodes => {
                                    let count = self.scene.get_nodes().len();
                                    count as f64
                                }
                                NetworkMetricType::NumNonMiningNodes => {
                                    let count = self.scene.get_nodes().len();
                                    count as f64
                                }
                                NetworkMetricType::NumLinks => {
                                    let count = self.scene.get_links().len();
                                    count as f64
                                }
                            };

                            OpResult::NetworkMetric(value)
                        }
                        OpRequest::NodeStatistics(node_idx) => {
                            let data_point = self
                                .scene
                                .get_node_by_index(&node_idx)
                                .expect("no such node")
                                .get_statistics()
                                .get_latest_data_point();

                            OpResult::NodeStatistics(data_point)
                        }
                        OpRequest::GlobalStatistics => {
                            let data_point = self.statistics.get_latest_data_point();

                            OpResult::GlobalStatistics(data_point)
                        }
                        OpRequest::CurrentTime => {
                            let time = self.asim.get_timer().now();
                            OpResult::CurrentTime(time)
                        }
                    };

                    log::trace!("Sending op result {result:?}");

                    let time = self.asim.get_timer().now();
                    if let Err(err) = self
                        .event_sender
                        .send((time, Event::OpResult { op_id, result }))
                    {
                        log::error!("Failed to send event; has the handler terminated? {err:?}");
                    }
                }
                Command::Destroy => {}
            }
        }

        true
    }

    fn run(&self) {
        {
            let mut state = self.state.lock();
            while *state == State::SettingUp {
                self.state_cond.wait(&mut state);
            }
        }

        log::debug!("Setting up global logic");
        let global_logic = self.initialize_logic(&self.failures);

        // Enables event handling, if requested
        self.process_commands(&global_logic, false);

        log::debug!("Building scene");
        {
            let _ctx = self.asim.with_context();
            self.build_scene(&*global_logic);
        }

        // Run initial tasks until they sleep for timer events
        self.update_stopped();

        // Start statistics collection
        {
            let statistics = self.statistics.clone();

            self.asim.spawn(async move {
                statistics.run(Duration::ZERO).await;
            });
        }

        {
            *self.state.lock() = State::Running;
            self.state_cond.notify_all();
        }

        log::debug!("All set up. Will start regular operation.");
        let mut last_hour = 0;
        let mut last_rate_limit = (START_TIME, Instant::now());

        loop {
            {
                let state = self.state.lock();
                if *state != State::Running {
                    break;
                }
            }

            self.process_commands(&global_logic, false);

            let this_hour = self.asim.get_timer().now().to_hours();
            if this_hour != last_hour {
                log::info!("{this_hour} hour(s) elapsed");
                last_hour = this_hour;
            }

            self.update();

            // Rate limit once ever virtual second
            let mut rate_limit = self.rate_limit.lock();

            // Stay paused
            while let Some(val) = *rate_limit
                && val == 0
            {
                log::debug!("Simulation stopped. Will wait...");
                self.process_commands(&global_logic, false);
                self.update_stopped();
                self.rate_limit_cond.wait(&mut rate_limit);
            }

            if let Some(rate_limit) = *rate_limit {
                let timer = self.asim.get_timer();
                let virtual_elapsed = timer.now() - last_rate_limit.0;
                let real_elapsed = Instant::now() - last_rate_limit.1;
                last_rate_limit = (timer.now(), Instant::now());

                let min_time = std::time::Duration::from_secs_f64(
                    virtual_elapsed.as_seconds_f64() / (rate_limit as f64),
                );

                // Slow down if simulation was too fast
                if real_elapsed < min_time {
                    let sleep_time = min_time - real_elapsed;
                    log::trace!(
                        "Elapsed time was {real_elapsed:?}, but min time is {min_time:?} because simulation advanced by {virtual_elapsed:?}. Sleeping for {}us",
                        (sleep_time.as_micros() as f64) / 1000.0
                    );
                    std::thread::sleep(sleep_time);
                }
            }
        }

        log::debug!("Stopping simulation and disconnecting all nodes");

        // This is mostly done to clean up memory
        // Otherwise there might be cyclic dependencies and stuff is never dropped
        self.asim.stop();
        self.scene.destroy();

        self.event_sender
            .send((self.asim.get_timer().now(), Event::SimulationStopped))
            .unwrap();

        log::trace!("Simulation stopped");

        {
            let mut state = self.state.lock();
            // Don't switch back from destroyed to another state
            if *state != State::Destroyed {
                *state = State::Stopped;
            }
            self.state_cond.notify_all();
        }

        // Keep processing commands until the simulation is destroyed
        loop {
            {
                let state = self.state.lock();
                if *state == State::Destroyed {
                    break;
                }
            }

            self.process_commands(&global_logic, true);
        }

        self.event_sender
            .send((self.asim.get_timer().now(), Event::SimulationDestroyed))
            .unwrap();
    }

    fn update_stopped(&self) {
        // Tasks might wake up other tasks so we loop here
        loop {
            let did_work = self.asim.execute_tasks();
            if !did_work {
                break;
            }
        }
    }

    fn update(&self) {
        // Move time to the next event and execute it
        self.asim.get_timer().advance();

        // Tasks might wake up other tasks so we loop here
        loop {
            let did_work = self.asim.execute_tasks();
            if !did_work {
                break;
            }
        }
    }
}

impl Drop for Simulation {
    fn drop(&mut self) {
        self.stop();

        {
            *self.state.lock() = State::Destroyed;
            self.state_cond.notify_all();
        }

        self.issue_command(Command::Destroy);

        if let Some(hdl) = self.worker_thread.lock().take() {
            hdl.join().unwrap();
        }

        if let Some(hdl) = self.handler_thread.lock().take() {
            hdl.join().unwrap();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_log::test;

    #[test]
    fn full_connectivity() {
        let _ = env_logger::try_init();

        let num_mining_nodes = 11;
        let protocol = ProtocolConfiguration::default();
        let network = NetworkConfiguration::Random {
            num_mining_nodes,
            num_non_mining_nodes: 0,
            connectivity: Connectivity::Full,
            node_bandwidth: 50,
            link_bandwidth: None,
            link_latency: 0,
            workload: Default::default(),
        };

        let failures = Failures::none(num_mining_nodes);
        let simulation = Simulation::new(protocol, network, failures, None).unwrap();
        simulation.start();

        assert_eq!(
            simulation.get_network_metric(NetworkMetricType::NumLinks) as u32,
            num_mining_nodes * (num_mining_nodes - 1) / 2
        );

        assert_eq!(
            simulation.get_network_metric(NetworkMetricType::NodePeerCount(4)) as u32,
            num_mining_nodes - 1
        );
    }

    #[test]
    fn sparse_connectivity() {
        let _ = env_logger::try_init();

        let num_mining_nodes = 10;
        let protocol = ProtocolConfiguration::default();
        let network = NetworkConfiguration::Random {
            num_mining_nodes,
            num_non_mining_nodes: 0,
            connectivity: Connectivity::Sparse {
                min_conns_per_node: 4,
            },
            node_bandwidth: 50,
            link_bandwidth: None,
            link_latency: 0,
            workload: Default::default(),
        };

        let failures = Failures::none(num_mining_nodes);
        let simulation = Simulation::new(protocol, network, failures, None).unwrap();
        simulation.start();

        // Not all nodes should be connected
        assert!(
            (simulation.get_network_metric(NetworkMetricType::NumLinks) as u32)
                < (num_mining_nodes * (num_mining_nodes - 1) / 2)
        );
        assert!(simulation.get_network_metric(NetworkMetricType::NodePeerCount(4)) as u32 >= 4);
    }

    #[test]
    fn two_nodes() {
        let _ = env_logger::try_init();

        let num_mining_nodes = 2;
        let protocol = ProtocolConfiguration::default();
        let network = NetworkConfiguration::Random {
            num_mining_nodes,
            num_non_mining_nodes: 0,
            connectivity: Connectivity::Full,
            node_bandwidth: 50,
            link_bandwidth: None,
            link_latency: 0,
            workload: Default::default(),
        };

        let failures = Failures::none(num_mining_nodes);
        let simulation = Simulation::new(protocol, network, failures, None).unwrap();
        simulation.start();

        assert_eq!(
            simulation.get_network_metric(NetworkMetricType::NumLinks) as u32,
            1
        );
    }
}
