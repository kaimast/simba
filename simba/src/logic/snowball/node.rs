use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::Hash;
use std::rc::Rc;

use tokio::sync::Semaphore;

use asim::sync::mpsc;

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

struct NodeState {
    current_candidate: Color,
    decided: bool,
    response_sender: mpsc::Sender<Color>,
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
            _ => log::warn!("Received unexpected message: {message:?}"),
        }
    }

    pub fn on_query(&mut self, node: &Node, source: ObjectId, candidate: Color) {
        log::trace!("Got query");

        if self.current_candidate == Color::Empty {
            self.current_candidate = candidate;
        }
        node.send_to(
            &source,
            Message::Snowball(SnowballMessage::QueryResponse(self.current_candidate)),
        );
    }

    fn handle_query_response(&mut self, _node: &Node, _source: ObjectId, response: Color) {
        self.response_sender.send(response);
    }

    fn start_next_sample(
        &mut self,
        sample_size: u32, // k in paper
        node: &Node,
    ) {
        log::trace!("Running SnowballNodeState:start_next_sample()");
        // self.current_candidate is col in paper, not using any col_0 for initial value
        let nodes = node.get_peers(); //get all nodes in network
        let mut rng = &mut rand::rng();
        assert!(sample_size as usize <= nodes.len());
        let sampled_nodes = nodes
            .into_iter()
            .choose_multiple(&mut rng, sample_size as usize);

        for peer_id in sampled_nodes {
            node.send_to(
                &peer_id,
                Message::Snowball(SnowballMessage::Query(self.current_candidate)),
            );
        }
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
        let mut frequency = HashMap::new(); // P in paper
        log::trace!("{candidate_preference:?}");
        // Count how many QueryResponse contains a particular candidate
        for color in results {
            *frequency.entry(color).or_insert(0) += 1;
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
                {
                    self.current_candidate = candidate;
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
                    accept_sem.add_permits(1);
                    log::trace!("Decided on color");
                }
            }
        }
        if !majority {
            *acceptance_count = 0;
        }
        last_chosen_candidate
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

        loop {
            log::trace!("Next round of snowball");

            {
                let mut state = self.state.borrow_mut();

                if state.current_candidate == Color::Empty {
                    unimplemented!();
                }

                if state.decided {
                    let id = node.get_identifier();
                    match state.current_candidate {
                        Color::Empty => log::trace!("No color decided on {id}"),
                        Color::Red => log::trace!("Red decided on {id}"),
                        Color::Blue => log::trace!("Blue decided on {id}"),
                    }
                    return;
                }

                state.start_next_sample(self.sample_size, &node);
            }

            let mut responses = vec![];
            while responses.len() < self.sample_size as usize {
                let mut r = self.response_receiver.borrow_mut().recv().await;
                responses.append(&mut r);
                log::trace!(
                    "Got response {} out of {}",
                    responses.len(),
                    self.sample_size
                );
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
        //do nothing for now
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
    ) -> Self {
        let (response_sender, response_receiver) = mpsc::channel();

        log::debug!("Created SnowballNodeLogic");

        // generate a random number between 0 and 3
        let mut rng = rand::rng();
        let random_number: u8 = rng.random_range(0..=2);
        let current_candidate = match random_number {
            1 => Color::Red,
            2 => Color::Blue,
            _ => Color::Red,
        };

        let state = RefCell::new(NodeState {
            current_candidate,
            response_sender,
            decided: false,
        });

        Self {
            state,
            accept_sem,
            acceptance_threshold,
            sample_size,
            query_threshold,
            response_receiver: RefCell::new(response_receiver),
        }
    }
}
