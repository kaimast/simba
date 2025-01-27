use std::cmp::Ordering;
use std::rc::Rc;

use asim::time::{Duration, START_TIME, Time};

use crate::config::{
    Difficulty, DifficultyAdjustment, IncrementalDifficultyAdjustment,
    NakamotoBlockGenerationConfig,
};
use crate::ledger::{DiffTarget, MAX_DIFF_TARGET, NakamotoBlock};
use crate::logic::Block;
use crate::node::NodeIndex;

use rand::RngCore;

pub trait BlockGenerator {
    fn should_create_block(&mut self, idx: NodeIndex) -> bool;
    fn get_difficulty(&self) -> Difficulty;
    fn get_resolution(&self) -> Duration;
    fn update_chain_head(
        &mut self,
        new_block: &Rc<NakamotoBlock>,
        parent_block: Option<&Rc<NakamotoBlock>>,
    );
}

struct ProofOfWork {
    target_block_interval: Time,
    difficulty_adjustment: DifficultyAdjustment,
    difficulty: Difficulty,
    difficulty_target: DiffTarget,
}

/// Simplistic implementation of Ouroboros
/// It currently does not have a proper leader schedule,
/// but just rotates block generators
struct Ouroboros {
    slot_length: Duration,
    num_nodes: u32,
    next_block_generator: NodeIndex,
}

impl BlockGenerator for ProofOfWork {
    fn should_create_block(&mut self, _idx: NodeIndex) -> bool {
        // TODO should be a function of the node's compute power
        let mut rng = rand::rng();

        let mut value = DiffTarget([0, 0, 0, 0]);
        for idx in 0..4 {
            value.0[idx] = rng.next_u64();
        }

        value < self.difficulty_target
    }

    fn get_difficulty(&self) -> Difficulty {
        self.difficulty
    }

    fn get_resolution(&self) -> Duration {
        // A somewhat arbitrary interval in which we simulate
        // an attempt to mine a block
        Duration::from_millis(100)
    }

    fn update_chain_head(
        &mut self,
        new_block: &Rc<NakamotoBlock>,
        parent_block: Option<&Rc<NakamotoBlock>>,
    ) {
        let elapsed = if let Some(parent) = parent_block {
            new_block.get_creation_time() - parent.get_creation_time()
        } else {
            new_block.get_creation_time() - START_TIME
        };

        let chain_length = new_block.get_height();

        let new_difficulty = match self.difficulty_adjustment {
            DifficultyAdjustment::PeriodBased { window_size } => {
                if chain_length % window_size == 0 {
                    log::debug!("Recomputing difficulty target");
                    //TODO re32)compute
                    self.difficulty
                } else {
                    // No recompute necessary
                    self.difficulty
                }
            }
            DifficultyAdjustment::Incremental(itype) => {
                match itype {
                    IncrementalDifficultyAdjustment::EthereumHomestead => {
                        let parent_diff = new_block.get_difficulty();
                        let elapsed = elapsed.to_seconds() as i128;

                        // Round down to a multiple of ten seconds
                        // For ETH (interval=14) this should work like the standard protocol
                        // but it allows us to use this mechanisms for other target intervals as well
                        let target_block_interval =
                            ((self.target_block_interval.to_seconds() / 10) * 10) as i128;

                        // Ignore difficulty bomb
                        let change = ((parent_diff / 2048) as i128)
                            * (1 - elapsed / target_block_interval).max(-99);

                        match change.cmp(&0) {
                            Ordering::Less => {
                                let change = (-change) as Difficulty;

                                match parent_diff.checked_sub(change) {
                                    Some(val) => val,
                                    None => {
                                        log::warn!("Reached minimum difficulty");
                                        0
                                    }
                                }
                            }
                            Ordering::Greater => {
                                let change = change as Difficulty;
                                match parent_diff.checked_add(change) {
                                    Some(Difficulty::MAX) | None => {
                                        log::warn!("Reached maximum difficulty");
                                        Difficulty::MAX - 1
                                    }
                                    Some(val) => val,
                                }
                            }
                            Ordering::Equal => *parent_diff,
                        }
                    }
                }
            }
        };

        if new_difficulty != self.difficulty {
            log::trace!(
                "Block interval was {}s. Difficulty changed from {} to {new_difficulty}",
                elapsed.as_seconds_f64(),
                self.difficulty
            );
            self.difficulty = new_difficulty;
            self.difficulty_target = MAX_DIFF_TARGET / DiffTarget([self.difficulty, 0, 0, 0])
        }
    }
}

impl BlockGenerator for Ouroboros {
    fn should_create_block(&mut self, idx: NodeIndex) -> bool {
        let result = idx == self.next_block_generator;
        self.next_block_generator = (self.next_block_generator + 1) % self.num_nodes;
        result
    }

    fn get_difficulty(&self) -> Difficulty {
        0
    }

    fn get_resolution(&self) -> Duration {
        self.slot_length
    }

    fn update_chain_head(
        &mut self,
        _new_block: &Rc<NakamotoBlock>,
        _parent_block: Option<&Rc<NakamotoBlock>>,
    ) {
    }
}

pub fn make_block_generator(
    num_nodes: u32,
    config: &NakamotoBlockGenerationConfig,
) -> Box<dyn BlockGenerator> {
    match config {
        NakamotoBlockGenerationConfig::ProofOfWork {
            difficulty_adjustment,
            target_block_interval,
            initial_difficulty,
        } => {
            let diff_target = MAX_DIFF_TARGET / DiffTarget([*initial_difficulty, 0, 0, 0]);

            Box::new(ProofOfWork {
                difficulty: *initial_difficulty,
                difficulty_target: diff_target,
                difficulty_adjustment: *difficulty_adjustment,
                target_block_interval: Time::from_seconds(*target_block_interval),
            })
        }
        NakamotoBlockGenerationConfig::Ouroboros {
            slot_length,
            epoch_length: _,
        } => Box::new(Ouroboros {
            num_nodes,
            next_block_generator: 0,
            slot_length: Duration::from_millis(*slot_length),
        }),
    }
}
