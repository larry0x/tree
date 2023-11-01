use {
    crate::Node,
    cosmwasm_schema::cw_serde,
    std::collections::BTreeMap,
};

pub type Batch<K, V> = BTreeMap<K, Op<V>>;

#[cw_serde]
pub enum Op<V> {
    Insert(V),
    Delete,
}

#[cw_serde]
pub enum OpResponse<K, V> {
    /// The node's children and/or data have been changed. This signals to the
    /// node's parent that the hash needs to be recomputed.
    Updated(Node<K, V>),
    /// After applying the op, the node no longer has any child or data.
    /// Therefore it is removed from the tree.
    Deleted,
    /// Nothing happened to the node. This signals that the hash does not need
    /// to be recomputed.
    Unchanged,
}
