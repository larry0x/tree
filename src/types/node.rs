use {
    crate::types::{hash_child, hash_data, Children, Hash, Nibble},
    blake3::Hasher,
    cosmwasm_schema::cw_serde,
};

#[cw_serde]
#[derive(Eq)]
pub struct Child {
    pub index: Nibble,
    pub version: u64,
    pub hash: Hash,
}

#[cw_serde]
pub struct Record {
    pub key: String,
    pub value: String,
}

/// Unlike Ethereum's Patricia trie, we don't make the distinction between
/// internal and leaf nodes. They are coalesced into one single node type:
///
/// - if a node have children, it's known as an internal node;
/// - if a node has data but no child, it's known as a leaf node.
///
/// Additionally, Ethereum's null node and extension node types are simply
/// dropped:
///
/// - null node is just a marker for an empty tree root, which we can do without;
/// - extension nodes offer some optimization if there are keys that share a
///   long common substring, but this is unlikely as dataset gets bigger, so the
///   opimization is limited with the tradeoff of higher code complexity. We
///   consider it's not worth it. See a similar discussion in Diem's JMT paper.
#[cw_serde]
#[derive(Default)]
pub struct Node {
    // TODO: replace this with BTreeMap<Nibble, Child> when possible
    pub children: Children,
    pub data: Option<Record>,
}

impl Node {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn new_internal(children: impl Into<Children>) -> Self {
        Self {
            children: children.into(),
            data: None,
        }
    }

    pub fn new_leaf(key: String, value: String) -> Self {
        Self {
            children: Children::new(vec![]),
            data: Some(Record { key, value })
        }
    }

    pub fn is_leaf(&self) -> bool {
        self.children.is_empty() && self.data.is_some()
    }

    pub fn is_empty(&self) -> bool {
        self.children.is_empty() && self.data.is_none()
    }

    /// Compute the node's hash, which is defined as:
    ///
    /// hash(childA.index || childA.hash || ... || childZ.hash || childZ.value || len(key) || key || value)
    ///
    /// where:
    /// - `||` means byte concatenation.
    /// - `child{A..Z}` are the node's children, ordered ascendingly by indexes.
    ///   Only children that exist are included.
    /// - `len()` returns a 16-bit (2 bytes) unsigned integer in big endian encoding.
    pub fn hash(&self) -> Hash {
        let mut hasher = Hasher::new();

        for child in &self.children {
            hash_child(&mut hasher, child);
        }

        if let Some(data) = &self.data {
            hash_data(&mut hasher, data)
        }

        hasher.finalize().into()
    }
}
