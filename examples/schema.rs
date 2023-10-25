use {
    cosmwasm_schema::write_api,
    tree::msg::{ExecuteMsg, InstantiateMsg, QueryMsg},
};

fn main() {
    write_api! {
        instantiate: InstantiateMsg,
        query: QueryMsg,
        execute: ExecuteMsg,
    };
}
