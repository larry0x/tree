use {
    crate::types::{Hash, Node, NodeKey},
    cosmwasm_schema::cw_serde,
    cosmwasm_std::Binary,
};

#[cw_serde]
pub struct RootResponse {
    pub version: u64,
    pub root_hash: Hash,
}

#[cw_serde]
pub struct GetResponse {
    pub version: u64,
    pub key: String,
    /// None if not found
    pub value: Option<String>,
    /// None if proof is not requested, or if the tree is empty
    pub proof: Option<Binary>,
}

#[cw_serde]
pub struct NodeResponse {
    pub node_key: NodeKey,
    pub node: Node,
    pub hash: Hash,
}

#[cw_serde]
pub struct OrphanResponse {
    pub node_key: NodeKey,
    pub since_version: u64,
}
