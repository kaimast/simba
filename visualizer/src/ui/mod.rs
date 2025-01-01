mod render_loop;
pub use render_loop::UiRenderLoop;

mod logic;
pub use logic::UiLogic;

mod statistics;
pub use statistics::Statistics;

use std::collections::HashMap;
use std::sync::Mutex as StdMutex;

use simba::GlobalStatistics;

use winit::dpi::PhysicalPosition;

use crate::scene::ViewType;

pub type CursorPosition = StdMutex<PhysicalPosition<f64>>;

pub type UiEvents = StdMutex<Vec<iced::Event>>;
pub type UiElement<'a> = iced::Element<'a, UiMessage>;

#[derive(Default)]
pub struct UiMessages {
    inner: StdMutex<Vec<UiMessage>>,
}

/// The different values an object's properties can have
#[derive(Clone, Debug)]
pub enum ObjectPropertyValue {
    Float(f64),
    Int(i64),
    Str(String),
    ObjectId(simba::ObjectId),
    Id(u128),
    IdList(Vec<u128>),
}

#[derive(Clone, Debug)]
pub enum ObjectPropertyUnit {
    BitsPerSecond,
}

impl ObjectPropertyUnit {
    fn get_suffix(&self) -> &str {
        match self {
            Self::BitsPerSecond => "bits/s",
        }
    }
}

pub type ObjectPropertyMap = HashMap<String, (ObjectPropertyValue, Option<ObjectPropertyUnit>)>;

impl std::fmt::Display for ObjectPropertyValue {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self {
            Self::Float(f) => write!(fmt, "{f}")?,
            Self::Int(i) => write!(fmt, "{i}")?,
            Self::Id(id) => write!(fmt, "{id:X}")?,
            Self::ObjectId(id) => write!(fmt, "{id}")?,
            Self::Str(s) => write!(fmt, "{s}")?,
            Self::IdList(id_list) => {
                write!(fmt, "[")?;
                for id in id_list {
                    write!(fmt, "{id:X},")?;
                }
                write!(fmt, "]")?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub enum UiMessage {
    ViewSelected(ViewType),
    ObjectSelected {
        name: String,
        properties: ObjectPropertyMap,
    },
    UpdateSelectedObject {
        properties: ObjectPropertyMap,
    },
    ObjectUnselected,
    UpdateGlobalStatistics(GlobalStatistics),
    IncreaseSpeed,
    DecreaseSpeed,
}

impl UiMessages {
    pub fn take(&self) -> Vec<UiMessage> {
        let mut lock = self.inner.lock().unwrap();
        std::mem::take(&mut *lock)
    }

    pub fn push(&self, msg: UiMessage) {
        let mut lock = self.inner.lock().unwrap();
        lock.push(msg);
    }
}
