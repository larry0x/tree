use {
    crate::{
        msg::{GetResponse, NodeResponse, OrphanResponse},
        state::{LAST_COMMITTED_VERSION, NODES, ORPHANS},
        types::{NibbleIterator, NibblePath, Node, NodeKey},
    },
    cosmwasm_std::{ensure, Order, StdError, StdResult, Storage},
    cw_storage_plus::Bound,
};

const DEFAULT_LIMIT: u32 = 10;

pub fn version(store: &dyn Storage) -> StdResult<u64> {
    LAST_COMMITTED_VERSION.load(store)
}

pub fn get(store: &dyn Storage, key: String) -> StdResult<GetResponse> {
    let version = LAST_COMMITTED_VERSION.load(store)?;
    let node_key = NodeKey::root(version);

    let key_hash = blake3::hash(key.as_bytes());
    let nibble_path = NibblePath::from(key_hash);

    Ok(GetResponse {
        key,
        value: get_value_at(store, node_key, &mut nibble_path.nibbles())?,
    })
}

fn get_value_at(
    store: &dyn Storage,
    current_node_key: NodeKey,
    nibble_iter: &mut NibbleIterator,
) -> StdResult<Option<String>> {
    let Some(current_node) = NODES.may_load(store, &current_node_key)? else {
        // Node is not found. The only case where this is allowed to happen is
        // if the current node is the root, which means the tree is empty.
        // TODO: use custom error type
        ensure!(current_node_key.nibble_path.is_empty(), StdError::generic_err("node not found"));

        return Ok(None);
    };

    match current_node {
        Node::Internal(internal_node) => {
            let index = nibble_iter.next().unwrap();

            let Some(child) = internal_node.get_child(index) else {
                return Ok(None);
            };

            let child_node_key = NodeKey {
                version: child.version,
                nibble_path: current_node_key.nibble_path.child(index),
            };

            get_value_at(store, child_node_key, nibble_iter)
        },
        Node::Leaf(leaf_node) => Ok(Some(leaf_node.value)),
    }
}

pub fn node(store: &dyn Storage, node_key: NodeKey) -> StdResult<Option<NodeResponse>> {
    Ok(NODES
        .may_load(store, &node_key)?
        .map(|node| NodeResponse {
            node_key,
            node,
        }))
}

pub fn nodes(
    store: &dyn Storage,
    start_after: Option<&NodeKey>,
    limit: Option<u32>,
) -> StdResult<Vec<NodeResponse>> {
    let start = start_after.map(Bound::exclusive);
    let limit = limit.unwrap_or(DEFAULT_LIMIT) as usize;

    NODES
        .range(store, start, None, Order::Ascending)
        .take(limit)
        .map(|item| {
            let (node_key, node) = item?;
            Ok(NodeResponse {
                node_key,
                node,
            })
        })
        .collect()
}

pub fn orphans(
    store: &dyn Storage,
    start_after: Option<&OrphanResponse>,
    limit: Option<u32>,
) -> StdResult<Vec<OrphanResponse>> {
    let start = start_after.map(|o| Bound::exclusive((o.since_version, &o.node_key)));
    let limit = limit.unwrap_or(DEFAULT_LIMIT) as usize;

    ORPHANS
        .items(store, start, None, Order::Ascending)
        .take(limit)
        .map(|item| {
            let (since_version, node_key) = item?;
            Ok(OrphanResponse {
                node_key,
                since_version,
            })
        })
        .collect()
}
