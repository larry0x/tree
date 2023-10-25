use {
    crate::types::{Hash, Nibble},
    cosmwasm_schema::cw_serde,
};

#[cw_serde]
pub struct Sibling {
    pub index: Nibble,
    pub hash: Hash,
}

// this can either be a membership or a non-membership proof
pub type Proof = Vec<Vec<Sibling>>;
