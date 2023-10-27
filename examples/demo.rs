use {
    cosmwasm_std::{testing::MockStorage, Storage},
    serde::ser::Serialize,
    tree::{Op, Tree},
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

fn print_values_of<S: Storage>(tree: &Tree<S>, keys: &[&str]) {
    let mut responses = vec![];
    for key in keys {
        responses.push(tree.get(key.to_string(), false, None).unwrap());
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

    tree.initialize().unwrap();

    tree.apply([
        ("foo".to_string(), Op::Insert("bar".into())),
        ("fuzz".to_string(), Op::Insert("buzz".into())),
        ("pumpkin".to_string(), Op::Insert("cat".into())),
        ("donald".to_string(), Op::Insert("trump".into())),
        ("joe".to_string(), Op::Insert("biden".into())),
        ("jake".to_string(), Op::Insert("shepherd".into())),
        ("satoshi".to_string(), Op::Insert("nakamoto".into())),
    ]
    .into_iter()
    .collect())
    .unwrap();

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
    print_values_of(&tree, &[
        "foo",
        "fuzz",
        "pumpkin",
        "donald",
        "joe",
        "jake",
        "satoshi",
        "larry", // should be None
    ]);
}
