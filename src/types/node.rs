use {
    crate::types::{hash_two, Hash, Nibble, NibblePath},
    cosmwasm_schema::cw_serde,
    cosmwasm_std::{ensure, StdError, StdResult},
    cw_storage_plus::{Key, KeyDeserialize, PrimaryKey},
    std::any::type_name,
};

const PLACEHOLDER_HASH: [u8; blake3::OUT_LEN] = [0; blake3::OUT_LEN];

#[cw_serde]
#[derive(Eq, Hash)]
pub struct NodeKey {
    pub version: u64,
    pub nibble_path: NibblePath,
}

impl NodeKey {
    pub fn root(version: u64) -> Self {
        Self {
            version,
            nibble_path: NibblePath::empty(),
        }
    }
}

impl<'a> PrimaryKey<'a> for &'a NodeKey {
    type Prefix = u64;
    type SubPrefix = ();
    type Suffix = NibblePath;
    type SuperSuffix = Self;

    fn key(&self) -> Vec<Key> {
        let mut key = vec![];
        key.extend(self.version.to_be_bytes());
        // in practice, there can be max 64 nibbles, so its safe to cast it to a single byte
        // length of BLAKE3 hash in bits: 256
        // bits in a nibble: 4
        // max nibble path length: 256 / 4 = 64
        // u8::MAX = 255
        key.push(self.nibble_path.num_nibbles as u8);
        key.extend(self.nibble_path.bytes.as_slice());
        vec![Key::Owned(key)]
    }
}

impl KeyDeserialize for &NodeKey {
    type Output = NodeKey;

    fn from_vec(value: Vec<u8>) -> StdResult<Self::Output> {
        ensure!(
            value.len() >= 9,
            StdError::parse_err(type_name::<Self::Output>(), "raw key must have at least 9 bytes")
        );

        let version = u64::from_be_bytes(value[..8].try_into().unwrap());
        let nibble_path = NibblePath::from_slice(&value[8..])?;

        Ok(NodeKey {
            version,
            nibble_path,
        })
    }
}

#[cw_serde]
pub enum Node {
    Internal(InternalNode),
    Leaf(LeafNode),
}

impl Node {
    pub fn new_internal(children: impl IntoIterator<Item = Child>) -> Self {
        Self::Internal(InternalNode::new(children))
    }

    pub fn new_leaf(key: String, value: String) -> Self {
        Self::Leaf(LeafNode::new(key, value))
    }

    pub fn hash(&self) -> Hash {
        match self {
            Node::Internal(internal_node) => internal_node.hash(),
            Node::Leaf(leaf_node) => leaf_node.hash(),
        }
    }
}

#[cw_serde]
#[derive(Eq)]
pub struct Child {
    pub index: Nibble,
    pub version: u64,
    pub hash: Hash,
}

// Ideally we want to usd a map type such as BTreeMap. Unfortunately, CosmWasm
// doesn't support serialization for map types:
// https://github.com/CosmWasm/serde-json-wasm/issues/41
#[cw_serde]
pub struct Children(Vec<Child>);

impl<T> From<T> for Children
where
    T: IntoIterator<Item = Child>,
{
    fn from(value: T) -> Self {
        Self(value.into_iter().collect())
    }
}

impl AsRef<[Child]> for Children {
    fn as_ref(&self) -> &[Child] {
        self.0.as_slice()
    }
}

impl Children {
    pub fn get(&self, index: Nibble) -> Option<&Child> {
        self.0
            .iter()
            .find(|child| child.index == index)
    }

    pub fn insert(&mut self, new_child: Child) {
        for (pos, child) in self.0.iter().enumerate() {
            if child.index == new_child.index {
                self.0[pos] = new_child;
                return;
            }

            if child.index > new_child.index {
                self.0.insert(pos, new_child);
                return;
            }
        }

        self.0.push(new_child);
    }
}

#[cw_serde]
pub struct InternalNode {
    pub children: Children,
}

impl InternalNode {
    pub fn new<T>(children: T) -> Self
    where
        T: IntoIterator<Item = Child>,
    {
        Self {
            children: children.into(),
        }
    }

    pub fn hash(&self) -> Hash {
        merkle_hash(self.children.as_ref(), 0, 16)
    }
}

fn merkle_hash(siblings: &[Child], start: usize, end: usize) -> Hash {
    if siblings.is_empty() {
        return PLACEHOLDER_HASH.into();
    }

    if siblings.len() == 1 {
        return siblings[0].hash.clone();
    }

    let mid = (start + end) / 2;
    let mid_nibble = Nibble::from(mid);
    let mid_pos = siblings.iter().position(|child| child.index >= mid_nibble).unwrap_or(end);

    let left_half = merkle_hash(&siblings[..mid_pos], start, mid);
    let right_half = merkle_hash(&siblings[mid_pos..], mid, end);

    hash_two(left_half, right_half)
}

#[cw_serde]
pub struct LeafNode {
    pub key: String,
    pub value: String,
}

impl LeafNode {
    pub fn new(key: String, value: String) -> Self {
        Self {
            key,
            value,
        }
    }

    /// A leaf node's hash is defined as `hash(hash(key) | hash(value))`
    pub fn hash(&self) -> Hash {
        hash_two(&self.key, &self.value)
    }
}
