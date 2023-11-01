use {
    crate::Nibble,
    cosmwasm_std::{ensure, ensure_eq, StdError, StdResult},
    cw_storage_plus::KeyDeserialize,
    hex::FromHexError,
    schemars::JsonSchema,
    serde::{
        de::{self, Deserialize, Deserializer, Visitor},
        ser::{Serialize, Serializer},
    },
    std::{any::type_name, cmp::Ordering, fmt, ops::Range},
};

#[derive(Clone, PartialEq, Eq, Hash, JsonSchema)]
pub struct NibblePath {
    pub num_nibbles: usize,
    pub bytes: Vec<u8>,
}

impl NibblePath {
    pub fn empty() -> Self {
        Self {
            num_nibbles: 0,
            bytes: vec![],
        }
    }

    pub fn is_empty(&self) -> bool {
        self.num_nibbles == 0
    }

    pub fn child(&self, index: Nibble) -> Self {
        let mut nibble_path = self.clone();
        nibble_path.push(index);
        nibble_path
    }

    pub fn push(&mut self, nibble: Nibble) {
        if self.num_nibbles % 2 == 0 {
            self.bytes.push(nibble.byte() << 4);
        } else {
            self.bytes[self.num_nibbles / 2] |= nibble.byte();
        }

        self.num_nibbles += 1;
    }

    pub fn pop(&mut self) -> Option<Nibble> {
        let popped_byte = if self.num_nibbles % 2 == 0 {
            self.bytes.last_mut().map(|byte| {
                let nibble = (*byte) & 0x0f;
                (*byte) &= 0xf0;
                nibble
            })
        } else {
            self.bytes.pop().map(|byte| byte >> 4)
        };

        if popped_byte.is_some() {
            self.num_nibbles -= 1;
        }

        popped_byte.map(Nibble::new)
    }

    /// Return a new NibblePath whose length is at most `n`. Nibbles above `n`
    /// are dropped. Only works if `n` < current length.
    pub fn crop(&self, n: usize) -> Self {
        assert!(n < self.num_nibbles);

        let mut bytes = self.bytes[..(n / 2)].to_vec();
        if n % 2 != 0 {
            bytes.push(self.bytes[n / 2] & 0xf0);
        }

        Self {
            num_nibbles: n,
            bytes,
        }
    }

    // panics if index is out of range
    pub fn get_nibble(&self, i: usize) -> Nibble {
        assert!(i < self.num_nibbles);
        Nibble::new((self.bytes[i / 2] >> (if i % 2 == 1 { 0 } else { 4 })) & 0xf)
    }

    pub fn nibbles(&self) -> NibbleIterator {
        NibbleIterator::new(self, 0, self.num_nibbles)
    }

    pub fn to_hex(&self) -> String {
        let mut hex_str = hex::encode(&self.bytes);
        if self.num_nibbles % 2 != 0 {
            hex_str.pop();
        }
        hex_str
    }

    pub fn from_hex(mut hex_str: String) -> Result<Self, FromHexError> {
        let num_nibbles = hex_str.len();

        if num_nibbles % 2 != 0 {
            hex_str.push('0');
        }

        let bytes = hex::decode(hex_str)?;

        Ok(NibblePath {
            num_nibbles,
            bytes,
        })
    }
}

impl fmt::Debug for NibblePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NibblePath(\"{}\")", self.to_hex())
    }
}

impl FromIterator<Nibble> for NibblePath {
    fn from_iter<T: IntoIterator<Item = Nibble>>(iter: T) -> Self {
        let mut nibble_path = NibblePath::empty();
        for nibble in iter {
            nibble_path.push(nibble);
        }
        nibble_path
    }
}

// this allows any type that can be referenced raw bytes to be converted to
// NibblePath. examples include Vec<u8>, str, String, and blake3::Hash.
impl<T: AsRef<[u8]>> From<T> for NibblePath {
    fn from(value: T) -> Self {
        Self {
            num_nibbles: value.as_ref().len() * 2,
            bytes: value.as_ref().to_vec(),
        }
    }
}

impl PartialOrd for NibblePath {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for NibblePath {
    fn cmp(&self, other: &Self) -> Ordering {
        // if the bytes are not the same, then we simply compare the types
        // otherwise, we additionally compare the num_nibbles
        match self.bytes.cmp(&other.bytes) {
            Ordering::Less => Ordering::Less,
            Ordering::Greater => Ordering::Greater,
            Ordering::Equal => self.num_nibbles.cmp(&other.num_nibbles),
        }
    }
}

impl Serialize for NibblePath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let hex_str = self.to_hex();
        serializer.serialize_str(&hex_str)
    }
}

impl<'de> Deserialize<'de> for NibblePath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(NibblePathVisitor)
    }
}

struct NibblePathVisitor;

impl<'de> Visitor<'de> for NibblePathVisitor {
    type Value = NibblePath;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a hex-encoded string")
    }

    // clippy complains if I only implement visit_string but not visit_str
    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.visit_string(v.into())
    }

    fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        NibblePath::from_hex(v).map_err(|err| E::custom(err))
    }
}

impl KeyDeserialize for NibblePath {
    type Output = NibblePath;

    fn from_vec(value: Vec<u8>) -> StdResult<Self::Output> {
        ensure!(
            !value.is_empty(),
            StdError::parse_err(type_name::<Self::Output>(), "raw key must have at least 1 byte")
        );

        let num_nibbles = u16::from_be_bytes(value[..2].try_into().unwrap()) as usize;
        let bytes = value[2..].to_vec();

        ensure_eq!(
            bytes.len(),
            num_nibbles / 2 + num_nibbles % 2,
            StdError::parse_err(
                type_name::<Self::Output>(),
                "num_nibbles and bytes length don't match"
            )
        );

        Ok(NibblePath {
            num_nibbles,
            bytes,
        })
    }
}

#[derive(Debug)]
pub struct NibbleIterator<'a> {
    nibble_path: &'a NibblePath,
    pos: Range<usize>,
    start: usize,
}

impl<'a> Iterator for NibbleIterator<'a> {
    type Item = Nibble;

    fn next(&mut self) -> Option<Self::Item> {
        self.pos.next().map(|i| self.nibble_path.get_nibble(i))
    }
}

impl<'a> NibbleIterator<'a> {
    pub fn new(nibble_path: &'a NibblePath, start: usize, end: usize) -> Self {
        Self {
            nibble_path,
            pos: (start..end),
            start,
        }
    }

    /// Returns the `next()` value without advancing the iterator.
    /// TODO: can we replace this with the std Peekable type?
    pub fn peek(&self) -> Option<Nibble> {
        if self.pos.start < self.pos.end {
            Some(self.nibble_path.get_nibble(self.pos.start))
        } else {
            None
        }
    }

    pub fn finished(&self) -> bool {
        self.peek().is_none()
    }

    pub fn visited_nibbles(&self) -> NibbleIterator<'a> {
        Self::new(self.nibble_path, self.start, self.pos.start)
    }

    pub fn remaining_nibbles(&self) -> NibbleIterator<'a> {
        Self::new(self.nibble_path, self.pos.start, self.pos.end)
    }

    pub fn nibble_path(&self) -> NibblePath {
        self.visited_nibbles().chain(self.remaining_nibbles()).collect()
    }
}
