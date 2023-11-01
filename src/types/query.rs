use {
    crate::Hash,
    cosmwasm_schema::cw_serde,
    cosmwasm_std::Binary,
};
#[cfg(feature = "debug")]
use crate::{Node, NodeKey};

#[cw_serde]
pub struct RootResponse {
    pub version: u64,
    pub root_hash: Hash,
}

#[cw_serde]
pub struct GetResponse<K, V> {
    pub key: K,
    /// None if not found
    pub value: Option<V>,
    /// None if proof is not requested, or if the tree is empty
    pub proof: Option<Binary>,
}

#[cfg(feature = "debug")]
#[cw_serde]
pub struct NodeResponse<K, V> {
    pub node_key: NodeKey,
    pub node: Node<K, V>,
    pub hash: Hash,
}

#[cfg(feature = "debug")]
#[cw_serde]
pub struct OrphanResponse {
    pub node_key: NodeKey,
    pub since_version: u64,
}
