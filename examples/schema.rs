use {
    cosmwasm_schema::write_api,
    cw_jellyfish_merkle::msg::{ExecuteMsg, InstantiateMsg, QueryMsg},
};

fn main() {
    write_api! {
        instantiate: InstantiateMsg,
        query: QueryMsg,
        execute: ExecuteMsg,
    };
}
