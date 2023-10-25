use {
    cosmwasm_std::{testing::MockStorage, Storage},
    serde::ser::Serialize,
    tree::Tree,
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

fn print_value_of<S: Storage>(tree: &Tree<S>, key: &str, version: Option<u64>) {
    let res = tree.get(key.into(), false, version).unwrap();
    print_json_pretty(&res)
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

    tree.insert("foo".into(), "bar".into()).unwrap();
    tree.insert("fuzz".into(), "buzz".into()).unwrap();
    tree.insert("pumpkin".into(), "cat".into()).unwrap();
    tree.insert("donald".into(), "trump".into()).unwrap();
    tree.insert("joe".into(), "biden".into()).unwrap();
    tree.insert("jake".into(), "shepherd".into()).unwrap();
    tree.insert("satoshi".into(), "nakamoto".into()).unwrap();

    tree.prune(None).unwrap();

    println!("ROOT:");
    println!("------------------------------------------------------------------");
    print_root(&tree, None);

    println!("\nNODES:");
    println!("------------------------------------------------------------------");
    print_nodes(&tree);

    println!("\nORPHANS:");
    println!("------------------------------------------------------------------");
    print_orphans(&tree);

    println!("\nKEY-VALUE PAIRS:");
    println!("------------------------------------------------------------------");
    print_value_of(&tree, "foo", None);
    print_value_of(&tree, "fuzz", None);
    print_value_of(&tree, "pumpkin", None);
    print_value_of(&tree, "donald", None);
    print_value_of(&tree, "joe", None);
    print_value_of(&tree, "jake", None);
    print_value_of(&tree, "satoshi", None);
    print_value_of(&tree, "larry", None); // should be None
}
