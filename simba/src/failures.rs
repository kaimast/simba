use rand::Rng;

use crate::config::FailureConfig;
use crate::node::NodeIndex;

pub struct Failures {
    num_nodes: u32,
    num_faulty_nodes: u32,
    faulty_nodes: Vec<bool>,
}

impl Failures {
    pub fn new(num_nodes: u32, config: Option<FailureConfig>) -> Self {
        let Some(config) = config else {
            return Self::none(num_nodes);
        };

        let mut num_faulty_nodes = 0;
        let mut faulty_nodes = vec![false; num_nodes as usize];

        //FIXME node0 still has a special role in some protocols
        for idx in 1..num_nodes {
            let faulty = {
                let rand = rand::thread_rng().gen_range(0.0..1.0);
                rand < config.faulty_nodes
            };

            if faulty {
                log::debug!("Node #{idx} is faulty");
                faulty_nodes[idx as usize] = true;
                num_faulty_nodes += 1;
            }
        }

        Self {
            num_nodes,
            num_faulty_nodes,
            faulty_nodes,
        }
    }

    pub fn none(num_nodes: u32) -> Self {
        Self {
            num_nodes,
            num_faulty_nodes: 0,
            faulty_nodes: vec![false; num_nodes as usize],
        }
    }

    pub fn num_correct_nodes(&self) -> u32 {
        self.num_nodes - self.num_faulty_nodes
    }

    pub fn is_faulty(&self, index: &NodeIndex) -> bool {
        let index = *index as usize;
        *self.faulty_nodes.get(index).unwrap()
    }
}
