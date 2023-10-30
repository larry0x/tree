use {
    cosmwasm_std::{from_binary, testing::MockStorage, Storage},
    serde::ser::Serialize,
    tree::{verify_membership, verify_non_membership, Op, Tree},
};

fn print_root<S: Storage>(tree: &Tree<S>, version: Option<u64>) {
    let res = tree.root(version).unwrap();
    print_json_pretty(&res);
}

fn print_nodes<S: Storage>(tree: &Tree<S>) {
    let res = tree.nodes(None, Some(usize::MAX)).unwrap();
    print_json_pretty(&res)
}

fn print_orphans<S: Storage>(tree: &Tree<S>) {
    let res = tree.orphans(None, Some(usize::MAX)).unwrap();
    print_json_pretty(&res)
}

fn print_values_and_verify<S: Storage>(tree: &Tree<S>, keys: &[&str]) {
    let root = tree.root(None).unwrap();

    let mut responses = vec![];
    for key in keys {
        let res = tree.get(key.to_string(), true, None).unwrap();
        let proof = from_binary(res.proof.as_ref().unwrap()).unwrap();

        // verify the proof
        if let Some(value) = &res.value {
            verify_membership(&root.root_hash, key, value, &proof).unwrap();
            println!("verified the existence of ({key}, {value})");
        } else {
            verify_non_membership(&root.root_hash, key, &proof).unwrap();
            println!("verified the non-existence of {key}");
        }

        responses.push(res);
    }

    print_json_pretty(&responses)
}

fn print_json_pretty<T>(data: &T)
where
    T: Serialize,
{
    let json = serde_json::to_string_pretty(data).unwrap();
    println!("{json}");
}

fn main() {
    let mut tree = Tree::new(MockStorage::new());

    println!("initializing!");
    tree.initialize().unwrap();

    println!("applying the 1st batch!");
    tree.apply([
        ("food".to_string(), Op::Insert("ramen".into())),
        ("fuzz".to_string(), Op::Insert("buzz".into())),
        ("larry".to_string(), Op::Insert("engineer".into())),
        ("pumpkin".to_string(), Op::Insert("cat".into())),
    ]
    .into_iter()
    .collect())
    .unwrap();

    println!("applying the 2nd batch!");
    tree.apply([
        ("fuzz".to_string(), Op::Delete),
        ("larry".to_string(), Op::Delete),
        ("satoshi".to_string(), Op::Insert("nakamoto".into())),
    ]
    .into_iter()
    .collect())
    .unwrap();

    println!("pruning!");
    tree.prune(None).unwrap();

    println!("ROOT:");
    println!("------------------------------------------------------------------");
    print_root(&tree, None);

    println!("\nNODES:");
    println!("------------------------------------------------------------------");
    print_nodes(&tree);

    println!("\nORPHANS:");
    println!("------------------------------------------------------------------");
    // should be empty since we already pruned
    // but you can also comment the pruning line to see what happens
    print_orphans(&tree);

    println!("\nKEY-VALUE PAIRS:");
    println!("------------------------------------------------------------------");
    print_values_and_verify(&tree, &[
        // these are the 3 keys that exist
        "food",
        "pumpkin",
        "satoshi",
        // keys below do not exist in the tree, should return None/null
        "foo",
        "fuzz",
        "larry",
    ]);
}
