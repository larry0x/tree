use {
    crate::types::{Node, NodeKey},
    cw_storage_plus::{Item, Map},
};

pub const LAST_COMMITTED_VERSION: Item<u64> = Item::new("v");

pub const NODES: Map<&NodeKey, Node> = Map::new("n");
