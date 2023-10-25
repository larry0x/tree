use {
    crate::types::{Hash, LeafNode, NibblePath, Proof},
    blake3::Hasher,
};

pub fn verify_membership(
    root_hash: &Hash,
    key: &str,
    value: &str,
    proof: &Proof,
) -> Result<(), VerificationError> {
    let proof_len = proof.len();
    let leaf_node = LeafNode::new(key.into(), value.into());
    let nibble_path = NibblePath::from(key.as_bytes().to_vec());

    let mut hash = leaf_node.hash();

    for (i, siblings) in proof.iter().enumerate() {
        let target_index = nibble_path.get_nibble(proof_len - i - 1);

        let mut hasher = Hasher::new();
        let mut last_index = None;

        for sibling in siblings {
            if let Some(index) = last_index {
                if sibling.index <= index {
                    return Err(VerificationError::SiblingsUnsorted);
                }
            }

            if target_index == sibling.index {
                if hash != sibling.hash {
                    return Err(VerificationError::SiblingHashMismatch {
                        provided: sibling.hash.clone(),
                        computed: hash,
                    });
                }
            }

            hasher.update(&[sibling.index.byte()]);
            hasher.update(sibling.hash.as_bytes());

            last_index = Some(sibling.index);
        }

        hash = hasher.finalize().into();
    }

    if &hash != root_hash {
        return Err(VerificationError::RootHashMismatch {
            provided: root_hash.clone(),
            computed: hash,
        });
    }

    Ok(())
}

pub fn verify_non_membership(
    root_hash: &Hash,
    key: &str,
    proof: &Proof,
) -> Result<(), VerificationError> {
    todo!();
}

#[derive(Debug, thiserror::Error)]
pub enum VerificationError {
    #[error("root hash mismatch! provided: {provided}, computed: {computed}")]
    RootHashMismatch {
        provided: Hash,
        computed: Hash,
    },

    #[error("sibling hash mismatch! provided: {provided}, computed: {computed}")]
    SiblingHashMismatch {
        provided: Hash,
        computed: Hash,
    },

    #[error("expecting sibling to not exist in non-existent proof but it exists")]
    SiblingFound,

    #[error("siblings in a step are not sorted by index")]
    SiblingsUnsorted,
}

#[cfg(test)]
mod test {
    use {
        crate::{
            execute::{insert, init},
            query,
            types::{Hash, Nibble, Sibling},
            verify::verify_membership,
        },
        cosmwasm_std::testing::MockStorage,
    };

    fn new_hash(hex_str: &str) -> Hash {
        hex::decode(hex_str).unwrap().as_slice().try_into().unwrap()
    }

    fn setup_test() -> MockStorage {
        let mut store = MockStorage::new();

        init(&mut store).unwrap();
        insert(&mut store, "foo".into(), "bar".into()).unwrap();
        insert(&mut store, "fuzz".into(), "buzz".into()).unwrap();
        insert(&mut store, "pumpkin".into(), "cat".into()).unwrap();

        store
    }

    /// Let's try proving (foo, bar) is in the tree
    #[test]
    fn verifying_membership() {
        let store = setup_test();
        let root = query::root(&store, None).unwrap();

        let proof = vec![
            vec![
                Sibling {
                    index: Nibble::from(6u8),
                    hash: new_hash("6ab811417f05e0c526991e86d67305e71f28803bc0149f35f68e247409f60055"),
                },
                Sibling {
                    index: Nibble::from(7u8),
                    hash: new_hash("8b71a1adc67423c9bb53a1eb6a20f664138f112697d8f419f1c0ee1528c47d9f"),
                },
            ],
            vec![
                Sibling {
                    index: Nibble::from(6u8),
                    hash: new_hash("fc2e4cbb65fe5aacb24d4b4546441baa16f130a3efb14d14fe703770cd21b825"),
                },
            ],
            vec![
                Sibling {
                    index: Nibble::from(6u8),
                    hash: new_hash("5bdd84c3628c3eb9891f61d62eb710db14486bdd2564d8820ec12466111d33ce"),
                },
                Sibling {
                    index: Nibble::from(7u8),
                    hash: new_hash("330dd01838a67a80022676874011c607b694b9ba3ca81503dbc2f422870ae664"),
                },
            ],
        ];

        assert!(verify_membership(&root.root_hash, "foo", "bar", &proof).is_ok());
    }

    /// Let's try proving the key "food" isn't in the tree
    #[test]
    fn verifying_non_membership() {
        let store = setup_test();
        let root = query::root(&store, None).unwrap();

        todo!();
    }
}
