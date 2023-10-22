use {
    schemars::JsonSchema,
    serde::{
        de::{self, Deserialize, Deserializer, Visitor},
        ser::{Serialize, Serializer},
    },
    std::fmt,
};

pub const HASH_LEN: usize = blake3::OUT_LEN;

/// The `blake3::Hash` type doesn't implement JsonSchema and doesn't have a good
/// serialization method. We replace it with this type.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, JsonSchema)]
pub struct Hash([u8; HASH_LEN]);

impl From<[u8; HASH_LEN]> for Hash {
    fn from(bytes: [u8; HASH_LEN]) -> Self {
        Self(bytes)
    }
}

impl From<blake3::Hash> for Hash {
    fn from(hash: blake3::Hash) -> Self {
        Self(*hash.as_bytes())
    }
}

impl AsRef<[u8]> for Hash {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl Hash {
    pub fn into_bytes(self) -> [u8; HASH_LEN] {
        self.0
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl Serialize for Hash {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let hex_str = hex::encode(self.0);
        serializer.serialize_str(&hex_str)
    }
}

impl<'de> Deserialize<'de> for Hash {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(HashVisitor)
    }
}

struct HashVisitor;

impl<'de> Visitor<'de> for HashVisitor {
    type Value = Hash;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a 32-byte array in hex encoding")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        let bytes = hex::decode(v).map_err(|err| E::custom(err))?;
        let bytes: [u8; HASH_LEN] = bytes.as_slice().try_into().map_err(|err| E::custom(err))?;
        Ok(Hash(bytes))
    }
}
