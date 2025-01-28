use super::{BITS_PER_NODE, Value};

use std::rc::Rc;

const CHILDREN_PER_BRANCH: usize = 2_usize.pow(BITS_PER_NODE as u32);

pub enum Node<V: Value> {
    Leaf(V),
    Branch {
        children: [Option<Box<Self>>; CHILDREN_PER_BRANCH],
    },
    Extension {
        bits: u8,
        child: Option<Box<Self>>,
    },
    Reference(Rc<FrozenNode<V>>),
}

pub enum FrozenNode<V: Value> {
    Leaf(V),
    Branch {
        children: [Option<Rc<Self>>; CHILDREN_PER_BRANCH],
    },
    Extension {
        bits: u8,
        child: Rc<Self>,
    },
    Reference(Rc<Self>),
}

impl<V: Value> Node<V> {
    pub fn into_frozen(self) -> FrozenNode<V> {
        match self {
            Self::Branch { mut children } => {
                let mut new_children: [Option<Rc<FrozenNode<V>>>; CHILDREN_PER_BRANCH] =
                    Default::default();
                for (pos, child) in children.iter_mut().enumerate() {
                    if let Some(child) = child.take() {
                        let child = (*child).into_frozen();
                        new_children[pos] = Some(Rc::new(child));
                    }
                }
                FrozenNode::Branch {
                    children: new_children,
                }
            }
            Self::Reference(node) => FrozenNode::Reference(node),
            Self::Extension { bits, child } => {
                let child = (*child.unwrap()).into_frozen();
                FrozenNode::Extension {
                    bits,
                    child: Rc::new(child),
                }
            }
            Self::Leaf(v) => FrozenNode::Leaf(v),
        }
    }

    pub fn make_reference(frozen_node: Rc<FrozenNode<V>>) -> Self {
        Self::Reference(frozen_node)
    }

    pub fn make_branch() -> Self {
        Self::Branch {
            children: Default::default(),
        }
    }

    pub fn make_extension(idx: u8) -> Self {
        Self::Extension {
            bits: idx,
            child: None,
        }
    }

    pub fn make_leaf(value: V) -> Self {
        Self::Leaf(value)
    }

    pub fn take_child(&mut self, idx: u8) -> Option<Box<Self>> {
        assert!((idx as usize) < CHILDREN_PER_BRANCH);

        match *self {
            Self::Leaf(_) => panic!("Cannot get child of leaf!"),
            Self::Branch { ref mut children } => children[idx as usize].take(),
            Self::Extension {
                bits,
                ref mut child,
            } => {
                if bits == idx {
                    child.take()
                } else {
                    None
                }
            }
            Self::Reference(_) => todo!(),
        }
    }

    pub fn get_child(&self, idx: u8) -> Option<&Self> {
        assert!((idx as usize) < CHILDREN_PER_BRANCH);

        match self {
            Self::Leaf(_) => panic!("Cannot get child of leaf!"),
            Self::Branch { children } => {
                if let Some(child) = children[idx as usize].as_ref() {
                    Some(child)
                } else {
                    None
                }
            }
            Self::Extension { bits, child } => {
                if *bits == idx {
                    if let Some(child) = child.as_ref() {
                        Some(child)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            Self::Reference(_) => panic!("Cannot get child of reference"),
        }
    }

    pub fn get_value(&self) -> &V {
        match self {
            Self::Leaf(v) => v,
            _ => panic!("Cannot get value of non-leaf!"),
        }
    }

    pub fn set_child(&mut self, idx: u8, new_child: Box<Self>) {
        assert!((idx as usize) < CHILDREN_PER_BRANCH);

        match *self {
            Self::Leaf(_) => panic!("Cannot set child of leaf!"),
            Self::Branch { ref mut children } => {
                children[idx as usize] = Some(new_child);
            }
            Self::Extension {
                bits,
                ref mut child,
            } => {
                if bits != idx {
                    panic!("Cannot set child");
                }

                *child = Some(new_child);
            }
            Self::Reference(_) => panic!("Cannot set child of frozen node"),
        }
    }

    pub fn into_branch(self) -> Self {
        match self {
            Self::Extension { bits, child } => {
                let mut children: [Option<Box<Self>>; CHILDREN_PER_BRANCH] = Default::default();

                children[bits as usize] = child;
                Self::Branch { children }
            }
            _ => panic!("Function can only be called on a branch"),
        }
    }

    pub fn is_branch(&self) -> bool {
        matches!(self, Self::Branch { .. })
    }

    /// If this is a reference; it will return the frozen node it points to
    pub fn get_reference(&self) -> Option<&FrozenNode<V>> {
        if let Self::Reference(frozen) = self {
            Some(frozen)
        } else {
            None
        }
    }
}

impl<V: Value> FrozenNode<V> {
    pub fn get_value(&self) -> &V {
        match self {
            Self::Leaf(v) => v,
            _ => panic!("Cannot get value of non-leaf!"),
        }
    }

    pub fn to_reference(self_ptr: Rc<Self>) -> Rc<Self> {
        if let Self::Reference(other) = &*self_ptr {
            other.clone()
        } else {
            self_ptr
        }
    }

    pub fn get_child(&self, idx: u8) -> Option<&Self> {
        assert!((idx as usize) < CHILDREN_PER_BRANCH);

        match self {
            Self::Leaf(_) => panic!("Cannot get child of leaf!"),
            Self::Branch { children } => {
                if let Some(child) = children[idx as usize].as_ref() {
                    Some(child)
                } else {
                    None
                }
            }
            Self::Extension { bits, child } => {
                if *bits == idx {
                    Some(child)
                } else {
                    None
                }
            }
            Self::Reference(c) => {
                assert!(c.is_reference());
                c.get_child(idx)
            }
        }
    }

    pub fn is_reference(&self) -> bool {
        matches!(self, Self::Reference(_))
    }
}
