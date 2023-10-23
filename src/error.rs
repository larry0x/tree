use crate::types::NodeKey;

#[derive(Debug, PartialEq, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Std(#[from] cosmwasm_std::StdError),

    #[error("cannot query at version {querying} which is newer than the latest ({latest})")]
    VersionNewerThanLatest {
        latest: u64,
        querying: u64,
    },

    #[error("root node of version {version} not found, probably pruned")]
    RootNodeNotFound {
        version: u64,
    },

    #[error(
        "tree corrupted! non-root node not found (version: {}, nibble_path: {})",
        node_key.version,
        node_key.nibble_path.to_hex(),
    )]
    NonRootNodeNotFound {
        node_key: NodeKey,
    },
}

pub(crate) type Result<T> = std::result::Result<T, Error>;
