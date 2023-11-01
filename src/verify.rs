use crate::{Hash, NibblePath, Proof, ProofChild, Record};

pub fn verify_membership<K, V>(
    root_hash: &Hash,
    key: &K,
    value: &V,
    proof: &Proof<K, V>,
) -> Result<()>
where
    K: Clone + AsRef<[u8]>,
    V: Clone + AsRef<[u8]>,
{
    let nibble_path = NibblePath::from(key);

    // compute the hash of the node that contains the data of interest
    // it should be the first element in the proof
    let node = proof.first().ok_or(VerificationError::ProofEmpty)?;
    let data = Record {
        key: key.clone(),
        value: value.clone(),
    };
    let hash = node.hash(None, Some(&data));

    compute_and_check_root_hash(root_hash, proof, nibble_path, hash)
}

pub fn verify_non_membership<K, V>(
    root_hash: &Hash,
    key: &K,
    proof: &Proof<K, V>,
) -> Result<()>
where
    K: AsRef<[u8]> + PartialEq,
    V: AsRef<[u8]>,
{
    let proof_len = proof.len();
    let nibble_path = NibblePath::from(key);

    let Some(node) = proof.first() else {
        return Err(VerificationError::ProofEmpty);
    };

    if proof_len > nibble_path.num_nibbles + 1 {
        return Err(VerificationError::ProofTooLong);
    }

    if proof_len <= nibble_path.num_nibbles && node.has_child_at_index(nibble_path.get_nibble(proof_len - 1)) {
        return Err(VerificationError::UnexpectedChild);
    }

    if let Some(data) = &node.data {
        if data.key == *key {
            return Err(VerificationError::KeyExists);
        }
    }

    let hash = node.hash(None, None);

    compute_and_check_root_hash(root_hash, proof, nibble_path, hash)
}

fn compute_and_check_root_hash<K, V>(
    root_hash: &Hash,
    proof: &Proof<K, V>,
    nibble_path: NibblePath,
    mut hash: Hash,
) -> Result<()>
where
    K: AsRef<[u8]>,
    V: AsRef<[u8]>,
{
    let proof_len = proof.len();

    // traverse up the tree and compute the hash of each node
    // eventually we should reach the root
    #[allow(clippy::needless_range_loop)]
    for i in 1..proof_len {
        let node = &proof[i];
        let child = ProofChild {
            index: nibble_path.get_nibble(proof_len - i - 1),
            hash,
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

#[derive(Debug, thiserror::Error)]
pub enum VerificationError {
    #[error("proof cannot be empty")]
    ProofEmpty,

    #[error("proof is too long")]
    ProofTooLong,

    #[error("want to prove non-membership but key in fact exists")]
    KeyExists,

    #[error("expecting node to not have a certain child but it does")]
    UnexpectedChild,

    #[error("hash mismatch! computed: {computed}, given: {given}")]
    RootHashMismatch {
        given: Hash,
        computed: Hash,
    },
}

type Result<T> = std::result::Result<T, VerificationError>;

// ----------------------------------- tests -----------------------------------

#[cfg(test)]
mod tests {
    use {
        crate::{
            verify_membership, verify_non_membership, Hash, Nibble, Proof, ProofChild, ProofNode,
            Record,
        },
        test_case::test_case,
    };

    fn hash(hex_str: &str) -> Hash {
        hex::decode(hex_str).unwrap().as_slice().try_into().unwrap()
    }

    #[test_case(
        hash("15484df8d087ecd9e58d6b7c8c6bc3e80718d367e1e55861bac3207709bf92fa"),
        "fuzz".into(),
        "buzz".into(),
        vec![
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
                    hash: hash("330dd01838a67a80022676874011c607b694b9ba3ca81503dbc2f422870ae664"),
                }],
                data: None,
            },
        ];
        "proving (fuzz, buzz) exists"
    )]
    fn verifying_membership(
        root_hash: Hash,
        key: String,
        value: String,
        proof: Proof<String, String>,
    ) {
        assert!(verify_membership(&root_hash, &key, &value, &proof).is_ok());
    }

    #[test_case(
        hash("15484df8d087ecd9e58d6b7c8c6bc3e80718d367e1e55861bac3207709bf92fa"),
        "f".into(),
        vec![
            ProofNode {
                children: vec![
                    ProofChild {
                        index: Nibble::new(6),
                        hash: hash("0aaeb7f6ce9c7ee7d47fc5643f3fe54eb30ae79a52d1a637b8723dc06d82d76a"),
                    },
                    ProofChild {
                        index: Nibble::new(7),
                        hash: hash("8b71a1adc67423c9bb53a1eb6a20f664138f112697d8f419f1c0ee1528c47d9f"),
                    },
                ],
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
                    hash: hash("330dd01838a67a80022676874011c607b694b9ba3ca81503dbc2f422870ae664"),
                }],
                data: None,
            },
        ];
        "proving f does not exist"
    )]
    #[test_case(
        hash("15484df8d087ecd9e58d6b7c8c6bc3e80718d367e1e55861bac3207709bf92fa"),
        "foo".into(),
        vec![
            ProofNode {
                children: vec![],
                data: Some(Record {
                    key: "food".into(),
                    value: "ramen".into(),
                }),
            },
            ProofNode {
                children: vec![ProofChild {
                    index: Nibble::new(7),
                    hash: hash("8b71a1adc67423c9bb53a1eb6a20f664138f112697d8f419f1c0ee1528c47d9f"),
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
                    hash: hash("330dd01838a67a80022676874011c607b694b9ba3ca81503dbc2f422870ae664"),
                }],
                data: None,
            },
        ];
        "proving foo does not exist"
    )]
    fn verifying_non_membership(
        root_hash: Hash,
        key: String,
        proof: Proof<String, String>,
    ) {
        assert!(verify_non_membership(&root_hash, &key, &proof).is_ok());
    }
}
