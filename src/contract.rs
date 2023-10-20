use {
    crate::{
        error::Result,
        execute,
        msg::{ExecuteMsg, InstantiateMsg, QueryMsg},
        query,
    },
    cosmwasm_std::{entry_point, to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response},
};

#[entry_point]
pub fn instantiate(deps: DepsMut, _: Env, _: MessageInfo, _: InstantiateMsg) -> Result<Response> {
    execute::init(deps.storage)
}

#[entry_point]
pub fn execute(deps: DepsMut, _: Env, _: MessageInfo, msg: ExecuteMsg) -> Result<Response> {
    match msg {
        ExecuteMsg::Insert {
            key,
            value,
        } => execute::insert(deps.storage, key, value),
        ExecuteMsg::Delete {
            key,
        } => todo!(),
        ExecuteMsg::Prune {
            up_to_version,
        } => todo!(),
    }
}

#[entry_point]
pub fn query(deps: Deps, _: Env, msg: QueryMsg) -> Result<Binary> {
    match msg {
        QueryMsg::Version {} => to_binary(&query::version(deps.storage)?),
        QueryMsg::Get {
            key,
        } => to_binary(&query::get(deps.storage, key)?),
        QueryMsg::Node {
            node_key,
        } => to_binary(&query::node(deps.storage, node_key)?),
        QueryMsg::Nodes {
            start_after,
            limit,
        } => to_binary(&query::nodes(deps.storage, start_after.as_ref(), limit)?),
    }
    .map_err(Into::into)
}
