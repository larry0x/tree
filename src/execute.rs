use {
    crate::{
        error::{Error, Result},
        state::{LAST_COMMITTED_VERSION, NODES, ORPHANS},
        types::{hash, Child, InternalNode, LeafNode, NibbleIterator, NibblePath, Node, NodeKey},
    },
    cosmwasm_std::{ensure, Order, Response, StdResult, Storage},
    cw_storage_plus::PrefixBound,
};

pub fn init(store: &mut dyn Storage) -> Result<Response> {
    // initialize version as zero
    LAST_COMMITTED_VERSION.save(store, &0)?;

    Ok(Response::new())
}

pub fn insert(store: &mut dyn Storage, key: String, value: String) -> Result<Response> {
    let version = increment_version(store)?;

    let key_hash = hash(key.as_bytes());
    // let nibble_path = NibblePath::from(key_hash.clone());
    let nibble_path = NibblePath::from(key.as_bytes().to_vec());

    let nibbles = nibble_path.clone().nibbles().map(|nibble| format!("{nibble:?}")).collect::<Vec<_>>().join("");
    dbg!(nibbles);

    let new_leaf_node = LeafNode::new(key_hash, key.clone(), value.clone());

    insert_at(
        store,
        version,
        NodeKey::root(version - 1),
        &mut nibble_path.nibbles(),
        new_leaf_node,
    )?;

    Ok(Response::new())
}

fn insert_at(
    store: &mut dyn Storage,
    version: u64,
    current_node_key: NodeKey,
    nibble_iter: &mut NibbleIterator,
    new_leaf_node: LeafNode,
) -> Result<(NodeKey, Node)> {
    let Some(current_node) = NODES.may_load(store, &current_node_key)? else {
        // Node is not found. The only case where this is allowed to happen is
        // if the current node is the root, which means the tree is empty.
        ensure!(
            current_node_key.nibble_path.is_empty(),
            Error::NonRootNodeNotFound { node_key: current_node_key }
        );

        // In this case, we simply create a new leaf node and make it the root.
        return create_leaf_node(store, version, NibblePath::empty(), new_leaf_node);
    };

    match current_node {
        Node::Internal(internal_node) => insert_at_internal(
            store,
            current_node_key,
            internal_node,
            version,
            nibble_iter,
            new_leaf_node,
        ),
        Node::Leaf(leaf_node) => insert_at_leaf(
            store,
            current_node_key,
            leaf_node,
            version,
            nibble_iter,
            new_leaf_node,
        ),
    }
}

fn insert_at_internal(
    store: &mut dyn Storage,
    current_node_key: NodeKey,
    mut current_node: InternalNode,
    version: u64,
    nibble_iter: &mut NibbleIterator,
    new_leaf_node: LeafNode,
) -> Result<(NodeKey, Node)> {
    mark_node_as_orphaned(store, version, &current_node_key)?;

    let child_index = nibble_iter.next().unwrap();
    let child_nibble_path = current_node_key.nibble_path.child(child_index);

    let (_, child) = match current_node.children.get(child_index) {
        Some(existing_child) => {
            let child_node_key = NodeKey {
                version: existing_child.version,
                nibble_path: child_nibble_path,
            };
            insert_at(store, version, child_node_key, nibble_iter, new_leaf_node)?
        },
        None => {
            create_leaf_node(store, version, child_nibble_path, new_leaf_node)?
        },
    };

    current_node.children.insert(Child {
        index: child_index,
        version,
        hash: child.hash(),
    });

    create_internal_node(store, version, current_node_key.nibble_path, current_node)
}

fn insert_at_leaf(
    store: &mut dyn Storage,
    current_node_key: NodeKey,
    current_node: LeafNode,
    version: u64,
    nibble_iter: &mut NibbleIterator,
    new_leaf_node: LeafNode,
) -> Result<(NodeKey, Node)> {
    // The current node is necessarily orphaned, so we mark it as such
    mark_node_as_orphaned(store, version, &current_node_key)?;

    // Firstly, if the existing leaf node has exactly the same key_hash as the
    // new leaf node:
    // - if the values are also the same, then no need to do anything
    // - if the values aren't the same, we create a new leaf node and return
    if current_node.key_hash == new_leaf_node.key_hash {
        return if current_node.value == new_leaf_node.value {
            Ok((current_node_key, Node::Leaf(current_node)))
        } else {
            create_leaf_node(store, version, current_node_key.nibble_path, new_leaf_node)
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
    let (_, existing_leaf_node) = create_leaf_node(store, version, existing_leaf_nibble_path, current_node)?;

    // In this example, new_leaf_index = 7
    let new_leaf_index = nibble_iter.next().unwrap();
    let new_leaf_nibble_path = common_nibble_path.child(new_leaf_index);
    let (_, new_leaf_node) = create_leaf_node(store, version, new_leaf_nibble_path, new_leaf_node)?;

    // Create the parent of the two new leaves which have indexes 5 and 7
    let new_internal_node = InternalNode::new([
        Child {
            index: existing_leaf_index,
            version,
            hash: existing_leaf_node.hash(),
        },
        Child {
            index: new_leaf_index,
            version,
            hash: new_leaf_node.hash(),
        },
    ]);
    let (mut new_node_key, mut new_node) = create_internal_node(
        store,
        version,
        common_nibble_path.clone(),
        new_internal_node,
    )?;

    // In this example, three indexes are iterated: 4, 3, 2
    for _ in 0..num_common_nibbles_below_internal {
        let index = common_nibble_path.pop().unwrap();
        let new_internal_node = InternalNode::new([Child {
            index,
            version,
            hash: new_node.hash(),
        }]);
        (new_node_key, new_node) = create_internal_node(
            store,
            version,
            common_nibble_path.clone(),
            new_internal_node,
        )?;
    }

    Ok((new_node_key, new_node))
}

pub fn delete(_store: &mut dyn Storage, _key: String) -> Result<Response> {
    // TODO!!

    Ok(Response::new())
}

pub fn prune(store: &mut dyn Storage, up_to_version: Option<u64>) -> Result<Response> {
    const BATCH_SIZE: usize = 10;

    let end = up_to_version.map(PrefixBound::inclusive);

    loop {
        let batch = ORPHANS
            .prefix_range(store, None, end.clone(), Order::Ascending)
            .take(BATCH_SIZE)
            .collect::<StdResult<Vec<_>>>()?;

        for (stale_since_version, node_key) in &batch {
            NODES.remove(store, node_key);
            ORPHANS.remove(store, (*stale_since_version, node_key));
        }

        if batch.len() < BATCH_SIZE {
            break;
        }
    }

    Ok(Response::new())
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

fn create_internal_node(
    store: &mut dyn Storage,
    version: u64,
    nibble_path: NibblePath,
    internal_node: InternalNode,
) -> Result<(NodeKey, Node)> {
    let node_key = NodeKey { version, nibble_path };
    let node = Node::Internal(internal_node);

    NODES.save(store, &node_key, &node)?;

    Ok((node_key, node))
}

fn create_leaf_node(
    store: &mut dyn Storage,
    version: u64,
    nibble_path: NibblePath,
    leaf_node: LeafNode,
) -> Result<(NodeKey, Node)> {
    let node_key = NodeKey { version, nibble_path };
    let node = Node::Leaf(leaf_node);

    NODES.save(store, &node_key, &node)?;

    Ok((node_key, node))
}

fn mark_node_as_orphaned(
    store: &mut dyn Storage,
    version: u64,
    node_key: &NodeKey,
) -> StdResult<()> {
    ORPHANS.insert(store, (version, node_key))
}

fn increment_version(store: &mut dyn Storage) -> StdResult<u64> {
    LAST_COMMITTED_VERSION.update(store, |version| Ok(version + 1))
}
