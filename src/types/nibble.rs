use {
    schemars::JsonSchema,
    serde::{
        de::{self, Deserialize, Deserializer, Visitor},
        ser::{Serialize, Serializer},
    },
    std::fmt,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, JsonSchema)]
pub struct Nibble(u8);

impl Nibble {
    pub fn byte(self) -> u8 {
        self.0
    }
}

impl From<u8> for Nibble {
    fn from(byte: u8) -> Self {
        if byte > 0x0f {
            panic!("nibble value cannot be greater than 0x0f");
        }

        Self(byte)
    }
}

impl Serialize for Nibble {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let hex_str = format!("{:x}", self.0);
        serializer.serialize_str(&hex_str)
    }
}

impl<'de> Deserialize<'de> for Nibble {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(NibbleVisitor)
    }
}

struct NibbleVisitor;

impl<'de> Visitor<'de> for NibbleVisitor {
    type Value = Nibble;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a single hex character")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        let hex_str = format!("0{v}");
        let bytes = hex::decode(hex_str).map_err(|err| E::custom(err))?;
        Ok(Nibble(bytes[0]))
    }
}
