use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::Hash;
use std::rc::Rc;

use tokio::sync::Semaphore;

use asim::sync::mpsc;
use asim::time::{Duration, Time};

use rand::Rng;

use super::SnowballMessage;

use rand::seq::IteratorRandom;

use serde::{Deserialize, Serialize};

use crate::Message;
use crate::logic::{NodeLogic, Transaction};
use crate::node::Node;
use crate::object::{Object, ObjectId};

#[derive(Debug, Serialize, Deserialize, Hash, PartialEq, Eq, Clone, Copy, PartialOrd, Ord)]
pub enum Color {
    Empty = 0,
    Red = 1,
    Blue = 2,
}

impl Color {
    pub fn is_valid(&self) -> bool {
        *self != Color::Empty
    }
    
    pub fn as_str(&self) -> &'static str {
        match self {
            Color::Empty => "Empty",
            Color::Red => "Red",
            Color::Blue => "Blue",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SnowballStats {
    pub rounds_executed: u32,
    pub queries_sent: u32,
    pub responses_received: u32,
    pub color_changes: u32,
    pub decision_time: Option<Time>,
    pub last_round_time: Time,
}

struct NodeState {
    current_candidate: Color,
    decided: bool,
    response_sender: mpsc::Sender<Color>,
    // Enhanced state tracking
    round_number: u32,
    stats: SnowballStats,
    last_heartbeat: Time,
}

pub struct SnowballNodeLogic {
    state: RefCell<NodeState>,
    accept_sem: Rc<Semaphore>,

    // Keep this separate to avoid deadlocks
    response_receiver: RefCell<mpsc::Receiver<Color>>,

    //Parameters
    acceptance_threshold: u32, // beta in paper
    sample_size: u32,          // k in paper
    query_threshold: u32,      // alpha in paper
    
    // Enhanced parameters
    heartbeat_interval: Duration,
    max_rounds: u32,
}

impl NodeState {
    fn handle_message(&mut self, node: &Node, source: ObjectId, message: Message) {
        log::trace!("Got message: {message:?}");

        match message {
            Message::Snowball(SnowballMessage::Query(query)) => {
                self.on_query(node, source, query);
            }
            Message::Snowball(SnowballMessage::QueryResponse(response)) => {
                self.handle_query_response(node, source, response);
            }
            Message::Snowball(SnowballMessage::Heartbeat) => {
                self.handle_heartbeat(node, source);
            }
            Message::Snowball(SnowballMessage::HeartbeatResponse) => {
                // Heartbeat response received, update last heartbeat time
                self.last_heartbeat = asim::time::now();
            }
            _ => log::warn!("Received unexpected message: {message:?}"),
        }
    }

    pub fn on_query(&mut self, node: &Node, source: ObjectId, _candidate: Color) {
        log::trace!("Got query for candidate: {:?}", _candidate);

        if self.current_candidate == Color::Empty {
            self.current_candidate = _candidate;
            log::debug!("Set initial candidate to: {:?}", _candidate);
        }
        
        // Send response with current candidate
        let response = self.current_candidate;
        node.send_to(
            &source,
            Message::Snowball(SnowballMessage::QueryResponse(response)),
        );
        
        self.stats.responses_received += 1;
    }

    fn handle_heartbeat(&mut self, node: &Node, source: ObjectId) {
        // Respond to heartbeat to indicate this node is alive
        node.send_to(
            &source,
            Message::Snowball(SnowballMessage::HeartbeatResponse),
        );
    }

    fn handle_query_response(&mut self, _node: &Node, _source: ObjectId, response: Color) {
        self.response_sender.send(response);
    }

    fn start_next_sample(
        &mut self,
        sample_size: u32, // k in paper
        node: &Node,
    ) -> Result<(), String> {
        log::trace!("Running SnowballNodeState:start_next_sample()");
        
        if sample_size == 0 {
            return Err("Sample size cannot be zero".to_string());
        }
        
        // self.current_candidate is col in paper, not using any col_0 for initial value
        let nodes = node.get_peers(); //get all nodes in network
        
        if nodes.is_empty() {
            return Err("No peers available for sampling".to_string());
        }
        
        let mut rng = &mut rand::rng();
        
        if sample_size as usize > nodes.len() {
            log::warn!("Sample size {} exceeds available peers {}, using all peers", sample_size, nodes.len());
        }
        
        let actual_sample_size = std::cmp::min(sample_size as usize, nodes.len());
        let sampled_nodes = nodes
            .into_iter()
            .choose_multiple(&mut rng, actual_sample_size);

        for peer_id in sampled_nodes {
            node.send_to(
                &peer_id,
                Message::Snowball(SnowballMessage::Query(self.current_candidate)),
            );
            self.stats.queries_sent += 1;
        }
        
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_sample_result(
        &mut self,
        accept_sem: &Semaphore,
        results: Vec<Color>,
        acceptance_threshold: u32,                      // beta in paper
        query_threshold: u32,                           // alpha in paper
        mut last_chosen_candidate: Color,               // lastcol in paper
        candidate_preference: &mut HashMap<Color, u32>, // d[] in paper
        acceptance_count: &mut u32,                     // cnt in paper
    ) -> Color {
        log::trace!("Running SnowballNodeState:handle_sample_results()");
        
        if results.is_empty() {
            log::warn!("No results to process");
            return last_chosen_candidate;
        }
        
        let mut frequency = HashMap::new(); // P in paper
        log::trace!("Processing {} responses", results.len());
        
        // Count how many QueryResponse contains a particular candidate
        for color in results {
            if color.is_valid() {
                *frequency.entry(color).or_insert(0) += 1;
            }
        }

        let mut majority: bool = false;

        for (candidate, f) in frequency {
            //col' in paper
            if f > query_threshold {
                majority = true;
                //d[col']++
                *candidate_preference.entry(candidate).or_insert(0) += 1;
                
                // d[col] < d[col']
                if *candidate_preference
                    .get(&self.current_candidate)
                    .unwrap_or(&0)
                    < candidate_preference[&candidate]
                    && self.current_candidate != candidate {
                        self.current_candidate = candidate;
                        self.stats.color_changes += 1;
                        log::debug!("Changed candidate to: {:?}", candidate);
                    }

                if candidate == last_chosen_candidate {
                    *acceptance_count = *acceptance_count + 1; //cnt++
                } else {
                    // col' != lastcol: lastcol = col'; cnt = 1
                    *acceptance_count = 1;
                    last_chosen_candidate = candidate;
                }

                if *acceptance_count >= acceptance_threshold {
                    self.decided = true;
                    self.stats.decision_time = Some(asim::time::now());
                    accept_sem.add_permits(1);
                    log::info!("Decided on color: {:?} after {} rounds", candidate, self.round_number);
                }
            }
        }
        
        if !majority {
            *acceptance_count = 0;
        }
        
        last_chosen_candidate
    }
    
    fn send_heartbeat(&self, node: &Node) {
        let peers = node.get_peers();
        for peer_id in peers {
            node.send_to(
                &peer_id,
                Message::Snowball(SnowballMessage::Heartbeat),
            );
        }
    }
}

#[async_trait::async_trait(?Send)]
impl NodeLogic for SnowballNodeLogic {
    fn init(&self, _node: Rc<Node>) {}

    async fn run(&self, node: Rc<Node>, _is_mining: bool) {
        log::trace!("Running SnowballNodeLogic:run()");
        let mut candidate_preference = HashMap::new(); // d[] in paper
        let mut last_chosen_candidate = self.state.borrow_mut().current_candidate; // lastcol in paper
        let mut acceptance_count = 0; // cnt in paper
        let mut last_heartbeat = asim::time::now();

        loop {
            log::trace!("Next round of snowball");

            {
                let mut state = self.state.borrow_mut();

                if state.current_candidate == Color::Empty {
                    log::error!("No initial candidate set - this should not happen");
                    return;
                }

                if state.decided {
                    let id = node.get_identifier();
                    log::info!("Node {} decided on color: {:?}", id, state.current_candidate);
                    return;
                }

                // Check if we've exceeded max rounds
                if state.round_number >= self.max_rounds {
                    log::warn!("Exceeded maximum rounds ({}) without decision", self.max_rounds);
                    return;
                }

                // Send heartbeat periodically
                let now = asim::time::now();
                if now - last_heartbeat >= self.heartbeat_interval {
                    state.send_heartbeat(&node);
                    last_heartbeat = now;
                }

                // Start the next sample
                if let Err(e) = state.start_next_sample(self.sample_size, &node) {
                    log::error!("Failed to start sample: {}", e);
                    return;
                }
                
                state.round_number += 1;
                state.stats.rounds_executed = state.round_number;
                state.stats.last_round_time = now;
            }

            // Collect responses with timeout
            let mut responses = vec![];
            let mut attempts = 0;
            const MAX_ATTEMPTS: u32 = 3;
            
            while responses.len() < self.sample_size as usize && attempts < MAX_ATTEMPTS {
                let received = self.response_receiver.borrow_mut().recv().await;
                let received_len = received.len();
                responses.extend(received);
                log::trace!(
                    "Got {} responses, total: {} out of {}",
                    received_len,
                    responses.len(),
                    self.sample_size
                );
                attempts += 1;
            }

            if responses.len() < self.sample_size as usize {
                log::warn!("Only received {} responses out of {} expected", responses.len(), self.sample_size);
            }

            {
                let mut state = self.state.borrow_mut();
                last_chosen_candidate = state.handle_sample_result(
                    &self.accept_sem,
                    responses,
                    self.acceptance_threshold,
                    self.query_threshold,
                    last_chosen_candidate,
                    &mut candidate_preference,
                    &mut acceptance_count,
                );
            }
        }
    }

    fn add_transaction(
        &self,
        _node: &Node,
        _transaction: Rc<Transaction>,
        _source: Option<ObjectId>,
    ) {
        // TODO: Implement transaction handling for Snowball
        // This would involve creating a new consensus instance for the transaction
        log::debug!("Transaction received but not yet implemented in Snowball");
    }

    fn handle_message(&self, node: &Rc<Node>, source: ObjectId, message: Message) {
        let mut state = self.state.borrow_mut();
        state.handle_message(node, source, message);
    }
}

impl SnowballNodeLogic {
        pub(super) fn new(
        acceptance_threshold: u32,
        sample_size: u32,
        query_threshold: u32,
        accept_sem: Rc<Semaphore>,
        heartbeat_interval: Duration,
        max_rounds: u32,
    ) -> Self {
        let (response_sender, response_receiver) = mpsc::channel();

        log::debug!("Created enhanced SnowballNodeLogic");

        // Generate a random number between 0 and 3
        let mut rng = rand::rng();
        let random_number: u8 = rng.random_range(0..=2);
        let current_candidate = match random_number {
            1 => Color::Red,
            2 => Color::Blue,
            _ => Color::Red,
        };

        let now = asim::time::START_TIME;
        let stats = SnowballStats {
            rounds_executed: 0,
            queries_sent: 0,
            responses_received: 0,
            color_changes: 0,
            decision_time: None,
            last_round_time: now,
        };

        let state = RefCell::new(NodeState {
            current_candidate,
            response_sender,
            decided: false,
            round_number: 0,
            stats,
            last_heartbeat: now,
        });

        Self {
            state,
            accept_sem,
            acceptance_threshold,
            sample_size,
            query_threshold,
            response_receiver: RefCell::new(response_receiver),
            heartbeat_interval,
            max_rounds,
        }
    }

    // Get statistics for monitoring and debugging
    #[cfg(test)]
    pub fn get_stats(&self) -> SnowballStats {
        self.state.borrow().stats.clone()
    }

    // Check if the node has decided
    #[cfg(test)]
     pub fn is_decided(&self) -> bool {
        self.state.borrow().decided
    }

    // Get the current candidate
    #[cfg(test)]
     pub fn get_current_candidate(&self) -> Color {
        self.state.borrow().current_candidate
    }
}
