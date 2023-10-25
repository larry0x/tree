use {
    cosmwasm_std::{testing::MockStorage, Storage},
    serde::ser::Serialize,
    tree::{execute, query},
};

fn insert(store: &mut dyn Storage, key: &str, value: &str) {
    execute::insert(store, key.into(), value.into()).unwrap();
}

fn print_root(store: &dyn Storage, version: Option<u64>) {
    let res = query::root(store, version).unwrap();
    print_json_pretty(&res);
}

fn print_nodes(store: &dyn Storage) {
    let res = query::nodes(store, None, Some(u32::MAX)).unwrap();
    print_json_pretty(&res)
}

fn print_orphans(store: &dyn Storage) {
    let res = query::orphans(store, None, Some(u32::MAX)).unwrap();
    print_json_pretty(&res)
}

fn print_value_of(store: &dyn Storage, key: &str, version: Option<u64>) {
    let res = query::get(store, key.into(), false, version).unwrap();
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
    let mut store = MockStorage::new();

    execute::init(&mut store).unwrap();

    insert(&mut store, "foo", "bar");
    insert(&mut store, "fuzz", "buzz");
    insert(&mut store, "pumpkin", "cat");
    insert(&mut store, "donald", "trump");
    insert(&mut store, "joe", "biden");
    insert(&mut store, "jake", "shepherd");
    insert(&mut store, "satoshi", "nakamoto");

    execute::prune(&mut store, None).unwrap();

    println!("ROOT:");
    println!("------------------------------------------------------------------");
    print_root(&store, None);

    println!("\nNODES:");
    println!("------------------------------------------------------------------");
    print_nodes(&store);

    println!("\nORPHANS:");
    println!("------------------------------------------------------------------");
    print_orphans(&store);

    println!("\nKEY-VALUE PAIRS:");
    println!("------------------------------------------------------------------");
    print_value_of(&store, "foo", None);
    print_value_of(&store, "fuzz", None);
    print_value_of(&store, "pumpkin", None);
    print_value_of(&store, "donald", None);
    print_value_of(&store, "joe", None);
    print_value_of(&store, "jake", None);
    print_value_of(&store, "satoshi", None);
    print_value_of(&store, "larry", None); // should be None
}
