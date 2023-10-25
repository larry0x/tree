use {
    crate::types::{Hash, Node, NodeKey, Proof},
    cosmwasm_schema::cw_serde,
};

#[cw_serde]
pub struct RootResponse {
    pub version: u64,
    pub root_hash: Hash,
}

#[cw_serde]
pub struct GetResponse {
    pub key: String,
    /// None if not found
    pub value: Option<String>,
    /// None if proof is not requested
    pub proof: Option<Proof>,
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
