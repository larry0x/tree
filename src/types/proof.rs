use cosmwasm_schema::cw_serde;

#[cw_serde]
pub enum Proof {
    Existence(/* TODO */),
    NonExistence(/* TODO */),
}
