#![feature(trait_alias)]

use generic_array::{GenericArray, typenum};

mod node;
use node::{FrozenNode, Node};

pub type Hash = GenericArray<u8, typenum::U32>;
pub trait Value = Send;

const BITS_PER_NODE: usize = 4;
const HASH_LENGTH: usize = 256;
const NUM_STEPS: usize = HASH_LENGTH / BITS_PER_NODE;

pub struct CowTree<V: Value> {
    root: Node<V>,
}

impl<V: Value> Default for CowTree<V> {
    fn default() -> Self {
        Self {
            root: Node::make_branch(),
        }
    }
}

impl<V: Value> CowTree<V> {
    pub fn freeze(self) -> FrozenCowTree<V> {
        let root = self.root.into_frozen();
        FrozenCowTree { root }
    }

    fn get_index(key: &Hash, step: usize) -> u8 {
        let byte: u8 = key[step / 2];

        if step % 2 == 0 {
            // Get lower 4 bits
            byte & 0x0F
        } else {
            // Get upper 4 bits
            (byte & 0xF0) >> 4
        }
    }

    pub fn insert(&mut self, key: &Hash, value: V) {
        let mut nodes: Vec<(u8, Box<Node<V>>)> = vec![];
        let mut step = 0;

        while step < NUM_STEPS - 2 {
            let idx = Self::get_index(key, step);

            let child = if step == 0 {
                self.root.take_child(idx)
            } else {
                nodes[step - 1].1.take_child(idx)
            };

            if let Some(child) = child {
                nodes.push((idx, child));
                step += 1;
            } else {
                break;
            }
        }

        assert_eq!(nodes.len(), step);

        if !nodes.is_empty() && !nodes[step].1.is_branch() {
            let (idx, node) = nodes.pop().unwrap();
            let branch = node.into_branch();
            nodes.push((idx, Box::new(branch)));
        }

        while step < NUM_STEPS - 1 {
            let idx = Self::get_index(key, step);
            let child_idx = Self::get_index(key, step + 1);
            let ext = Node::make_extension(child_idx);

            nodes.push((idx, Box::new(ext)));
            step += 1;
        }

        let idx = Self::get_index(key, step);
        nodes.push((idx, Box::new(Node::make_leaf(value))));

        assert_eq!(nodes.len(), NUM_STEPS);
        let mut last_node = None;

        while let Some((idx, mut node)) = nodes.pop() {
            if let Some((child_idx, child_node)) = last_node.take() {
                node.set_child(child_idx, child_node);
            }

            last_node = Some((idx, node));
        }

        let (idx, node) = last_node.unwrap();
        self.root.set_child(idx, node);
    }

    pub fn get(&self, key: &Hash) -> Option<&V> {
        let mut current_node = &self.root;

        for step in 0..NUM_STEPS {
            let idx = Self::get_index(key, step);

            if let Some(frozen) = current_node.get_reference() {
                return Self::get_frozen(key, step, frozen);
            } else if let Some(child) = current_node.get_child(idx) {
                current_node = child;
            } else {
                return None;
            }
        }

        Some(current_node.get_value())
    }

    fn get_frozen<'a>(key: &Hash, start_step: usize, start: &'a FrozenNode<V>) -> Option<&'a V> {
        let mut current_node = start;

        for step in start_step..NUM_STEPS {
            let idx = Self::get_index(key, step);

            if let Some(child) = current_node.get_child(idx) {
                current_node = child;
            } else {
                return None;
            }
        }

        Some(current_node.get_value())
    }
}

pub struct FrozenCowTree<V: Value> {
    root: FrozenNode<V>,
}

impl<V: Value> FrozenCowTree<V> {
    pub fn get(&self, key: &Hash) -> Option<&V> {
        let mut current_node = &self.root;
        for step in 0..NUM_STEPS {
            let idx = CowTree::<V>::get_index(key, step);
            if let Some(child) = current_node.get_child(idx) {
                current_node = child;
            } else {
                return None;
            }
        }

        Some(current_node.get_value())
    }

    pub fn deep_clone(&self) -> CowTree<V> {
        let mut new_root = Node::make_branch();

        // Always duplicate the first level and then make references for all others
        if let FrozenNode::Branch { children } = &self.root {
            for (pos, child) in children.iter().enumerate() {
                if let Some(child) = child {
                    let frozen = FrozenNode::to_reference(child.clone());
                    new_root.set_child(pos as u8, Box::new(Node::make_reference(frozen)));
                }
            }
        } else {
            panic!("Invalid state");
        }

        CowTree { root: new_root }
    }
}

#[cfg(test)]
mod test {
    use super::CowTree;
    use sha3::{Digest, Sha3_256};

    #[test]
    fn insert_get() {
        let mut tree = CowTree::default();

        let mut hasher = Sha3_256::new();
        hasher.update(b"this is some key we are hashing");

        let key = hasher.finalize();
        let value = "this is a test".to_string();

        tree.insert(&key, value.clone());

        assert_eq!(tree.get(&key), Some(&value));
    }

    #[test]
    fn freeze() {
        let mut tree1 = CowTree::default();

        let key1 = {
            let mut hasher = Sha3_256::new();
            hasher.update(b"this is some key we are hashing");
            hasher.finalize()
        };

        let value1 = "this is a test".to_string();

        tree1.insert(&key1, value1.clone());

        let frozen = tree1.freeze();

        let mut tree2 = frozen.deep_clone();

        let key2 = {
            let mut hasher = Sha3_256::new();
            hasher.update(b"this is some other key we are hashing");
            hasher.finalize()
        };

        let value2 = "this is a second test".to_string();

        tree2.insert(&key2, value2.clone());

        assert_eq!(frozen.get(&key1), Some(&value1));
        assert_eq!(frozen.get(&key2), None);

        assert_eq!(tree2.get(&key1), Some(&value1));
        assert_eq!(tree2.get(&key2), Some(&value2));
    }
}
