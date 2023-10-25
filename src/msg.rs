use {
    crate::types::{Hash, Node, NodeKey, Proof},
    cosmwasm_schema::{cw_serde, QueryResponses},
    cosmwasm_std::Empty,
};

pub type InstantiateMsg = Empty;

#[cw_serde]
pub enum ExecuteMsg {
    /// Insert a key-value pair into the tree, increment the version.
    Insert {
        key: String,
        value: String,
    },

    /// Delete a key from the tree, increment the version.
    Delete {
        key: String,
    },

    /// Delete stale nodes, i.e. ones that are no longer the latest part of the
    /// tree.
    Prune {
        /// Prune nodes that became stale prior to version (inclusive).
        /// If not provided, all stale nodes are pruned.
        up_to_version: Option<u64>,
    },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Query the root node at a specific version
    #[returns(u64)]
    Root {
        /// If not specified, default to the latest version
        version: Option<u64>,
    },

    /// Query the value corresponding to the given key
    #[returns(GetResponse)]
    Get {
        key: String,
        /// Whether to request merkle proof
        prove: bool,
        /// If not specified, default to the latest version
        version: Option<u64>,
    },

    /// Query a specific node by the node key
    #[returns(NodeResponse)]
    Node {
        node_key: NodeKey,
    },

    /// List all nodes
    #[returns(Vec<NodeResponse>)]
    Nodes {
        start_after: Option<NodeKey>,
        limit: Option<u32>,
    },

    /// List nodes that are orphaned, i.e. no longer part of the latest version
    /// of the tree.
    #[returns(Vec<OrphanResponse>)]
    Orphans {
        start_after: Option<OrphanResponse>,
        limit: Option<u32>,
    },
}

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
