use {
    cosmwasm_std::{Empty, StdResult, Storage},
    cw_storage_plus::{PrimaryKey, IndexList},
    std::marker::PhantomData,
};

pub struct IndexedSet<'a, K, I>
where
    K: PrimaryKey<'a>,
    I: IndexList<Empty>,
{
    namespace: &'a [u8],
    pub idx: I,
    item_type: PhantomData<K>,
}

impl<'a, K, I> IndexedSet<'a, K, I>
where
    K: PrimaryKey<'a>,
    I: IndexList<Empty>,
{
    pub const fn new(namespace: &'a str, indexes: I) -> Self {
        Self {
            namespace: namespace.as_bytes(),
            idx: indexes,
            item_type: PhantomData,
        }
    }

    pub fn insert(&self, store: &mut dyn Storage, item: K) -> StdResult<()> {
        todo!();
        // let old_item = self.may_get(store, key.clone())?;
        // self.replace(store, key, Some(item), old_tem.as_ref())
    }

    pub fn delete(&self, store: &mut dyn Storage, item: K) -> StdResult<()> {
        todo!();
        // let old_item = self.may_get(store, key.clone())?;
        // self.replace(store, key, None, old_item.as_ref())
    }
}
