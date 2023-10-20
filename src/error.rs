#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Std(#[from] cosmwasm_std::StdError),

    #[error("tree corrupted: non-root node not found")]
    NonRootNodeNotFound {
        // TODO: add node_key here
    },
}

pub(crate) type Result<T> = std::result::Result<T, Error>;
