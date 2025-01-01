use std::collections::HashMap;
use std::rc::Rc;

pub use asim::network::Object;
pub use asim::network::ObjectId;

pub(crate) type ObjectMap = HashMap<ObjectId, Rc<dyn Object>>;
