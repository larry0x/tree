use {
    crate::types::{Hash, Nibble, NibblePath},
    blake3::Hasher,
    cosmwasm_schema::cw_serde,
    cosmwasm_std::{ensure, StdError, StdResult},
    cw_storage_plus::{Key, KeyDeserialize, PrimaryKey},
    schemars::JsonSchema,
    serde::{Deserialize, Serialize},
    std::{any::type_name, fmt},
};

const INTERNAL_PREFIX: u8 = 0;
const LEAF_PREFIX:     u8 = 1;

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Hash, JsonSchema)]
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

impl fmt::Debug for NodeKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NodeKey{{version={} nibbles=\"{}\"}}", self.version, self.nibble_path.to_hex())
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
        // TODO: we should set a limit to nibble_path length so that num_nibbles
        // is always smaller than u16::MAX = 65535
        // this means keys must be no longer than 32767 bytes which should be
        // more than enough
        key.extend((self.nibble_path.num_nibbles as u16).to_be_bytes());
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
    pub fn new_internal(children: Vec<Child>) -> Self {
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

impl From<Vec<Child>> for Children {
    fn from(vec: Vec<Child>) -> Self {
        Self(vec)
    }
}

impl AsRef<[Child]> for Children {
    fn as_ref(&self) -> &[Child] {
        self.0.as_slice()
    }
}

impl<'a> IntoIterator for &'a Children {
    type Item = &'a Child;
    type IntoIter = std::slice::Iter<'a, Child>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.as_slice().iter()
    }
}

impl Children {
    pub fn count(&self) -> usize {
        self.0.len()
    }

    pub fn get(&self, index: Nibble) -> Option<&Child> {
        self.0
            .iter()
            .find(|child| child.index == index)
    }

    /// If there is exactly one child, then return a reference to this child.
    /// If there is more than one children, return None.
    ///
    /// Under normal circumstances, this function shouldn't be called when there
    /// is zero child.
    pub fn get_only(&self) -> Option<&Child> {
        if self.0.len() == 1 {
            return Some(&self.0[0]);
        }

        None
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

    pub fn remove(&mut self, index: Nibble) {
        let Some(pos) = self.0.iter().position(|child| child.index == index) else {
            panic!("child not found with index {index}");
        };

        self.0.remove(pos);
    }
}

#[cw_serde]
pub struct InternalNode {
    pub children: Children,
}

impl InternalNode {
    pub fn new(children: Vec<Child>) -> Self {
        Self {
            children: children.into(),
        }
    }

    // We define the hash of an internal node as
    //
    // hash(internal_prefix || childA.index || childA.hash || ... || childZ.index || childZ.hash)
    //
    // where || means byte concatenation, and child{A..Z} are children that
    // exist, in ascending order. That is, non-existing children are not part
    // of the preimage.
    pub fn hash(&self) -> Hash {
        let mut hasher = Hasher::new();
        hasher.update(&[INTERNAL_PREFIX]);
        for child in &self.children {
            hasher.update(&[child.index.byte()]);
            hasher.update(child.hash.as_bytes());
        }
        hasher.finalize().into()
    }
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

    fn key(&self) -> &[u8] {
        self.key.as_bytes()
    }

    fn value(&self) -> &[u8] {
        self.value.as_bytes()
    }

    /// We define the hash of a leaf node as:
    ///
    /// hash(leaf_prefix || len(key) || key || len(value) || value)
    ///
    /// where || means byte concatenation, and len() returns a 32-bit unsigned
    /// integer in big endian encoding.
    ///
    /// The length prefix is necessary, because otherwise we won't be able to
    /// differentiate, for example, these two:
    ///
    /// | key       | value    |
    /// | --------- | -------- |
    /// | `b"foo"`  | `b"bar"` |
    /// | `b"foob"` | `b"ar"`  |
    pub fn hash(&self) -> Hash {
        let mut hasher = Hasher::new();
        hasher.update(&[LEAF_PREFIX]);
        hasher.update((self.key().len() as u32).to_be_bytes().as_slice());
        hasher.update(self.key());
        hasher.update((self.value().len() as u32).to_be_bytes().as_slice());
        hasher.update(self.value());
        hasher.finalize().into()
    }
}
