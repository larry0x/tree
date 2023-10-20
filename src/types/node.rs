use {
    crate::types::{Nibble, NibblePath},
    blake3::Hash,
    cosmwasm_schema::cw_serde,
    cosmwasm_std::{ensure, HexBinary, StdError, StdResult},
    cw_storage_plus::{Key, KeyDeserialize, PrimaryKey},
    std::any::type_name,
};

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

    pub fn new_leaf(key_hash: Hash, key: String, value: String) -> Self {
        Self::Leaf(LeafNode::new(key_hash, key, value))
    }
}

#[cw_serde]
#[derive(Copy, Eq, PartialOrd, Ord)]
pub struct Child {
    pub index: Nibble,
    // We only need to store the child node's version, not it's full NodeKey,
    // because it's full NodeKey is simply the current node's NodeKey plus the
    // child index.
    pub version: u64,
}

#[cw_serde]
pub struct InternalNode {
    // Ideally we want to usd a map type such as BTreeMap. Unfortunately,
    // CosmWasm doesn't support serialization for map types:
    // https://github.com/CosmWasm/serde-json-wasm/issues/41
    //
    // This is less efficient, and we want to keep the Vec sorted for dedup and
    // reproducability...
    pub children: Vec<Child>,
}

impl InternalNode {
    pub fn new(children: impl IntoIterator<Item = Child>) -> Self {
        Self {
            children: children.into_iter().collect(),
        }
    }

    pub fn get_child(&self, index: Nibble) -> Option<&Child> {
        self.children
            .iter()
            .find(|child| child.index == index)
    }

    pub fn get_child_mut(&mut self, index: Nibble) -> Option<&mut Child> {
        self.children
            .iter_mut()
            .find(|child| child.index == index)
    }

    pub fn set_child(&mut self, index: Nibble, version: u64) {
        if let Some(child) = self.get_child_mut(index) {
            child.version = version;
        } else {
            self.children.push(Child { index, version });
            self.children.sort();
        }
    }
}

#[cw_serde]
pub struct LeafNode {
    pub key_hash: HexBinary,
    // we don't really need to store the raw key, but we do it here for demo purpose
    pub key: String,
    pub value: String,
}

impl LeafNode {
    pub fn new(key_hash: Hash, key: String, value: String) -> Self {
        Self {
            key_hash: key_hash.as_bytes().into(),
            key,
            value,
        }
    }
}

#[cw_serde]
pub struct StaleNodeIndex {
    pub node_key: NodeKey,
    pub stale_since_version: u64,
}

// impl<'a> PrimaryKey<'a> for &'a StaleNodeIndex {
// }

