use {
    crate::{
        set::Set,
        types::{
            Child, GetResponse, InternalNode, LeafNode, NibbleIterator, NibblePath, Node, NodeKey,
            NodeResponse, OrphanResponse, RootResponse,
        },
    },
    cosmwasm_std::{ensure, Order, Response, StdResult, Storage},
    cw_storage_plus::{Bound, Item, Map, PrefixBound},
    std::cmp::Ordering,
};

const LAST_COMMITTED_VERSION: Item<u64>            = Item::new("v");
const NODES:                  Map<&NodeKey, Node>  = Map::new("n");
const ORPHANS:                Set<(u64, &NodeKey)> = Set::new("o");

const DEFAULT_QUERY_BATCH_SIZE: usize = 10;
const PRUNE_BATCH_SIZE:         usize = 10;

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

    pub fn insert(&mut self, key: String, value: String) -> Result<()> {
        let version = self.increment_version()?;
        let nibble_path = NibblePath::from(key.as_bytes().to_vec());
        let new_leaf_node = LeafNode::new(key.clone(), value.clone());

        self.insert_at(
            version,
            NodeKey::root(version - 1),
            &mut nibble_path.nibbles(),
            new_leaf_node,
        )?;

        Ok(())
    }

    fn insert_at(
        &mut self,
        version: u64,
        current_node_key: NodeKey,
        nibble_iter: &mut NibbleIterator,
        new_leaf_node: LeafNode,
    ) -> Result<(NodeKey, Node)> {
        let Some(current_node) = NODES.may_load(&mut self.store, &current_node_key)? else {
            // Node is not found. The only case where this is allowed to happen is
            // if the current node is the root, which means the tree is empty.
            ensure!(
                current_node_key.nibble_path.is_empty(),
                TreeError::NonRootNodeNotFound { node_key: current_node_key }
            );

            // In this case, we simply create a new leaf node and make it the root.
            return self.create_leaf_node(version, NibblePath::empty(), new_leaf_node);
        };

        match current_node {
            Node::Internal(internal_node) => self.insert_at_internal(
                current_node_key,
                internal_node,
                version,
                nibble_iter,
                new_leaf_node,
            ),
            Node::Leaf(leaf_node) => self.insert_at_leaf(
                current_node_key,
                leaf_node,
                version,
                nibble_iter,
                new_leaf_node,
            ),
        }
    }

    fn insert_at_internal(
        &mut self,
        current_node_key: NodeKey,
        mut current_node: InternalNode,
        version: u64,
        nibble_iter: &mut NibbleIterator,
        new_leaf_node: LeafNode,
    ) -> Result<(NodeKey, Node)> {
        self.mark_node_as_orphaned(version, &current_node_key)?;

        let child_index = nibble_iter.next().unwrap();
        let child_nibble_path = current_node_key.nibble_path.child(child_index);

        let (_, child_node) = match current_node.children.get(child_index) {
            Some(existing_child) => {
                let child_node_key = NodeKey {
                    version: existing_child.version,
                    nibble_path: child_nibble_path,
                };
                self.insert_at(version, child_node_key, nibble_iter, new_leaf_node)?
            },
            None => {
                self.create_leaf_node(version, child_nibble_path, new_leaf_node)?
            },
        };

        current_node.children.insert(Child {
            index: child_index,
            version,
            hash: child_node.hash(),
            is_leaf: child_node.is_leaf(),
        });

        self.create_internal_node(version, current_node_key.nibble_path, current_node)
    }

    fn insert_at_leaf(
        &mut self,
        current_node_key: NodeKey,
        current_node: LeafNode,
        version: u64,
        nibble_iter: &mut NibbleIterator,
        new_leaf_node: LeafNode,
    ) -> Result<(NodeKey, Node)> {
        // The current node is necessarily orphaned, so we mark it as such
        self.mark_node_as_orphaned(version, &current_node_key)?;

        // Firstly, if the existing leaf node has exactly the same key_hash as the
        // new leaf node:
        // - if the values are also the same, then no need to do anything
        // - if the values aren't the same, we create a new leaf node and return
        if current_node.key == new_leaf_node.key {
            return if current_node.value == new_leaf_node.value {
                Ok((current_node_key, Node::Leaf(current_node)))
            } else {
                self.create_leaf_node(version, current_node_key.nibble_path, new_leaf_node)
            };
        }

        // What if they do not have the key_hash? Let's illustrate how this function
        // works in this case with an example.
        //
        // Say, right before calling this function, the tree looks like this:
        //
        //                     [ ] <-- root
        //                      |
        //                     [0]
        //                      |
        //                    [0, 1] <-- has other children [0, 1, *] not shown
        //                      |
        //                  [0, 1, 2] <-- current_node (leaf)
        //
        // The current_node has key_hash              = [0, 1, 2, 3, 4, 5, 6]
        // We want to insert a new leaf with key_hash = [0, 1, 2, 3, 4, 7, 8]
        // Only the last byte (i.e. last two nibbles) differs.
        //
        // Our objective is that the resulting tree should look like this:
        //
        //                     [ ] <-- root
        //                      |
        //                     [0]
        //                      |
        //                    [0, 1]
        //                      |
        //                  [0, 1, 2] <-- new internal node (replacing current leaf)
        //                      |
        //                 [0, 1, 2, 3] <-- new internal node
        //                      |
        //               [0, 1, 2, 3, 4] <-- new internal node
        //                  /      \
        //    [0, 1, 2, 3, 4, 5]   [0, 1, 2, 3, 4, 7] <-- two new leaves
        //
        // When we start, nibble_iter should be at the `2` nibble.
        // We use the ^ symbol to denote the cursor position:
        //
        // nibble_iter = [0, 1, 2, 3, 4, 5, 8, 9]
        //                      ^
        //
        // First, we grab the existing leaf's key_hash, put it together with the new
        // leaf's key_hash, and skip all nibbles that have already been visited up
        // to this part of the tree:
        //
        // visited_nibbles:       [0, 1, 2]
        //                         ^a    ^b
        // existing_leaf_nibbles: [0, 1, 2, 3, 4, 5, 6]
        //                         ^a    ^b
        //
        // The cursor starts at location ^a and stops at ^b when visited_nibbles
        // runs out.
        let mut visited_nibbles = nibble_iter.visited_nibbles();
        let existing_leaf_nibble_path = NibblePath::from(current_node.key.clone().as_bytes().to_vec());
        let mut existing_leaf_nibbles = existing_leaf_nibble_path.nibbles();
        skip_common_prefix(&mut visited_nibbles, &mut existing_leaf_nibbles);

        // We now grab all the remaining nibbles, and keep advancing until the paths
        // split:
        //
        // nibble_iter:                          [0, 1, 2, 3, 4, 7, 8]
        //                                                 ^c ^d
        // existing_leaf_nibbles_below_internal:          [3, 4, 5, 6]
        //                                                 ^c ^d
        //
        // The cursor starts at ^c and stops and ^d when the next nibbles diverge.
        //
        // - num_common_nibbles_below_internal = 2
        // - common_nibble_path = [0, 1, 2, 3, 4]
        let mut existing_leaf_nibbles_below_internal = existing_leaf_nibbles.remaining_nibbles();
        let num_common_nibbles_below_internal = skip_common_prefix(
            nibble_iter,
            &mut existing_leaf_nibbles_below_internal,
        );
        let mut common_nibble_path: NibblePath = nibble_iter.visited_nibbles().collect();

        // Now what we need to do is to create 3 new internal nodes, with the
        // following nibble paths, respectively:
        //
        // - [0, 1, 2] (this one replaces the existing leaf)
        // - [0, 1, 2, 3]
        // - [0, 1, 2, 3, 4] (this one becomes the new parent of the existing leaf
        //   and the new leaf)
        //
        // We do this from bottom up. First we create the parent node, then its
        // parent, so on.
        //
        // In this example, existing_leaf_index = 5
        let existing_leaf_index = existing_leaf_nibbles_below_internal.next().unwrap();
        let existing_leaf_nibble_path = common_nibble_path.child(existing_leaf_index);
        let (_, existing_leaf_node) = self.create_leaf_node(
            version,
            existing_leaf_nibble_path,
            current_node,
        )?;

        // In this example, new_leaf_index = 7
        let new_leaf_index = nibble_iter.next().unwrap();
        let new_leaf_nibble_path = common_nibble_path.child(new_leaf_index);
        let (_, new_leaf_node) = self.create_leaf_node(
            version,
            new_leaf_nibble_path,
            new_leaf_node,
        )?;

        // Create the parent of the two new leaves which have indexes 5 and 7
        let new_internal_node = InternalNode::new(vec![
            Child {
                index: existing_leaf_index,
                version,
                hash: existing_leaf_node.hash(),
                is_leaf: true,
            },
            Child {
                index: new_leaf_index,
                version,
                hash: new_leaf_node.hash(),
                is_leaf: true,
            },
        ]);
        let (mut new_node_key, mut new_node) = self.create_internal_node(
            version,
            common_nibble_path.clone(),
            new_internal_node,
        )?;

        // In this example, three indexes are iterated: 4, 3, 2
        for _ in 0..num_common_nibbles_below_internal {
            let index = common_nibble_path.pop().unwrap();
            let new_internal_node = InternalNode::new(vec![Child {
                index,
                version,
                hash: new_node.hash(),
                is_leaf: false,
            }]);
            (new_node_key, new_node) = self.create_internal_node(
                version,
                common_nibble_path.clone(),
                new_internal_node,
            )?;
        }

        Ok((new_node_key, new_node))
    }

    pub fn delete(&mut self, key: String) -> Result<()> {
        let version = self.increment_version()?;
        let nibble_path = NibblePath::from(key.as_bytes().to_vec());

        let root_node_key = NodeKey::root(version - 1);
        let Some(root_node) = NODES.may_load(&self.store, &root_node_key)? else {
            // tree is empty, no-op
            return Ok(());
        };

        match root_node {
            Node::Internal(internal_node) => {
                self.delete_at_internal(
                    root_node_key,
                    internal_node,
                    version,
                    &mut nibble_path.nibbles(),
                    &key,
                )?;
            },
            Node::Leaf(leaf_node) => {
                // root node is a leaf node, meaning there is only one KV in the
                // tree. if this key equals the key we want to delete, then we
                // simply delete it. otherwise, the key doesn't exist, no op.
                if leaf_node.key == key {
                    self.mark_node_as_orphaned(version, &root_node_key)?;
                }
            },
        }

        Ok(())
    }

    fn delete_at_internal(
        &mut self,
        current_node_key: NodeKey,
        mut current_node: InternalNode,
        version: u64,
        nibble_iter: &mut NibbleIterator,
        key: &str,
    ) -> Result<DeleteResponse> {
        let child_index = nibble_iter.next().unwrap();
        let Some(child) = current_node.children.get_mut(child_index) else {
            // can't find the child, meaning the key doesn't exist in the tree,
            // no op.
            return Ok(DeleteResponse::Unchanged);
        };

        let child_node_key = NodeKey {
            version: child.version,
            nibble_path: current_node_key.nibble_path.child(child_index),
        };
        let child_node = NODES.load(&self.store, &child_node_key)?;

        match child_node {
            Node::Internal(internal_node) => {
                // the child is an internal node: we recursively attempt to
                // delete under it.
                let res = self.delete_at_internal(
                    child_node_key,
                    internal_node,
                    version,
                    nibble_iter,
                    key,
                )?;

                match res {
                    // the child internal node has been deleted and replaced
                    // with a leaf node
                    DeleteResponse::Replaced(new_child) => {
                        child.version = new_child.version;
                        child.hash = new_child.hash;
                        child.is_leaf = new_child.is_leaf; // this is necessarily true
                    },
                    // the child internal node is not replaced, but its child
                    // list has changed, so we need to update the version and
                    // recompute the hash
                    DeleteResponse::Updated(updated_child_node) => {
                        child.version = version;
                        child.hash = updated_child_node.hash();
                    },
                    // no op
                    DeleteResponse::Unchanged => {
                        return Ok(DeleteResponse::Unchanged);
                    },
                }
            },
            Node::Leaf(leaf_node) => {
                // the child is a leaf node: now we compare if the child's key
                // matches the key we want to delete.
                //
                // note: they are not necessarily the same - the child's key may
                // be a prefix of the key we want to delete. for example, we
                // want to delete `food`, but the child is `foo`.
                //
                // if the keys don't match, then otherwise, the key doesn't
                // exist in the tree, no op.
                if leaf_node.key != key {
                    return Ok(DeleteResponse::Unchanged);
                }

                // keys match, delete the leaf
                self.mark_node_as_orphaned(version, &child_node_key)?;
                current_node.children.remove(child_index);

                // now the tricky part - if after deleting the leaf, the current
                // node only has 1 child, and this child is a leaf, then we need
                // to collapse the path
                if let Some(other_child) = current_node.children.get_only() {
                    if other_child.is_leaf {
                        self.mark_node_as_orphaned(version, &current_node_key)?;

                        return Ok(DeleteResponse::Replaced(other_child.clone()));
                    }
                }
            },
        }

        let (_, updated_current_node) = self.create_internal_node(
            version,
            current_node_key.nibble_path,
            current_node,
        )?;

        Ok(DeleteResponse::Updated(updated_current_node))
    }

    pub fn prune(&mut self, up_to_version: Option<u64>) -> Result<Response> {
        let end = up_to_version.map(PrefixBound::inclusive);

        loop {
            let batch = ORPHANS
                .prefix_range(&mut self.store, None, end.clone(), Order::Ascending)
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

    fn create_internal_node(
        &mut self,
        version: u64,
        nibble_path: NibblePath,
        internal_node: InternalNode,
    ) -> Result<(NodeKey, Node)> {
        let node_key = NodeKey { version, nibble_path };
        let node = Node::Internal(internal_node);

        NODES.save(&mut self.store, &node_key, &node)?;

        Ok((node_key, node))
    }

    fn create_leaf_node(
        &mut self,
        version: u64,
        nibble_path: NibblePath,
        leaf_node: LeafNode,
    ) -> Result<(NodeKey, Node)> {
        let node_key = NodeKey { version, nibble_path };
        let node = Node::Leaf(leaf_node);

        NODES.save(&mut self.store, &node_key, &node)?;

        Ok((node_key, node))
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
            value: self.get_value_at(node_key, &mut nibble_path.nibbles())?,
            proof: None, // TODO
        })
    }

    fn get_value_at(
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

                self.get_value_at(child_node_key, nibble_iter)
            },
            Node::Leaf(leaf_node) => {
                // TODO: impl PartialEq to prettify this syntax
                if leaf_node.key.into_bytes().as_ref() == nibble_iter.nibble_path().bytes {
                    return Ok(Some(leaf_node.value));
                }

                Ok(None)
            },
        }
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

/// Advance both iterators if their next nibbles are the same, until either
/// reaches the end or their next nibbles mismatch. Return the number of matched
/// nibbles.
fn skip_common_prefix(x: &mut NibbleIterator, y: &mut NibbleIterator) -> usize {
    let mut count = 0;

    loop {
        let x_peek = x.peek();
        if x_peek.is_none() {
            break;
        }

        let y_peek = y.peek();
        if y_peek.is_none() {
            break;
        }

        if x_peek != y_peek {
            break;
        }

        x.next();
        y.next();
        count += 1;
    }

    count
}

/// If the user specifies a version, we use it. Otherwise, load the latest version.
fn unwrap_version(store: &dyn Storage, version: Option<u64>) -> StdResult<u64> {
    if let Some(version) = version {
        Ok(version)
    } else {
        LAST_COMMITTED_VERSION.load(store)
    }
}

/// This is the response data of the `delete_at` private method, describing what
/// happened to the internal node after performing the deletion.
enum DeleteResponse {
    /// The internal node has only 1 child left, and therefore has been deleted
    /// and replaced by that child (that child also gets a new nibble path).
    Replaced(Child),
    /// The internal node has been updated but not replaced, meaning it still
    /// has at least two children after the deletion. The updated node is returned.
    Updated(Node),
    /// Nothing happened. This signals that the hashes don't need to be re-computed.
    Unchanged,
}
