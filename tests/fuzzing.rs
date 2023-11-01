// only run this test if the "fuzzing" feature is enabled
// this test takes very long to run so we don't want it be run by Github CI
// we only manually run it:
// $ cargo test --features fuzzing --test fuzzing -- --nocapture
#![cfg(feature = "fuzzing")]

//! Our fuzz testing strategy is as follows:
//!
//! - Write an initial batch of 100 random keys and values. Each key or value is
//!   an alphanumeric string with length between 1-20 characters.
//!
//! - Write another 99 batches. Each batch consists of:
//!   - 50 inserts under existing keys
//!   - 30 inserts under new keys
//!   - 10 deletes of existing keys
//!   - 10 deletes of non-existing keys (should be no-op)
//!
//! - After each batch, we check every key that has ever been inserted or
//!   deleted, query the value and proof, check the values are correct and
//!   proofs are valid.
//!
//! Basically, we prove the following properties:
//!
//! - any KV pair that's in the tree can always be proven to exist against the
//!   root hash;
//! - any key that's not in the tree can always be proven to not exist against
//!   the root hash.

use {
    anyhow::bail,
    cosmwasm_std::{from_binary, testing::MockStorage, Storage},
    rand::Rng,
    random_string::{charsets::ALPHANUMERIC, generate},
    std::{collections::BTreeMap, fs},
    tree::{verify_membership, verify_non_membership, Batch, Op, Tree},
};

const TREE: Tree<String, String> = Tree::new_default();

#[test]
fn fuzzing() {
    let mut rng = rand::thread_rng();
    let mut batches = BTreeMap::new();
    let mut log = Batch::new();
    let mut store = MockStorage::new();

    // do the initial batch
    let batch = generate_initial_batch(&mut rng);
    batches.insert(1, batch.clone());
    write_to_log(&mut log, &batch);
    TREE.apply(&mut store, batch).unwrap();
    check(&store, &log, 1).unwrap();

    // do the subsequent 99 batches
    for i in 2..=100 {
        let batch = generate_subsequent_batch(&log, &mut rng);
        batches.insert(i, batch.clone());
        write_to_log(&mut log, &batch);
        TREE.apply(&mut store, batch).unwrap();
        if let Err(err) = check(&store, &log, i) {
            // if fails, write the batches to a file so we can analyze it
            let batches_bytes = serde_json::to_vec_pretty(&batches).unwrap();
            fs::write("testdata/batches.json", batches_bytes).unwrap();
            panic!("{err}");
        }
    }
}

fn rand_str<R: Rng>(rng: &mut R) -> String {
    generate(rng.gen_range(1..=20), ALPHANUMERIC)
}

fn rand_key_from_log<'a, R: Rng>(
    log: &'a Batch<String, String>,
    rng: &mut R,
) -> (&'a String, &'a Op<String>) {
    log.iter().nth(rng.gen_range(0..log.len())).unwrap()
}

fn generate_initial_batch<R: Rng>(rng: &mut R) -> Batch<String, String> {
    let mut batch = Batch::new();
    for _ in 0..100 {
        batch.insert(rand_str(rng), Op::Insert(rand_str(rng)));
    }
    batch
}

fn generate_subsequent_batch<R: Rng>(log: &Batch<String, String>, rng: &mut R) -> Batch<String, String> {
    let mut batch = Batch::new();
    // 50 inserts under existing keys
    for _ in 0..50 {
        loop {
            let (key, op) = rand_key_from_log(log, rng);
            if let Op::Insert(_) = op {
                batch.insert(key.clone(), Op::Insert(rand_str(rng)));
                break;
            }
        }
    }
    // 10 deletes under existing keys
    for _ in 0..10 {
        loop {
            let (key, op) = rand_key_from_log(log, rng);
            if let Op::Insert(_) = op {
                if batch.get(key).is_none() {
                    batch.insert(key.clone(), Op::Delete);
                }
                break;
            }
        }
    }
    // 30 inserts under possibly new keys
    for _ in 0..30 {
        batch.insert(rand_str(rng), Op::Insert(rand_str(rng)));
    }
    // 10 deletes under possibly new keys
    for _ in 0..10 {
        batch.insert(rand_str(rng), Op::Delete);
    }
    batch
}

/// The log is a map storing the last op that was done to a key. By looking at
/// it we can tell whether a key is in the tree or not. If the last op was an
/// insert, then it's in the tree. It's the last op is a delete, or if there
/// hasn't been any op done under this key, then it's not in the tree.
fn write_to_log(log: &mut Batch<String, String>, batch: &Batch<String, String>) {
    for (key, op) in batch {
        log.insert(key.clone(), op.clone());
    }
}

/// For every key that has ever been inserted or deleted, query the value, and
/// verify the merkle proof.
fn check(store: &dyn Storage, log: &Batch<String, String>, i: usize) -> anyhow::Result<()> {
    let root = TREE.root(store, None).unwrap();
    println!("batch {i}, root = {}", root.root_hash);

    for (key, op) in log {
        let res = TREE.get(store, key, true, None).unwrap();
        let proof = from_binary(&res.proof.unwrap()).unwrap();
        if let Op::Insert(value) = op {
            if res.value.as_ref() != Some(value) {
                bail!("incorrect value for key = {key}, expecting Some({value}), found {:?}", res.value);
            };
            if let Err(err) = verify_membership(&root.root_hash, key, value, &proof) {
                bail!("failed to verify membership for key = {key}, value = {value}, err = {err}");
            };
        } else {
            if let Some(value) = res.value {
                bail!("incorrect value for key = {key}, expecting None, found Some({value})");
            };
            if let Err(err) = verify_non_membership(&root.root_hash, key, &proof) {
                bail!("failed to verify non-membership for key = {key}, err = {err}");
            };
        }
    }

    Ok(())
}
