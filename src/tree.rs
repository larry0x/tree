use {
    crate::{
        Batch, Child, GetResponse, Nibble, NibbleIterator, NibblePath, NibbleRange,
        NibbleRangeIterator, Node, NodeKey, Op, OpResponse, Proof, ProofNode, Record, RootResponse,
        Set,
    },
    cosmwasm_std::{to_binary, Order, StdResult, Storage},
    cw_storage_plus::{Item, Map, PrefixBound},
    serde::{de::DeserializeOwned, ser::Serialize},
    std::{cmp::Ordering, collections::HashMap},
};
#[cfg(feature = "debug")]
use {
    crate::{NodeResponse, OrphanResponse},
    cw_storage_plus::Bound,
};

const PRUNE_BATCH_SIZE: usize = 10;
#[cfg(feature = "debug")]
const DEFAULT_QUERY_BATCH_SIZE: usize = 10;

/// A versioned and merklized key-value store, based on a radix tree data
/// structure.
///
/// Versioned means it allows queries under historical states (provided they
/// have not been pruned). Merklized means it is capable of generating Merkle
/// proofs to demontrate that certain key-value pairs exist or do not exist in
/// the tree.
///
/// `Tree` works similarly as common storage primitives provided by
/// [cw-storage-plus](https://github.com/CosmWasm/cw-storage-plus), such as Item,
/// Map, and IndexedMap. It can be declared as a constant:
///
/// ```rust
/// use tree::Tree;
/// const TREE: Tree<Vec<u8>, Vec<u8>> = Tree::new_default();
/// ```
///
/// `Tree` offers a minimal API:
///
/// | method    | description                                                                   |
/// | --------- | ----------------------------------------------------------------------------- |
/// | `apply`   | perform a batch insertion or deletion operations                              |
/// | `prune`   | delete nodes that are not longer part of the tree since a given version       |
/// | `root`    | query the root node hash                                                      |
/// | `get`     | query the value associated with the given key, optionally with a Merkle proof |
/// | `iterate` | enumerate key-value pairs stored in the tree                                  |
pub struct Tree<'a, K, V> {
    version: Item<'a, u64>,
    nodes: Map<'a, &'a NodeKey, Node<K, V>>,
    orphans: Set<'a, (u64, &'a NodeKey)>,
}

impl<'a, K, V> Default for Tree<'a, K, V> {
    fn default() -> Self {
        Self::new_default()
    }
}

impl<'a, K, V> Tree<'a, K, V> {
    pub const fn new(
        version_namespace: &'a str,
        node_namespace: &'a str,
        orphan_namespace: &'a str,
    ) -> Self {
        Tree {
            version: Item::new(version_namespace),
            nodes: Map::new(node_namespace),
            orphans: Set::new(orphan_namespace),
        }
    }

    /// Create a `Tree` using the default namespaces.
    //
    // ideally we just use `Tree::default`, however rust still doesn't support
    // Default trait to return a const:
    // https://github.com/rust-lang/rust/issues/67792
    pub const fn new_default() -> Self {
        Self::new("v", "n", "o")
    }
}

// note: whereas other common storage primitives (such as Item, Map) only
// requires K, V to implement cw_serde traits (namely Serialize + DeserializedOwned)
// we additionally require AsRef<[u8]>. there are two uses for this:
// - conversion of K into NibblePath
// - hashing K and V
// most types that you'll typically use implement AsRef<[u8]>, such as Vec<u8>
// and String.
impl<'a, K, V> Tree<'a, K, V>
where
    K: Serialize + DeserializeOwned + Clone + PartialEq + AsRef<[u8]>,
    V: Serialize + DeserializeOwned + Clone + PartialEq + AsRef<[u8]>,
{
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
    pub fn apply(&self, store: &mut dyn Storage, batch: Batch<K, V>) -> Result<()> {
        let old_version = self.version.may_load(store)?.unwrap_or(0);
        let old_root_key = NodeKey::root(old_version);

        // note: we don't save the new version to store just yet, unless we know
        // the root node has been changed.
        let new_version = old_version + 1;

        // collect the batch into a sorted Vec, also converting the string keys
        // to NibblePaths
        let batch = batch
            .into_iter()
            .map(|(key, op)| (NibblePath::from(&key), key, op))
            .collect::<Vec<_>>();

        // recursively apply the batch, starting from the root (depth = 0)
        match self.apply_at(
            store,
            new_version,
            &old_root_key,
            None,
            &batch,
        )? {
            OpResponse::Updated(updated_root_node) => {
                self.set_version(store, new_version)?;
                self.create_node(store, new_version, NibblePath::empty(), &updated_root_node)?;
                if old_version > 0 {
                    self.mark_node_as_orphaned(store, new_version, &old_root_key)?;
                }
            },
            OpResponse::Deleted => {
                self.set_version(store, new_version)?;
                if old_version > 0 {
                    self.mark_node_as_orphaned(store, new_version, &old_root_key)?;
                }
            },
            OpResponse::Unchanged => {
                // do nothing. note that we don't increment the version if the
                // root node is not changed.
            },
        }

        Ok(())
    }

    fn apply_at(
        &self,
        store: &mut dyn Storage,
        version: u64,
        current_node_key: &NodeKey,
        current_node: Option<Node<K, V>>,
        // some basic rust knowledge here: the following are different!
        //
        // mut batch: &T
        // this means that `batch` can be pointed to a different T instance
        //
        // batch: &mut T
        // this means that the T instance that `batch` points to can be mutated
        mut batch: &[(NibblePath, K, Op<V>)],
    ) -> Result<OpResponse<K, V>> {
        // attempt to load the node. if not found, we simply create a new empty
        // node (no children, no data)
        let mut current_node = if let Some(node) = current_node {
            node
        } else {
            self.nodes.may_load(store, current_node_key)?.unwrap_or_else(Node::new)
        };

        // make a mutable clone of the current node. after we've executed the
        // ops, we will compare with the original whether it has been changed
        let current_node_before = current_node.clone();

        // a cache of the current node's children that have been changed.
        // we don't want to write these nodes to store immediately, because if
        // the current node ends up having only one child, we will need to
        // collapse the path (i.e. delete the current node, move the only child
        // one level up)
        let mut updated_child_nodes = HashMap::new();

        // if the node has data, and the data's key doesn't exactly equal the
        // node's nibble path, we take it out and insert it into the batch.
        // we call this the "dangling_data"
        let mut dangling_data = None;
        if let Some(Record { key, .. }) = &current_node.data {
            // in a previously bugged implementation, here we simply compared
            // key.as_bytes() and current_nibble_path.bytes; this misses the
            // case where the nibble path may have odd number of nibbles and
            // the last nibble in the key may be zero
            if NibblePath::from(key) != current_node_key.nibble_path {
                dangling_data = current_node.data.take();
            }
        }

        // what this part means is a bit hard to explain...
        //
        // basically if there is a key in the batch that is an exact match with
        // the current node's nibble path, then it is necessarily the first item
        // in the batch (I don't have a rigorous proof, but empirically this is
        // true)
        //
        // if this is the case, we apply the op at the current node, and remove
        // this item from the batch.
        //
        // additionally, if this node originally had data is will be overwritten
        // here, we take it out as "dangling data" and insert it later
        if batch[0].0 == current_node_key.nibble_path {
            current_node.apply_op(&batch[0]);
            batch = &batch[1..];
        }

        // insert the dangling data into the batch
        //
        // note: only insert if the key isn't already in the batch. if it's
        // already in, it will be overwritten anyways so we just discard it
        //
        // this requires copying the batch in memory (slice --> vec) which is
        // slow, but i don't have a good idea to improve on this
        // basically we have to do this to allow iteration (if we hash the keys,
        // there would be no dangling data but no iteration either). it's a
        // tradeoff between performance and feature, and is one that we're
        // willing to make (iteration is such as important feature)
        let mut owned_batch;
        let batch = if let Some(Record { key, value }) = dangling_data {
            let nibble_path = NibblePath::from(&key);
            owned_batch = batch.to_vec();
            if let Err(pos) = batch.binary_search_by_key(&&nibble_path, |(nibble_path, _, _)| nibble_path) {
                owned_batch.insert(pos, (nibble_path, key, Op::Insert(value)));
            }
            owned_batch.as_slice()
        } else {
            batch
        };

        // now, if there is only one item left in the batch AND one of the
        // following is satisfied, then we apply the op at the current node:
        //
        // - the current node is a leaf, and the key matches exactly the nibble
        //   path we want to write to
        // - the current node has neither any child nor data
        //
        // if this condition is not satisfied, we need to dispatch the ops to
        // the current node's children.
        if batch.len() == 1 && current_node.is_empty() {
            current_node.apply_op(&batch[0]);
        } else {
            let nibble_range_iter = NibbleRangeIterator::new(batch, current_node_key.depth());
            for NibbleRange { nibble, start, end } in nibble_range_iter {
                let child = current_node.children.get(nibble);
                let child_version = child.map(|c| c.version).unwrap_or(version);
                let child_node_key = current_node_key.child(child_version, nibble);

                match self.apply_at(
                    store,
                    version,
                    &child_node_key,
                    updated_child_nodes.remove(&nibble),
                    &batch[start..=end],
                )? {
                    OpResponse::Updated(updated_child_node) => {
                        current_node.children.insert(Child {
                            index: nibble,
                            version,
                            hash: updated_child_node.hash(),
                        });

                        if child_node_key.version < version {
                            self.mark_node_as_orphaned(store, version, &child_node_key)?;
                        }

                        updated_child_nodes.insert(nibble, updated_child_node);
                    },
                    OpResponse::Deleted => {
                        current_node.children.remove(nibble);
                        if child_node_key.version < version {
                            self.mark_node_as_orphaned(store, version, &child_node_key)?;
                        }
                    },
                    OpResponse::Unchanged => (),
                }
            }
        }

        // if the current node has neither any child nor data, then it should be
        // deleted
        if current_node.is_empty() {
            return Ok(OpResponse::Deleted);
        }

        // if the current node has no data and exactly 1 child, and this child
        // is a leaf node, then the path can be collapsed (i.e. the current node
        // deleted, and that child leaf node moved on level up)
        if current_node.data.is_none() && current_node.children.count() == 1 {
            let child = current_node.children.get_only();
            if let Some(child_node) = updated_child_nodes.get(&child.index) {
                if child_node.is_leaf() {
                    return Ok(OpResponse::Updated(child_node.clone()));
                }
            } else {
                let child_node_key = current_node_key.child(child.version, child.index);
                let child_node = self.nodes.load(store, &child_node_key)?;
                if child_node.is_leaf() {
                    self.mark_node_as_orphaned(store, version, &child_node_key)?;
                    return Ok(OpResponse::Updated(child_node));
                }
            };
        }

        // now we know the current node won't be deleted or collapsed,
        // we can write the updated child nodes
        for (nibble, node) in updated_child_nodes {
            let nibble_path = current_node_key.nibble_path.child(nibble);
            self.create_node(store, version, nibble_path, &node)?;
        }

        if current_node != current_node_before {
            return Ok(OpResponse::Updated(current_node));
        }

        Ok(OpResponse::Unchanged)
    }

    pub fn prune(&self, store: &mut dyn Storage, up_to_version: Option<u64>) -> Result<()> {
        let end = up_to_version.map(PrefixBound::inclusive);

        loop {
            let batch = self
                .orphans
                .prefix_range(store, None, end.clone(), Order::Ascending)
                .take(PRUNE_BATCH_SIZE)
                .collect::<StdResult<Vec<_>>>()?;

            for (stale_since_version, node_key) in &batch {
                self.nodes.remove(store, node_key);
                self.orphans.remove(store, (*stale_since_version, node_key));
            }

            if batch.len() < PRUNE_BATCH_SIZE {
                break;
            }
        }

        Ok(())
    }

    fn version_or_default(&self, store: &dyn Storage, version: Option<u64>) -> StdResult<u64> {
        if let Some(version) = version {
            Ok(version)
        } else {
            self.version.load(store)
        }
    }

    fn set_version(&self, store: &mut dyn Storage, version: u64) -> StdResult<()> {
        self.version.save(store, &version)
    }

    fn create_node(
        &self,
        store: &mut dyn Storage,
        version: u64,
        nibble_path: NibblePath,
        node: &Node<K, V>,
    ) -> StdResult<()> {
        self.nodes.save(store, &NodeKey::new(version, nibble_path), node)
    }

    fn mark_node_as_orphaned(
        &self,
        store: &mut dyn Storage,
        orphaned_since_version: u64,
        node_key: &NodeKey,
    ) -> StdResult<()> {
        self.orphans.insert(store, (orphaned_since_version, node_key))
    }

    pub fn root(&self, store: &dyn Storage, version: Option<u64>) -> Result<RootResponse> {
        let version = self.version_or_default(store, version)?;
        let root_node = self.root_node(store, version)?;

        Ok(RootResponse {
            version,
            root_hash: root_node.hash(),
        })
    }

    fn root_node(&self, store: &dyn Storage, version: u64) -> Result<Node<K, V>> {
        let root_node_key = NodeKey::root(version);
        self.nodes
            .may_load(store, &root_node_key)?
            .ok_or(TreeError::RootNodeNotFound { version })
    }

    pub fn get(
        &self,
        store: &dyn Storage,
        key: &K,
        prove: bool,
        version: Option<u64>,
    ) -> Result<GetResponse<K, V>> {
        let version = self.version_or_default(store, version)?;
        let nibble_path = NibblePath::from(&key);

        let (value, proof) = self.get_at(
            store,
            NodeKey::root(version),
            &mut nibble_path.nibbles(),
            prove,
        )?;

        let proof = if prove {
            Some(to_binary(&proof)?)
        } else {
            None
        };

        Ok(GetResponse { key: key.clone(), value, proof })
    }

    fn get_at(
        &self,
        store: &dyn Storage,
        current_node_key: NodeKey,
        nibble_iter: &mut NibbleIterator,
        prove: bool,
    ) -> Result<(Option<V>, Proof<K, V>)> {
        let Some(current_node) = self.nodes.may_load(store, &current_node_key)? else {
            // Node is not found. There are a few circumstances:
            // - if the node is the root,
            //   - and it's older than the latest version: it may simply be that
            //     that version has been pruned
            //   - and it's the current version: it may simply be that the current
            //     tree is empty
            //   - and it's newer than the latest version: this query is illegal
            // - if the node is not the root: database corrupted
            if current_node_key.nibble_path.is_empty() {
                let latest_version = self.version.load(store)?;
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
                return Err(TreeError::NonRootNodeNotFound {
                    node_key: current_node_key,
                });
            }
        };

        // if the node has data and the key matches the request key, then we
        // have found it
        if let Some(Record { key, value }) = current_node.data.clone() {
            if NibblePath::from(&key) == nibble_iter.nibble_path() {
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
            store,
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
    pub fn iterate<'c, S: Storage>(
        &'a self,
        store: &'c S,
        order: Order,
        min: Option<&K>,
        max: Option<&K>,
        version: Option<u64>,
    ) -> Result<TreeIterator<'c, K, V, S>>
    where
        'a: 'c,
    {
        let version = self.version_or_default(store, version)?;
        let root_node = self.root_node(store, version)?;

        Ok(TreeIterator::new(self, store, order, min, max, root_node))
    }

    #[cfg(feature = "debug")]
    pub fn node(
        &self,
        store: &dyn Storage,
        node_key: NodeKey,
    ) -> Result<Option<NodeResponse<K, V>>> {
        Ok(self
            .nodes
            .may_load(store, &node_key)?
            .map(|node| NodeResponse {
                node_key,
                hash: node.hash(),
                node,
            }))
    }

    #[cfg(feature = "debug")]
    pub fn nodes(
        &self,
        store: &dyn Storage,
        start_after: Option<&NodeKey>,
        limit: Option<usize>,
    ) -> Result<Vec<NodeResponse<K, V>>> {
        let start = start_after.map(Bound::exclusive);
        let limit = limit.unwrap_or(DEFAULT_QUERY_BATCH_SIZE);

        self.nodes
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

    #[cfg(feature = "debug")]
    pub fn orphans(
        &self,
        store: &dyn Storage,
        start_after: Option<&OrphanResponse>,
        limit: Option<usize>,
    ) -> Result<Vec<OrphanResponse>> {
        let start = start_after.map(|o| Bound::exclusive((o.since_version, &o.node_key)));
        let limit = limit.unwrap_or(DEFAULT_QUERY_BATCH_SIZE);

        self.orphans
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
}

pub struct TreeIterator<'a, K, V, S> {
    tree: &'a Tree<'a, K, V>,
    store: &'a S,
    order: Order,
    min: Option<NibblePath>,
    max: Option<NibblePath>,
    visited_nibbles: NibblePath,
    visited_nodes: Vec<Node<K, V>>,
}

impl<'a, K, V, S> TreeIterator<'a, K, V, S>
where
    K: AsRef<[u8]>,
{
    pub fn new(
        tree: &'a Tree<'a, K, V>,
        store: &'a S,
        order: Order,
        min: Option<&K>,
        max: Option<&K>,
        root_node: Node<K, V>,
    ) -> Self {
        Self {
            tree,
            store,
            order,
            min: min.map(NibblePath::from),
            max: max.map(NibblePath::from),
            visited_nibbles: NibblePath::empty(),
            visited_nodes: vec![root_node],
        }
    }
}

impl<'a, K, V, S> Iterator for TreeIterator<'a, K, V, S>
where
    S: Storage,
    K: Serialize + DeserializeOwned + Clone + AsRef<[u8]>,
    V: Serialize + DeserializeOwned + Clone,
{
    type Item = Result<(K, V)>;

    fn next(&mut self) -> Option<Self::Item> {
        iterate_at(
            self.tree,
            self.store,
            self.order,
            self.min.as_ref(),
            self.max.as_ref(),
            &mut self.visited_nibbles,
            &mut self.visited_nodes,
            None,
        )
        .transpose()
    }
}

#[allow(clippy::too_many_arguments)]
fn iterate_at<K, V>(
    tree: &Tree<K, V>,
    store: &dyn Storage,
    order: Order,
    min: Option<&NibblePath>,
    max: Option<&NibblePath>,
    visited_nibbles: &mut NibblePath,
    visited_nodes: &mut Vec<Node<K, V>>,
    start_after_index: Option<Nibble>,
) -> Result<Option<(K, V)>>
where
    K: Serialize + DeserializeOwned + Clone + AsRef<[u8]>,
    V: Serialize + DeserializeOwned + Clone,
{
    let Some(current_node) = visited_nodes.last().cloned() else {
        return Ok(None);
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
        let child_node = tree.nodes.load(store, &child_node_key)?;

        visited_nibbles.push(child.index);
        visited_nodes.push(child_node.clone());

        // if the child node has data, and the key is in range, then we stop the
        // recursion and return this data
        if let Some(Record { key, value }) = child_node.data {
            // we only need to compare the key with the min. we don't need to
            // compare with the max because any key greater than max should have
            // already been dropped when comparing `nibbles_in_range`
            if key_in_range(&key, min) {
                return Ok(Some((key, value)));
            }
        }

        // if the current node has no data, then we do a depth-first search,
        // exploring the children of this child
        return iterate_at(tree, store, order, min, max, visited_nibbles, visited_nodes, None);
    }

    // now we've gone over all the childs of the current node, and still hasn't
    // returned. this means there is no data found in any of the subtrees below
    // the current node. we need to go up one level and search in the siblings.
    let (Some(index), Some(_)) = (visited_nibbles.pop(), visited_nodes.pop()) else {
        return Ok(None);
    };

    iterate_at(tree, store, order, min, max, visited_nibbles, visited_nodes, Some(index))
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

fn key_in_range<K: AsRef<[u8]>>(key: &K, min: Option<&NibblePath>) -> bool {
    if let Some(min) = min {
        if key.as_ref() < min.bytes.as_slice() {
            return false;
        }
    }

    true
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
