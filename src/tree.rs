use {
    crate::{
        Child, GetResponse, Nibble, NibbleIterator, NibblePath, NibbleRange, NibbleRangeIterator,
        Node, Record, NodeKey, NodeResponse, Op, OpResponse, OrphanResponse, Proof, ProofNode,
        RootResponse, Set,
    },
    cosmwasm_std::{to_binary, Order, StdResult, Storage},
    cw_storage_plus::{Bound, Item, Map, PrefixBound},
    std::{
        cmp::Ordering,
        collections::{BTreeMap, HashMap},
    },
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

    /// Apply a batch of ops to the tree. Each op can be either 1) inserting a
    /// value at a key, or 2) deleting a key.
    ///
    /// This method requires that ops in the batch must be sorted ascendingly by
    /// the keys, which is why we require it be a BTreeMap.
    ///
    /// For use in blockchains, typically it works like this:
    /// - The tree persists the state of last committed block.
    /// - The chain maintains an in-memory write batch; while executing
    ///   transactions in a new block, state changes are recorded in that batch.
    /// - If the block is ready to be committed, the chain calls this `apply`
    ///   method to write the changes to to disk, while resetting its in-memory
    ///   to empty, getting ready for the next block.
    ///
    /// Note: keys must not be empty, but we don't assert it here.
    pub fn apply(&mut self, batch: BTreeMap<String, Op>) -> Result<()> {
        let (old_version, new_version) = self.increment_version()?;
        let old_root_key = NodeKey::root(old_version);

        // collect the batch into a sorted Vec, also converting the string keys
        // to NibblePaths
        let batch = batch
            .into_iter()
            .map(|(key, op)| {
                let nibble_path = NibblePath::from(key.as_bytes().to_vec());
                (nibble_path, op)
            })
            .collect::<Vec<_>>();

        // recursively apply the batch, starting from the root (depth = 0)
        match self.apply_at(new_version, &old_root_key, batch.as_slice())? {
            OpResponse::Updated(updated_root_node) => {
                self.create_node(new_version, NibblePath::empty(), &updated_root_node)?;
                if old_version > 0 {
                    self.mark_node_as_orphaned(new_version, &old_root_key)?;
                }
            },
            OpResponse::Deleted => {
                if old_version > 0 {
                    self.mark_node_as_orphaned(new_version, &old_root_key)?;
                }
            },
            OpResponse::Unchanged => (),
        }

        Ok(())
    }

    fn apply_at(
        &mut self,
        version: u64,
        current_node_key: &NodeKey,
        batch: &[(NibblePath, Op)],
    ) -> Result<OpResponse> {
        // attempt to load the node. if not found, we simply create a new empty
        // node (no children, no data)
        let current_node_before = NODES.may_load(&self.store, current_node_key)?.unwrap_or_else(Node::new);

        // make a mutable clone of the current node. after we've executed the
        // ops, we will compare with the original whether it has been changed
        let mut current_node = current_node_before.clone();

        // a cache of the current node's children that have been changed.
        // we don't want to write these nodes to store immediately, because if
        // the current node ends up having only one child, we will need to
        // collapse the path (i.e. delete the current node, move the only child
        // one level up)
        let mut updated_child_nodes = HashMap::new();

        // if there is only one item in the batch AND one of the following is
        // satisfied, then we apply the op at the current node:
        // - the current node's nibble path matches exactly the nibble path we
        //   want to write to
        // - the current node is a leaf, and the key matches exactly the nibble
        //   path we want to write to
        // - the current node has neither any child nor data
        //
        // if this condition is not satisfied, we need to dispatch the ops to
        // the current node's children.
        if batch.len() == 1 && execute_op_at_node(current_node_key, &current_node, &batch[0].0) {
            let (nibble_path, op) = batch[0].clone();
            current_node.data = match op {
                Op::Insert(value) => {
                    Some(Record {
                        key: String::from_utf8(nibble_path.bytes.clone()).unwrap(),
                        value,
                    })
                },
                Op::Delete => None,
            };
        } else {
            let nibble_range_iter = NibbleRangeIterator::new(batch, current_node_key.depth());
            for NibbleRange { nibble, start, end } in nibble_range_iter {
                self.apply_at_index(
                    version,
                    current_node_key,
                    &mut current_node,
                    nibble,
                    &batch[start..=end],
                    &mut updated_child_nodes,
                )?;
            }
        }

        // Now that we have finished executing the ops, we need to look at a
        // complexity not present in Ethereum's Patricia Merkle tree (PMT) or
        // Diem's Jellyfish Merkle tree (JMT). That is, our tree's internal
        // nodes may have data too. In contrary, in PMT/JMT, data are only found
        // in leaf nodes.
        //
        // The complexity is this: if the current node had previously been a
        // leaf node (has data but no children), but after applying the ops now
        // it has children, then the data may needs to be moved down the tree to
        // a new leaf node.
        if let Some(Record { key, value }) = current_node.data.clone() {
            if !current_node.children.is_empty() && key.as_bytes() != current_node_key.nibble_path.bytes {
                current_node.data = None;
                let nibble_path = NibblePath::from(key.as_bytes().to_vec());
                let nibble = nibble_path.get_nibble(current_node_key.depth());
                self.apply_at_index(
                    version,
                    current_node_key,
                    &mut current_node,
                    nibble,
                    &[(nibble_path, Op::Insert(value.clone()))],
                    &mut updated_child_nodes,
                )?;
            }
        }

        // finally, everything is done with the current node, we need to reply
        // the outcome to our parent node. the rules are:
        // - if the current node has neither any child nor data, then it should
        //   be deleted
        // - if the current node has no data and exactly 1 child, and this child
        //   is a leaf node, then the path can be collapsed (i.e. the current
        //   node deleted, and that child leaf node moved on level up)
        // - if the current node has been updated, we pass the updated node to
        //   the parent who will recompute the hash
        // - if the current node has NOT been changed, we inform the parent node
        //   about this so it doesn't need to recompute the hash
        if current_node.data.is_none() {
            if current_node.children.is_empty() {
                return Ok(OpResponse::Deleted);
            }

            if let Some(child) = current_node.children.get_only() {
                // the current node has only 1 child. this child may have just
                // been updated, in which case it should be in the `updated_child_nodes`
                // map, or not updated, in which case it needs to be loaded from
                // the store
                if let Some(child_node) = updated_child_nodes.remove(&child.index) {
                    if child_node.is_leaf() {
                        return Ok(OpResponse::Updated(child_node));
                    }
                } else {
                    let child_node_key = current_node_key.child(child.version, child.index);
                    let child_node = NODES.load(&self.store, &child_node_key)?;
                    if child_node.is_leaf() {
                        self.mark_node_as_orphaned(version, &child_node_key)?;
                        return Ok(OpResponse::Updated(child_node));
                    }
                };
            } else {
                // now we know the current node won't be deleted or collapsed,
                // we can write the updated child nodes
                for (nibble, node) in updated_child_nodes {
                    let nibble_path = current_node_key.nibble_path.child(nibble);
                    self.create_node(version, nibble_path, &node)?;
                }
            }
        }

        if current_node != current_node_before {
            return Ok(OpResponse::Updated(current_node));
        }

        Ok(OpResponse::Unchanged)
    }

    fn apply_at_index(
        &mut self,
        version: u64,
        current_node_key: &NodeKey,
        current_node: &mut Node,
        index: Nibble,
        batch: &[(NibblePath, Op)],
        updated_child_nodes: &mut HashMap<Nibble, Node>,
    ) -> Result<()> {
        let child = current_node.children.get(index);
        let child_version = child.map(|c| c.version).unwrap_or(version);
        let child_node_key = current_node_key.child(child_version, index);

        match self.apply_at(version, &child_node_key, batch)? {
            OpResponse::Updated(updated_child_node) => {
                current_node.children.insert(Child {
                    index,
                    version,
                    hash: updated_child_node.hash(),
                });

                if child_node_key.version < version {
                    self.mark_node_as_orphaned(version, &child_node_key)?;
                }

                updated_child_nodes.insert(index, updated_child_node);
            },
            OpResponse::Deleted => {
                current_node.children.remove(index);
                if child_node_key.version < version {
                    self.mark_node_as_orphaned(version, &child_node_key)?;
                }
            },
            OpResponse::Unchanged => (),
        }

        Ok(())
    }

    pub fn prune(&mut self, up_to_version: Option<u64>) -> Result<()> {
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

        Ok(())
    }

    fn create_node(&mut self, version: u64, nibble_path: NibblePath, node: &Node) -> StdResult<()> {
        NODES.save(&mut self.store, &NodeKey::new(version, nibble_path), node)
    }

    fn mark_node_as_orphaned(
        &mut self,
        orphaned_since_version: u64,
        node_key: &NodeKey,
    ) -> StdResult<()> {
        ORPHANS.insert(&mut self.store, (orphaned_since_version, node_key))
    }

    fn increment_version(&mut self) -> StdResult<(u64, u64)> {
        let old_version = LAST_COMMITTED_VERSION.load(&self.store)?;
        let new_version = old_version + 1;
        LAST_COMMITTED_VERSION.save(&mut self.store, &new_version)?;

        Ok((old_version, new_version))
    }

    pub fn root(&self, version: Option<u64>) -> Result<RootResponse> {
        let version = unwrap_version(&self.store, version)?;
        let root_node_key = NodeKey::root(version);
        let Some(root_node) = NODES.may_load(&self.store, &root_node_key)? else {
            return Err(TreeError::RootNodeNotFound { version });
        };

        Ok(RootResponse {
            version,
            root_hash: root_node.hash(),
        })
    }

    pub fn get(&self, key: String, prove: bool, version: Option<u64>) -> Result<GetResponse> {
        let version = unwrap_version(&self.store, version)?;
        let nibble_path = NibblePath::from(key.as_bytes().to_vec());

        let (value, proof) = self.get_at(
            NodeKey::root(version),
            &mut nibble_path.nibbles(),
            prove,
        )?;

        let proof = if prove {
            Some(to_binary(&proof)?)
        } else {
            None
        };

        Ok(GetResponse { key, value, proof })
    }

    fn get_at(
        &self,
        current_node_key: NodeKey,
        nibble_iter: &mut NibbleIterator,
        prove: bool,
    ) -> Result<(Option<String>, Proof)> {
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
                        Ok((None, vec![]))
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
        if let Some(Record { key, value }) = current_node.data.clone() {
            if key.as_bytes() == nibble_iter.nibble_path().bytes {
                let proof = if prove {
                    vec![ProofNode::from_node(current_node.clone(), None, true)]
                } else {
                    vec![]
                };
                return Ok((Some(value), proof));
            }
        }

        // otherwise, if we have already reached the last nibble, then key is
        // not found
        let Some(index) = nibble_iter.next() else {
            let proof = if prove {
                vec![ProofNode::from_node(current_node, None, false)]
            } else {
                vec![]
            };
            return Ok((None, proof));
        };

        // if there're still more nibbles, but the current node doesn't have the
        // corresponding child, then key is not found
        let Some(child) = current_node.children.get(index) else {
            let proof = if prove {
                vec![ProofNode::from_node(current_node, None, false)]
            } else {
                vec![]
            };
            return Ok((None, proof));
        };

        let (value, mut proof) = self.get_at(
            current_node_key.child(child.version, index),
            nibble_iter,
            prove,
        )?;

        if prove {
            proof.push(ProofNode::from_node(current_node, Some(index), false));
        }

        Ok((value, proof))
    }

    /// This function signature is inspired by `cosmwasm_std::Storage` trait's
    /// `range` method.
    ///
    /// Notes:
    /// - The bound `start` is inclusive and `end` is exclusive;
    /// - `start` should be lexicographically smaller than `end`, regardless of
    ///   the iteration `order`. If `start` >= `end`, an empty iterator is
    ///   generated.
    pub fn iterate<'a>(
        &'a self,
        order: Order,
        min: Option<&str>,
        max: Option<&str>,
        version: Option<u64>,
    ) -> Result<TreeIterator<'a, S>> {
        let version = unwrap_version(&self.store, version)?;
        let root_node_key = NodeKey::root(version);
        let Some(root_node) = NODES.may_load(&self.store, &root_node_key)? else {
            return Err(TreeError::RootNodeNotFound { version });
        };

        Ok(TreeIterator::new(&self.store, order, min, max, root_node))
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

pub struct TreeIterator<'a, S> {
    store: &'a S,
    order: Order,
    min: Option<NibblePath>,
    max: Option<NibblePath>,
    visited_nibbles: NibblePath,
    visited_nodes: Vec<Node>,
}

impl<'a, S> TreeIterator<'a, S> {
    pub fn new(
        store: &'a S,
        order: Order,
        min: Option<&str>,
        max: Option<&str>,
        root_node: Node,
    ) -> Self {
        Self {
            store,
            order,
            min: min.map(|s| NibblePath::from(s.as_bytes().to_vec())),
            max: max.map(|s| NibblePath::from(s.as_bytes().to_vec())),
            visited_nibbles: NibblePath::empty(),
            visited_nodes: vec![root_node],
        }
    }
}

impl<'a, S> Iterator for TreeIterator<'a, S>
where
    S: Storage,
{
    type Item = (String, String);

    fn next(&mut self) -> Option<Self::Item> {
        iterate_at(
            self.store,
            self.order,
            self.min.as_ref(),
            self.max.as_ref(),
            &mut self.visited_nibbles,
            &mut self.visited_nodes,
            None,
        )
    }
}

fn iterate_at(
    store: &dyn Storage,
    order: Order,
    min: Option<&NibblePath>,
    max: Option<&NibblePath>,
    visited_nibbles: &mut NibblePath,
    visited_nodes: &mut Vec<Node>,
    start_after_index: Option<Nibble>,
) -> Option<(String, String)> {
    // TODO: avoid the cloning here
    let Some(current_node) = visited_nodes.last().cloned() else {
        return None;
    };

    // going through the node's children. pushing the first one that's in
    // the range into the stack
    for child in iter_with_order(current_node.children, order) {
        if skip(child.index, start_after_index, order) {
            continue;
        }

        let child_nibble_path = visited_nibbles.child(child.index);
        if !nibbles_in_range(&child_nibble_path, min, max) {
            continue;
        }

        let child_node_key = NodeKey::new(child.version, child_nibble_path);
        let child_node = NODES.load(store, &child_node_key).unwrap(); // TODO

        visited_nibbles.push(child.index);
        visited_nodes.push(child_node.clone());

        // if the child node has data, and the key is in range, then we stop the
        // recursion and return this data
        if let Some(Record { key, value }) = child_node.data {
            // we only need to compare the key with the min. we don't need to
            // compare with the max because any key greater than max should have
            // already been dropped when comparing `nibbles_in_range`
            if key_in_range(&key, min) {
                return Some((key, value));
            }
        }

        // if the current node has no data, then we do a depth-first search,
        // exploring the children of this child
        if let Some(record) = iterate_at(store, order, min, max, visited_nibbles, visited_nodes, None) {
            return Some(record);
        }
    }

    // now we've gone over all the childs of the current node, and still hasn't
    // returned. this means there is no data found in any of the subtrees below
    // the current node. we need to go up one level and search in the siblings.
    let (Some(index), _) = (visited_nibbles.pop(), visited_nodes.pop()) else {
        return None;
    };

    iterate_at(store, order, min, max, visited_nibbles, visited_nodes, Some(index))
}

fn iter_with_order<'a, I>(items: I, order: Order) -> Box<dyn Iterator<Item = I::Item> + 'a>
where
    I: IntoIterator,
    I::IntoIter: DoubleEndedIterator + 'a,
{
    match order {
        Order::Ascending => Box::new(items.into_iter()),
        Order::Descending => Box::new(items.into_iter().rev()),
    }
}

fn skip(index: Nibble, start_after: Option<Nibble>, order: Order) -> bool {
    let Some(after) = start_after else {
        return false;
    };

    match order {
        Order::Ascending => index <= after,
        Order::Descending => index >= after,
    }
}

fn nibbles_in_range(
    nibble_path: &NibblePath,
    min: Option<&NibblePath>,
    max: Option<&NibblePath>,
) -> bool {
    // the min bound is a bit complex
    //
    // for example, if nibble_path = [12], min = [12345]
    // if we just compare the bytes, we get nibble_path < min and thus out of
    // range. however we don't want to discard this nibble_path just yet,
    // because if we go down the tree, we may find a node >= min
    //
    // to fix this, we crop min to the same length as nibble_path and then do
    // the comparison
    if let Some(min) = min {
        println!("comparing nibble_path={nibble_path:?} with min_cropped={:?}", min.crop(nibble_path.num_nibbles));
        if nibble_path.bytes < min.crop(nibble_path.num_nibbles).bytes {
            return false;
        }
    }

    // the max bound is simpler, we just compare the bytes
    if let Some(max) = max {
        if nibble_path.bytes >= max.bytes {
            return false;
        }
    }

    true
}

fn key_in_range(key: &str, min: Option<&NibblePath>) -> bool {
    if let Some(min) = min {
        if key.as_bytes() < min.bytes.as_slice() {
            return false;
        }
    }

    true
}

fn execute_op_at_node(node_key: &NodeKey, node: &Node, nibble_path: &NibblePath) -> bool {
    if &node_key.nibble_path == nibble_path {
        return true;
    }

    if let Some(Record { key, .. }) = &node.data {
        if key.as_bytes() == nibble_path.bytes {
            return true;
        }
    }

    if node.is_empty() {
        return true;
    }

    false
}

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
