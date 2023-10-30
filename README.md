> **_THIS IS A TECHNICAL DEMO, NOT FOR PRODUCTION USE_**

# tree

A _versioned_ and _merklized_ radix tree.

- **Versioned** means each node are indexed by a version. Each time a batch of new data is written, the version is incremented. Data of old versions are not deleted unless a `prune` method is manually called. This allows us to query data at past versions. For blockchains, this is used in archive nodes.
- **Merklized** means a root hash is derived as a commitment for the entire tree. For any key, a concise proof can be generated to prove its membership or non-membership in the tree. For blockchains, this is necessary for building light clients, which are used in IBC for example.

## Comparison with alternative solutions

Whereas [IAVL](https://github.com/cosmos/iavl) and [Merk](https://github.com/turbofish-org/merk) are binary search trees, this work is a 16-ary radix tree, similar to Ethereum's Patricia Merkle tree (PMT) and [Diem's Jellyfish Merkle tree (JMT)](https://github.com/diem/diem/tree/latest/storage/jellyfish-merkle).

Compared to PMT and JMT, we made a few adjustments:

- **Whereas PMT/JMT hashes keys, we use raw keys.** This is necessary to allow iteration (e.g. "what is the very next key in the tree after the key `abc`?"). Using raw keys increases complexity because 1) keys can be of variable length, and 2) internal nodes may also have values. This said, iteration is such a powerful feature for smart contract development that I think it's worth it.

- **Whereas PMT/JMT does not support deleting, we do.** The EVM doesn't support deleting keys. If you use Solidity's `delete` keyword, what it actually does is to set the storage slot to a default value such as `0`, `false`, or `""`. I'm not sure about Move but my guess is it's the same. This is really bad, because if, say, there is a `0` at a certain storage slot, the contract can't tell if it's that the value doesn't exist, or that the value exists and it's a zero. This can open security holes if the developer fails to pay attention. In CWD, we definitely want to support deleting keys.

- **We simplified the node structure.** PMT has 4 node types: internal, leaf, extension, and null. JMT got rid of extension. In this work, we reduce this to only one single node type, which simplifies logics in many cases.

The following table summarizes features supported by various state commitment schemes:

|                            | IAVL | Merk | JMT | this work |
| -------------------------- | ---- | ---- | --- | --------- |
| stable rust implementation | ❌    | ❌    | ✅   | ✅         |
| batched ops                | ❌    | ✅    | ✅   | ✅         |
| deletion                   | ✅    | ✅    | ❌   | ✅         |
| iteration                  | ✅    | ✅    | ❌   | ✅         |
| O(1) read                  | ❌    | ✅    | ❌   | ❌         |
| historical query           | ✅    | ❌    | ✅   | ✅         |
| merkle proof               | ✅    | ✅    | ✅   | ✅         |

The only feature this work misses out is O(1) read, which I think is mutually exclusive with historical queries (see note below). For use in blockchains, historical queries seem to be more important, so we're willing to make this tradeoff.

Note - You can either index a key by `version || bytes` which gives you historical queries but no O(1) reads, or by the raw `bytes` which gives you O(1) reads but no historical queries. Or you can have two stores, one indexing by raw bytes and the other versioned (similar to Osmosis' [fastnode](https://github.com/cosmos/iavl/pull/468) improvement to IAVL), so you get both, but at the tradeoff of doubling disk usage. An alternative is to use the underlying DB's timestamping feature (which is what both [Merk's doc](https://github.com/turbofish-org/merk/blob/develop/docs/algorithms.md) and [ADR-065](https://github.com/cosmos/cosmos-sdk/blob/main/docs/architecture/adr-065-store-v2.md) suggest) but that's only O(1) from the tree's perspective; the underlying DB still needs to do the timestamping stuff which is not O(1).

## Acknowledgment

We took inspiration from JMT's code, which is open sourced under Apache-2.0.

## Copyright

Materials in this repository are private property owned solely by [larry0x](https://twitter.com/larry0x). They are published for informational purposes only. No license, right of reproduction, of distribution, or other right with respect thereto is granted or implied.
