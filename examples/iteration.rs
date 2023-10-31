use {
    cosmwasm_std::{testing::MockStorage, Order},
    tree::{Op, Tree},
};

fn main() {
    let mut tree = Tree::new(MockStorage::new());

    tree.initialize().unwrap();

    tree.apply([
        ("food".to_string(), Op::Insert("ramen".into())),
        ("fuzz".to_string(), Op::Insert("buzz".into())),
        ("jake".to_string(), Op::Insert("shepherd".into())),
        ("larry".to_string(), Op::Insert("engineer".into())),
        ("pumpkin".to_string(), Op::Insert("cat".into())),
        ("satoshi".to_string(), Op::Insert("nakamoto".into())),
    ]
    .into_iter()
    .collect())
    .unwrap();

    // adjust min, max, and order as you like to test things out
    let order = Order::Ascending;
    let min = None;
    let max = None;
    let mut iter = tree.iterate(order, min, max, None).unwrap();

    // should print Some((food, ramen))
    let record = iter.next();
    dbg!(record);

    // should print Some((fuzz, buzz))
    let record = iter.next();
    dbg!(record);

    // should print Some((jake, shepherd))
    let record = iter.next();
    dbg!(record);

    // should print Some((larry, engineer))
    let record = iter.next();
    dbg!(record);

    // should print Some((pumpkin, cat))
    let record = iter.next();
    dbg!(record);

    // should print Some((satoshi, nakamoto))
    let record = iter.next();
    dbg!(record);

    // should print None
    let record = iter.next();
    dbg!(record);
}
