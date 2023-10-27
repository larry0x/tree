use {
    crate::{
        Child, Children, GetResponse, Hash, NibbleIterator, NibblePath, Node, NodeData, NodeKey,
        NodeResponse, Op, OrphanResponse, RootResponse, Set,
    },
    cosmwasm_std::{ensure, Order, Response, StdResult, Storage},
    cw_storage_plus::{Bound, Item, Map, PrefixBound},
    std::{cmp::Ordering, collections::BTreeMap},
};

const LAST_COMMITTED_VERSION: Item<u64>            = Item::new("v");
const NODES:                  Map<&NodeKey, Node>  = Map::new("n");
const ORPHANS:                Set<(u64, &NodeKey)> = Set::new("o");

const DEFAULT_QUERY_BATCH_SIZE: usize = 10;
const PRUNE_BATCH_SIZE:         usize = 10;

pub struct Tree<S> {
    store: S,
}

impl<S> Tree<S>
where
    S: Storage,
{
    pub fn new(store: S) -> Self {
        Self { store }
    }

    pub fn initialize(&mut self) -> Result<()> {
        // initialize version as zero
        LAST_COMMITTED_VERSION.save(&mut self.store, &0)?;

        Ok(())
    }

    pub fn apply(&mut self, batch: BTreeMap<String, Op>) -> Result<Response> {
        todo!();
    }

    pub fn prune(&mut self, up_to_version: Option<u64>) -> Result<Response> {
        let end = up_to_version.map(PrefixBound::inclusive);

        loop {
            let batch = ORPHANS
                .prefix_range(&self.store, None, end.clone(), Order::Ascending)
                .take(PRUNE_BATCH_SIZE)
                .collect::<StdResult<Vec<_>>>()?;

            for (stale_since_version, node_key) in &batch {
                NODES.remove(&mut self.store, node_key);
                ORPHANS.remove(&mut self.store, (*stale_since_version, node_key));
            }

            if batch.len() < PRUNE_BATCH_SIZE {
                break;
            }
        }

        Ok(Response::new())
    }

    fn mark_node_as_orphaned(
        &mut self,
        orphaned_since_version: u64,
        node_key: &NodeKey,
    ) -> StdResult<()> {
        ORPHANS.insert(&mut self.store, (orphaned_since_version, node_key))
    }

    fn increment_version(&mut self) -> StdResult<u64> {
        LAST_COMMITTED_VERSION.update(&mut self.store, |version| Ok(version + 1))
    }

    pub fn root(&self, version: Option<u64>) -> Result<RootResponse> {
        let version = unwrap_version(&self.store, version)?;

        let root_node_key = NodeKey {
            version,
            nibble_path: NibblePath::empty(),
        };

        let Some(root_node) = NODES.may_load(&self.store, &root_node_key)? else {
            return Err(TreeError::RootNodeNotFound { version });
        };

        Ok(RootResponse {
            version,
            root_hash: root_node.hash(),
        })
    }

    pub fn get(&self, key: String, _prove: bool, version: Option<u64>) -> Result<GetResponse> {
        let version = unwrap_version(&self.store, version)?;
        let node_key = NodeKey::root(version);
        let nibble_path = NibblePath::from(key.as_bytes().to_vec());

        Ok(GetResponse {
            key,
            value: self.get_at(node_key, &mut nibble_path.nibbles())?,
            proof: None, // TODO
        })
    }

    fn get_at(
        &self,
        current_node_key: NodeKey,
        nibble_iter: &mut NibbleIterator,
    ) -> Result<Option<String>> {
        let Some(current_node) = NODES.may_load(&self.store, &current_node_key)? else {
            // Node is not found. There are a few circumstances:
            // - if the node is the root,
            //   - and it's older than the latest version: it may simply be that
            //     that version has been pruned
            //   - and it's the current version: it may simply be that the current
            //     tree is empty
            //   - and it's newer than the latest version: this query is illegal
            // - if the node is not the root: database corrupted
            if current_node_key.nibble_path.is_empty() {
                let latest_version = LAST_COMMITTED_VERSION.load(&self.store)?;
                return match current_node_key.version.cmp(&latest_version) {
                    Ordering::Equal => {
                        Ok(None)
                    },
                    Ordering::Less => {
                        Err(TreeError::RootNodeNotFound {
                            version: current_node_key.version,
                        })
                    },
                    Ordering::Greater => {
                        Err(TreeError::VersionNewerThanLatest {
                            latest: latest_version,
                            querying: current_node_key.version,
                        })
                    },
                };
            } else {
                return Err(TreeError::NonRootNodeNotFound { node_key: current_node_key });
            }
        };

        // if the node has data and the key matches the request key, then we
        // have found it
        if let Some(NodeData { key, value }) = current_node.data {
            if key.as_bytes() == nibble_iter.nibble_path().bytes {
                return Ok(Some(value));
            }
        }

        // otherwise, if we have already reached the last nibble, then key is
        // not found
        let Some(index) = nibble_iter.next() else {
            return Ok(None);
        };

        // if there're still more nibbles, but the current node doesn't have the
        // corresponding child, then key is not found
        let Some(child) = current_node.children.get(index) else {
            return Ok(None);
        };

        self.get_at(current_node_key.child(child.version, index), nibble_iter)
    }

    pub fn node(&self, node_key: NodeKey) -> Result<Option<NodeResponse>> {
        Ok(NODES
            .may_load(&self.store, &node_key)?
            .map(|node| NodeResponse {
                node_key,
                hash: node.hash(),
                node,
            }))
    }

    pub fn nodes(
        &self,
        start_after: Option<&NodeKey>,
        limit: Option<usize>,
    ) -> Result<Vec<NodeResponse>> {
        let start = start_after.map(Bound::exclusive);
        let limit = limit.unwrap_or(DEFAULT_QUERY_BATCH_SIZE);

        NODES
            .range(&self.store, start, None, Order::Ascending)
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
        &self,
        start_after: Option<&OrphanResponse>,
        limit: Option<usize>,
    ) -> Result<Vec<OrphanResponse>> {
        let start = start_after.map(|o| Bound::exclusive((o.since_version, &o.node_key)));
        let limit = limit.unwrap_or(DEFAULT_QUERY_BATCH_SIZE);

        ORPHANS
            .items(&self.store, start, None, Order::Ascending)
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
}

/// If the user specifies a version, we use it. Otherwise, load the latest version.
fn unwrap_version(store: &dyn Storage, version: Option<u64>) -> StdResult<u64> {
    if let Some(version) = version {
        Ok(version)
    } else {
        LAST_COMMITTED_VERSION.load(store)
    }
}

#[derive(Debug, PartialEq, thiserror::Error)]
pub enum TreeError {
    #[error(transparent)]
    Std(#[from] cosmwasm_std::StdError),

    #[error("cannot query at version {querying} which is newer than the latest ({latest})")]
    VersionNewerThanLatest {
        latest: u64,
        querying: u64,
    },

    #[error("root node of version {version} not found, probably pruned")]
    RootNodeNotFound {
        version: u64,
    },

    #[error(
        "tree corrupted! non-root node not found (version: {}, nibble_path: {})",
        node_key.version,
        node_key.nibble_path.to_hex(),
    )]
    NonRootNodeNotFound {
        node_key: NodeKey,
    },
}

type Result<T> = std::result::Result<T, TreeError>;
