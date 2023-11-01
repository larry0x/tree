use {
    crate::{Nibble, NibblePath},
    cosmwasm_std::{ensure, StdError, StdResult},
    cw_storage_plus::{Key, KeyDeserialize, PrimaryKey},
    schemars::JsonSchema,
    serde::{Deserialize, Serialize},
    std::{any::type_name, fmt},
};

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Hash, JsonSchema)]
pub struct NodeKey {
    pub version: u64,
    pub nibble_path: NibblePath,
}

impl NodeKey {
    pub fn new(version: u64, nibble_path: NibblePath) -> Self {
        Self {
            version,
            nibble_path,
        }
    }

    pub fn root(version: u64) -> Self {
        Self {
            version,
            nibble_path: NibblePath::empty(),
        }
    }

    pub fn child(&self, version: u64, index: Nibble) -> Self {
        Self {
            version,
            nibble_path: self.nibble_path.child(index),
        }
    }

    pub fn depth(&self) -> usize {
        self.nibble_path.num_nibbles
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
