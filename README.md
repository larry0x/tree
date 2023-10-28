# tree

An versioned and merklized radix tree.

Design objectives:

- batched ops (an op is either an insertion or a deletion)
- iteration
- generating proofs (membership and non-membership)

## Comparison with alternative state commitment schemes

|                            | IAVL | Merk | JMT | this work |
| -------------------------- | ---- | ---- | --- | --------- |
| stable rust implementation | ❌    | ❌    | ✅   | ✅         |
| batched ops                | ❌    | ✅    | ✅   | ✅         |
| deletion                   | ✅    | ✅    | ❌   | ✅         |
| O(1) read                  | ❌    | ✅    | ❌   | ❌         |
| historical query           | ✅    | ❌    | ✅   | ✅         |
| merkle proof               | ✅    | ✅    | ✅   | ✅         |

The only feature this work misses out is O(1) read, which I think is mutually exclusive with historical queries.[^1] For use in blockchains, historical queries seem to be more important, so we're willing to make this tradeoff.

[^1]: You can either index a key by `version || bytes` which gives you historical queries but no O(1) reads, or by the raw `bytes` which gives you O(1) reads but no historical queries. Or you can have two stores, one indexing by raw bytes and the other versioned (similar to Osmosis' [fastnode](https://github.com/cosmos/iavl/pull/468) improvement to IAVL), so you get both, but at the tradeoff of doubling disk usage. An alternative is to use the underlying DB's timestamping feature (which is what both [Merk's doc](https://github.com/turbofish-org/merk/blob/develop/docs/algorithms.md) and [ADR-065](https://github.com/cosmos/cosmos-sdk/blob/main/docs/architecture/adr-065-store-v2.md) suggest) but that's only O(1) from the tree's perspective; the underlying DB still needs to do the timestamping stuff which is not O(1).

## Copyright

Materials in this repository are private property owned solely by [larry0x](https://twitter.com/larry0x). They are published for informational purposes only. No license, right of reproduction, of distribution, or other right with respect thereto is granted or implied.
