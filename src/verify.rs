use crate::{Hash, NibblePath, NodeData, Proof, ProofChild};

pub fn verify_membership(
    root_hash: &Hash,
    key: &str,
    value: &str,
    proof: &Proof,
) -> Result<()> {
    let nibble_path = NibblePath::from(key.as_bytes().to_vec());
    let proof_len = proof.len();

    // compute the hash of the node that contains the data of interest
    // it should be the first element in the proof
    let node = proof.first().ok_or(VerificationError::EmptyProof)?;
    let data = NodeData {
        key: key.into(),
        value: value.into(),
    };
    let mut hash = node.hash(None, Some(&data));

    // traverse up the tree and compute the hash of each node
    // eventually we should reach the root
    for i in 1..proof_len {
        let node = &proof[i];
        let child = ProofChild {
            index: nibble_path.get_nibble(proof_len - i - 1),
            // TODO: can we avoid this cloning?
            hash: hash.clone(),
        };
        hash = node.hash(Some(&child), None);
    }

    // now we have arrived at the root, the computed root hash should match the
    // given root hash
    if hash != *root_hash {
        return Err(VerificationError::RootHashMismatch {
            given: root_hash.clone(),
            computed: hash,
        });
    }

    Ok(())
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
    #[error("proof cannot be empty")]
    EmptyProof,

    #[error("hash mismatch! computed: {computed}, given: {given}")]
    RootHashMismatch {
        given: Hash,
        computed: Hash,
    },
}

type Result<T> = std::result::Result<T, VerificationError>;

// ----------------------------------- tests -----------------------------------

#[cfg(test)]
use crate::{Nibble, ProofNode};

#[cfg(test)]
fn hash(hex_str: &str) -> Hash {
    hex::decode(hex_str).unwrap().as_slice().try_into().unwrap()
}

#[test]
fn verifying_membership() {
    let root_hash = hash("15484df8d087ecd9e58d6b7c8c6bc3e80718d367e1e55861bac3207709bf92fa");
    let key = "fuzz";
    let value = "buzz";
    let proof = vec![
        ProofNode {
            children: vec![],
            data: None,
        },
        ProofNode {
            children: vec![ProofChild {
                index: Nibble::new(6),
                hash: hash("0aaeb7f6ce9c7ee7d47fc5643f3fe54eb30ae79a52d1a637b8723dc06d82d76a"),
            }],
            data: None,
        },
        ProofNode {
            children: vec![ProofChild {
                index: Nibble::new(0xc),
                hash: hash("33f24d09639e54c70bfac0168b9ffa29bca260877fa9d01aecb7a9edf8299c43"),
            }],
            data: None,
        },
        ProofNode {
            children: vec![ProofChild {
                index: Nibble::new(7),
                hash: hash("330dd01838a67a80022676874011c607b694b9ba3ca81503dbc2f422870ae664")
            }],
            data: None,
        },
    ];

    assert!(verify_membership(&root_hash, key, value, &proof).is_ok());
}
