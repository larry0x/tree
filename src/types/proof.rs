use {
    crate::types::{hash_data, hash_proof_child, Children, Hash, Nibble, Node, NodeData},
    blake3::Hasher,
    cosmwasm_schema::cw_serde,
};

/// This can either be a membership or a non-membership proof.
///
/// For membership proof, it is the path leading from the node containing the
/// KV of interest to the root.
///
/// For non-membership proof, it is the path leading from a node that lacks the
/// child that would lead to the KV of interest if it existed, to the root.
pub type Proof = Vec<ProofNode>;

/// ProofChild is like Child but simplified by removing the version. We don't
/// need the version for proof because the version isn't merklized.
#[cw_serde]
pub struct ProofChild {
    pub index: Nibble,
    pub hash: Hash,
}

impl From<Children> for Vec<ProofChild> {
    fn from(children: Children) -> Self {
        children
            .into_iter()
            .map(|child| ProofChild {
                index: child.index,
                hash: child.hash,
            })
            .collect()
    }
}

/// ProofNode is like Node but simplified in three ways:
/// - contains SimpleChild instead of Child
/// - children doesn't need to include the child of interest, because it can be
///   inferred, and for the sake of reducing proof size, we leave it out
/// - similarly, for membership proofs, the data does not need to be included.
#[cw_serde]
pub struct ProofNode {
    pub children: Vec<ProofChild>,
    pub data: Option<NodeData>
}


impl ProofNode {
    pub fn from_node(mut node: Node, drop_child_at_index: Option<Nibble>, drop_data: bool) -> Self {
        if let Some(index) = drop_child_at_index {
            node.children.remove(index);
        }

        if drop_data {
            node.data = None;
        }

        Self {
            children: node.children.into(),
            data: node.data,
        }
    }

    pub fn has_child_at_index(&self, index: Nibble) -> bool {
        self.children
            .iter()
            .any(|child| child.index == index)
    }

    // TODO: refactor this code to make it less ugly??
    pub fn hash(&self, maybe_child: Option<&ProofChild>, maybe_data: Option<&NodeData>) -> Hash {
        let mut hasher = Hasher::new();

        for child in &self.children {
            if let Some(c) = maybe_child {
                if c.index < child.index {
                    hash_proof_child(&mut hasher, c);
                }
            }

            hash_proof_child(&mut hasher, child)
        }

        if let Some(c) = maybe_child {
            if let Some(child) = self.children.last() {
                if c.index > child.index {
                    hash_proof_child(&mut hasher, c);
                }
            } else {
                hash_proof_child(&mut hasher, c);
            }
        }

        match (maybe_data, &self.data) {
            (Some(d), None) | (Some(d), Some(_)) | (None, Some(d)) => {
                hash_data(&mut hasher, d);
            }
            _ => (),
        }

        hasher.finalize().into()
    }
}
