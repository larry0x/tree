#[derive(Debug, PartialEq, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Std(#[from] cosmwasm_std::StdError),

    #[error("cannot query at version {query_version} which is newer than the latest ({latest_version})")]
    VersionNewerThanLatest {
        query_version: u64,
        latest_version: u64,
    },

    #[error("root node of version {version} not found, probably pruned")]
    RootNodeNotFound {
        version: u64,
    },

    #[error("tree corrupted: non-root node not found")]
    NonRootNodeNotFound {
        // TODO: add node_key here
    },
}

pub(crate) type Result<T> = std::result::Result<T, Error>;
