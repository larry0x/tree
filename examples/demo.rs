use {
    cosmwasm_std::{from_binary, testing::MockStorage, Storage},
    serde::ser::Serialize,
    tree::{verify_membership, verify_non_membership, Op, Tree},
};

const TREE: Tree<String, String> = Tree::new_default();

fn main() {
    let mut store = MockStorage::new();

    TREE.apply(&mut store, [
        ("food".to_string(), Op::Insert("ramen".into())),
        ("fuzz".to_string(), Op::Insert("buzz".into())),
        ("larry".to_string(), Op::Insert("engineer".into())),
        ("pumpkin".to_string(), Op::Insert("cat".into())),
    ]
    .into_iter()
    .collect())
    .unwrap();

    TREE.apply(&mut store, [
        ("fuzz".to_string(), Op::Delete),
        ("larry".to_string(), Op::Delete),
        ("satoshi".to_string(), Op::Insert("nakamoto".into())),
    ]
    .into_iter()
    .collect())
    .unwrap();

    TREE.prune(&mut store, None).unwrap();

    println!("ROOT:");
    println!("------------------------------------------------------------------");
    print_root(&store, None);

    println!("\nNODES:");
    println!("------------------------------------------------------------------");
    print_nodes(&store);

    println!("\nORPHANS:");
    println!("------------------------------------------------------------------");
    // should be empty since we already pruned
    // but you can also comment the pruning line to see what happens
    print_orphans(&store);

    println!("\nKEY-VALUE PAIRS:");
    println!("------------------------------------------------------------------");
    print_values_and_verify(&store, &[
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

fn print_json_pretty<T>(data: &T)
where
    T: Serialize,
{
    let json = serde_json::to_string_pretty(data).unwrap();
    println!("{json}");
}

fn print_root(store: &dyn Storage, version: Option<u64>) {
    let res = TREE.root(store, version).unwrap();
    print_json_pretty(&res);
}

fn print_nodes(store: &dyn Storage) {
    let res = TREE.nodes(store, None, Some(usize::MAX)).unwrap();
    print_json_pretty(&res)
}

fn print_orphans(store: &dyn Storage) {
    let res = TREE.orphans(store, None, Some(usize::MAX)).unwrap();
    print_json_pretty(&res)
}

fn print_values_and_verify(store: &dyn Storage, keys: &[&str]) {
    let root = TREE.root(store, None).unwrap();

    let mut responses = vec![];
    for key in keys {
        let key = key.to_string();
        let res = TREE.get(store, &key, true, None).unwrap();
        let proof = from_binary(res.proof.as_ref().unwrap()).unwrap();

        // verify the proof
        if let Some(value) = &res.value {
            verify_membership(&root.root_hash, &key, value, &proof).unwrap();
            println!("verified the existence of ({key}, {value})");
        } else {
            verify_non_membership(&root.root_hash, &key, &proof).unwrap();
            println!("verified the non-existence of {key}");
        }

        responses.push(res);
    }

    print_json_pretty(&responses)
}
