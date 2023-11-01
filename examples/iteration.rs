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
    let min = Some("fxxx");
    let max = Some("pzzz");
    let mut iter = TREE.iterate(&store, order, min, max, None).unwrap();

    // should print Some((food, ramen))
    dbg!(iter.next());

    // should print Some((fuzz, buzz))
    dbg!(iter.next());

    // should print Some((jake, shepherd))
    dbg!(iter.next());

    // should print Some((larry, engineer))
    dbg!(iter.next());

    // should print Some((pumpkin, cat))
    dbg!(iter.next());

    // should print Some((satoshi, nakamoto))
    dbg!(iter.next());

    // should print None
    dbg!(iter.next());
}
