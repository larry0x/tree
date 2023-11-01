use {
    cosmwasm_std::{testing::MockStorage, Order},
    tree::{Op, Tree},
};

const TREE: Tree<String, String> = Tree::new_default();

fn main() {
    let mut store = MockStorage::new();

    TREE.apply(&mut store, [
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
    let mut iter = TREE.iterate(&store, order, min, max, None).unwrap();

    // should print Some((food, ramen))
    dbg!(iter.next().transpose().unwrap());

    // should print Some((fuzz, buzz))
    dbg!(iter.next().transpose().unwrap());

    // should print Some((jake, shepherd))
    dbg!(iter.next().transpose().unwrap());

    // should print Some((larry, engineer))
    dbg!(iter.next().transpose().unwrap());

    // should print Some((pumpkin, cat))
    dbg!(iter.next().transpose().unwrap());

    // should print Some((satoshi, nakamoto))
    dbg!(iter.next().transpose().unwrap());

    // should print None
    dbg!(iter.next().transpose().unwrap());

    // should print None
    dbg!(iter.next().transpose().unwrap());
}
