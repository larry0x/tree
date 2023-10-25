use {
    crate::{
        error::{Error, Result},
        msg::{GetResponse, NodeResponse, OrphanResponse, RootResponse},
        state::{LAST_COMMITTED_VERSION, NODES, ORPHANS},
        types::{NibbleIterator, NibblePath, Node, NodeKey},
    },
    cosmwasm_std::{Order, StdResult, Storage},
    cw_storage_plus::Bound,
    std::cmp::Ordering,
};

const DEFAULT_LIMIT: u32 = 10;

/// If the user specifies a version, we use it. Otherwise, load the latest version.
fn unwrap_version(store: &dyn Storage, version: Option<u64>) -> StdResult<u64> {
    if let Some(version) = version {
        Ok(version)
    } else {
        LAST_COMMITTED_VERSION.load(store)
    }
}

pub fn root(store: &dyn Storage, version: Option<u64>) -> Result<RootResponse> {
    let version = unwrap_version(store, version)?;

    let root_node_key = NodeKey {
        version,
        nibble_path: NibblePath::empty(),
    };

    let Some(root_node) = NODES.may_load(store, &root_node_key)? else {
        return Err(Error::RootNodeNotFound { version });
    };

    Ok(RootResponse {
        version,
        root_hash: root_node.hash(),
    })
}

pub fn get(
    store: &dyn Storage,
    key: String,
    prove: bool,
    version: Option<u64>,
) -> Result<GetResponse> {
    let version = unwrap_version(store, version)?;
    let node_key = NodeKey::root(version);
    let nibble_path = NibblePath::from(key.as_bytes().to_vec());

    Ok(GetResponse {
        key,
        value: get_value_at(store, node_key, &mut nibble_path.nibbles())?,
        proof: None, // TODO
    })
}

fn get_value_at(
    store: &dyn Storage,
    current_node_key: NodeKey,
    nibble_iter: &mut NibbleIterator,
) -> Result<Option<String>> {
    let Some(current_node) = NODES.may_load(store, &current_node_key)? else {
        // Node is not found. There are a few circumstances:
        // - if the node is the root,
        //   - and it's older than the latest version: it may simply be that
        //     that version has been pruned
        //   - and it's the current version: it may simply be that the current
        //     tree is empty
        //   - and it's newer than the latest version: this query is illegal
        // - if the node is not the root: database corrupted
        if current_node_key.nibble_path.is_empty() {
            let latest_version = LAST_COMMITTED_VERSION.load(store)?;
            return match current_node_key.version.cmp(&latest_version) {
                Ordering::Equal => {
                    Ok(None)
                },
                Ordering::Less => {
                    Err(Error::RootNodeNotFound {
                        version: current_node_key.version,
                    })
                },
                Ordering::Greater => {
                    Err(Error::VersionNewerThanLatest {
                        latest: latest_version,
                        querying: current_node_key.version,
                    })
                },
            };
        } else {
            return Err(Error::NonRootNodeNotFound { node_key: current_node_key });
        }
    };

    match current_node {
        Node::Internal(internal_node) => {
            let index = nibble_iter.next().unwrap();

            let Some(child) = internal_node.children.get(index) else {
                return Ok(None);
            };

            let child_node_key = NodeKey {
                version: child.version,
                nibble_path: current_node_key.nibble_path.child(index),
            };

            get_value_at(store, child_node_key, nibble_iter)
        },
        Node::Leaf(leaf_node) => {
            // TODO: impl PartialEq to prettify this syntax
            if leaf_node.key.into_bytes().as_ref() == nibble_iter.nibble_path().bytes {
                return Ok(Some(leaf_node.value))
            }

            Ok(None)
        },
    }
}

pub fn node(store: &dyn Storage, node_key: NodeKey) -> Result<Option<NodeResponse>> {
    Ok(NODES
        .may_load(store, &node_key)?
        .map(|node| NodeResponse {
            node_key,
            hash: node.hash(),
            node,
        }))
}

pub fn nodes(
    store: &dyn Storage,
    start_after: Option<&NodeKey>,
    limit: Option<u32>,
) -> Result<Vec<NodeResponse>> {
    let start = start_after.map(Bound::exclusive);
    let limit = limit.unwrap_or(DEFAULT_LIMIT) as usize;

    NODES
        .range(store, start, None, Order::Ascending)
        .take(limit)
        .map(|item| {
            let (node_key, node) = item?;
            Ok(NodeResponse {
                node_key,
                hash: node.hash(),
                node,
            })
        })
        .collect()
}

pub fn orphans(
    store: &dyn Storage,
    start_after: Option<&OrphanResponse>,
    limit: Option<u32>,
) -> Result<Vec<OrphanResponse>> {
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
