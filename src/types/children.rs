//! Eventually we want to represent children with a BTreeMap<Nibble, Child>
//! but CosmWasm currently doesn't support serializing maps. This type is a
//! walkaround. Once map serialization is supported, we can delete this.

use {
    crate::{Child, Nibble},
    cosmwasm_schema::cw_serde,
};

#[cw_serde]
#[derive(Default)]
pub struct Children(Vec<Child>);

impl From<Vec<Child>> for Children {
    fn from(vec: Vec<Child>) -> Self {
        Self(vec)
    }
}

impl AsRef<[Child]> for Children {
    fn as_ref(&self) -> &[Child] {
        self.0.as_slice()
    }
}

impl IntoIterator for Children {
    type Item = Child;
    type IntoIter = std::vec::IntoIter<Child>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a Children {
    type Item = &'a Child;
    type IntoIter = std::slice::Iter<'a, Child>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.as_slice().iter()
    }
}

impl Children {
    pub fn new(vec: Vec<Child>) -> Self {
        Self(vec)
    }

    pub fn count(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn get(&self, index: Nibble) -> Option<&Child> {
        self.0.iter().find(|child| child.index == index)
    }

    /// If there is one and only one child, return a reference to this child.
    /// Otherwise (no child or more than one children), panic.
    pub fn get_only(&self) -> &Child {
        assert!(self.count() == 1);
        &self.0[0]
    }

    pub fn insert(&mut self, new_child: Child) {
        for (pos, child) in self.0.iter().enumerate() {
            if child.index == new_child.index {
                self.0[pos] = new_child;
                return;
            }

            if child.index > new_child.index {
                self.0.insert(pos, new_child);
                return;
            }
        }

        self.0.push(new_child);
    }

    // note: attempting to delete a non-existent child results no-op, not error
    pub fn remove(&mut self, index: Nibble) {
        if let Some(pos) = self.0.iter().position(|child| child.index == index) {
            self.0.remove(pos);
        }
    }
}
