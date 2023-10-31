use {crate::Node, cosmwasm_schema::cw_serde};

#[cw_serde]
pub enum Op {
    Insert(String),
    Delete,
}

#[cw_serde]
pub enum OpResponse {
    /// The node's children and/or data have been changed. This signals to the
    /// node's parent that the hash needs to be recomputed.
    Updated(Node),
    /// After applying the op, the node no longer has any child or data.
    /// Therefore it is removed from the tree.
    Deleted,
    /// Nothing happened to the node. This signals that the hash does not need
    /// to be recomputed.
    Unchanged,
}
