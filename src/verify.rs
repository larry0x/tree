use crate::{Hash, Proof};

pub fn verify_membership(
    _root_hash: &Hash,
    _key: &str,
    _value: &str,
    _proof: &Proof,
) -> Result<()> {
    todo!();
}

pub fn verify_non_membership(
    _root_hash: &Hash,
    _key: &str,
    _proof: &Proof,
) -> Result<()> {
    todo!();
}

#[derive(Debug, thiserror::Error)]
pub enum VerificationError {
}

type Result<T> = std::result::Result<T, VerificationError>;
