use crate::{Nibble, NibblePath, Op};

#[cfg_attr(test, derive(Debug, PartialEq))]
pub struct NibbleRange {
    pub nibble: Nibble,
    pub start: usize,
    // end is inclusive
    pub end: usize,
}

/// Assume we have a list of nibble paths, which can be of variable lengths,
/// ordered ascendingly (important!). For example:
///
///         0123456
///         0135
///         013568
///         02222222222
/// pos --> 123456
///         13579abc
///         ^
///         nibble_idx
///
/// In this example, we are looking at the very first nibble of each nibble path,
/// so nibble_idx = 0. We're currently looking at the 5th nibble path in the
/// list, so pos = 4.
///
/// If we were to iterate this list of nibble paths from pos = 0, we would
/// iterate over two tuples:
///
/// - (0, 3)
/// - (4, 5)
///
/// The first one (0, 4) is the pos range of the nibble paths that have Nibble(0)
/// on it's nibble_idx. Both 0 and 3 are inclusive (important!)
///
/// Similarly, (4, 6) is the pos range of the nibble paths that have Nibble(1)
/// on it's nibble_idx. Both 4 and 5 are inclusive (important!)
///
/// This iterator type is adapted from Diem:
/// https://github.com/diem/diem/blob/diem-core-v1.4.4/storage/jellyfish-merkle/src/lib.rs#L188
/// which is open source under Apache-2.0 license.
///
/// A difference is that Diem assumes all nibble paths are of the same length
/// (because in Jellyfish Merkle Tree, the keys are hashed), while we do not
/// make this assumption.
pub struct NibbleRangeIterator<'a> {
    // must be sorted by nibble path
    batch: &'a [(NibblePath, Op)],
    // which index in the nibble path we're looking at
    nibble_idx: usize,
    // which nibble path in the batch we're looking at
    pos: usize,
}

impl<'a> NibbleRangeIterator<'a> {
    pub fn new(batch: &'a [(NibblePath, Op)], nibble_idx: usize) -> Self {
        Self {
            batch,
            nibble_idx,
            // we start iterating from the first nibble path, pos = 0
            pos: 0,
        }
    }

    fn get_nibble(&self, pos: usize) -> Nibble {
        self.batch[pos].0.get_nibble(self.nibble_idx)
    }
}

impl<'a> Iterator for NibbleRangeIterator<'a> {
    type Item = NibbleRange;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos == self.batch.len() {
            return None;
        }

        // bisect between the current pos and the end of the list, looking for
        // the first nibble path whose's nibble is greater than that of the
        // current pos
        let left = self.pos;
        let current_nibble = self.get_nibble(left);
        let (mut i, mut j) = (left, self.batch.len() - 1);
        while i < j {
            let mid = j - (j - i) / 2;
            if self.get_nibble(mid) > current_nibble {
                j = mid - 1;
            } else {
                i = mid;
            }
        }

        self.pos = i + 1;

        Some(NibbleRange {
            nibble: current_nibble,
            start: left,
            end: i,
        })
    }
}

#[test]
fn iterating_nibble_ranges() {
    let batch = [
        "\"0123456\"",
        "\"0135\"",
        "\"013568\"",
        "\"02222222222\"",
        "\"123456\"",
        "\"13579abc\"",
    ]
    .into_iter()
    .map(|nibble_path_str| {
        let nibble_path = serde_json::from_str(nibble_path_str).unwrap();
        (nibble_path, Op::Delete)
    })
    .collect::<Vec<_>>();

    let nibble_range_iter = NibbleRangeIterator::new(batch.as_slice(), 0);
    let ranges = nibble_range_iter.collect::<Vec<_>>();
    assert_eq!(ranges, vec![
        NibbleRange {
            nibble: Nibble::from(0u8),
            start: 0,
            end: 3,
        },
        NibbleRange {
            nibble: Nibble::from(1u8),
            start: 4,
            end: 5,
        },
    ]);
}
