use {
    cosmwasm_std::{testing::MockStorage, Storage},
    cw_jellyfish_merkle::{execute, query},
    serde::ser::Serialize,
};

fn insert(store: &mut dyn Storage, key: &str, value: &str) {
    execute::insert(store, key.into(), value.into()).unwrap();
}

fn print_root(store: &dyn Storage) {
    let res = query::root(store).unwrap();
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

fn print_value_of(store: &dyn Storage, key: &str) {
    let res = query::get(store, key.into()).unwrap();
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

    println!("ROOT:");
    println!("------------------------------------------------------------------");
    print_root(&store);

    println!("\nNODES:");
    println!("------------------------------------------------------------------");
    print_nodes(&store);

    println!("\nORPHANS:");
    println!("------------------------------------------------------------------");
    print_orphans(&store);

    println!("\nKEY-VALUE PAIRS:");
    println!("------------------------------------------------------------------");
    print_value_of(&store, "foo");
    print_value_of(&store, "fuzz");
    print_value_of(&store, "pumpkin");
    print_value_of(&store, "donald");
    print_value_of(&store, "joe");
    print_value_of(&store, "jake");
    print_value_of(&store, "satoshi");
    print_value_of(&store, "larry"); // should be None

    // let's try pruning orphaned nodes
    println!("\n********************* pruning orphaned nodes *********************");
    execute::prune(&mut store, None).unwrap();

    println!("\nNODES AFTER PRUNING:");
    println!("------------------------------------------------------------------");
    print_nodes(&store);

    println!("\nORPHANS AFTER PRUNING:");
    println!("------------------------------------------------------------------");
    print_orphans(&store);
}
