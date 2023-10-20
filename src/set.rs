use {
    cosmwasm_std::{Empty, Order, StdResult, Storage},
    cw_storage_plus::{Bound, Key, KeyDeserialize, Path, Prefix, Prefixer, PrimaryKey},
    std::marker::PhantomData,
};

pub struct Set<'a, T> {
    namespace: &'a [u8],
    item_type: PhantomData<T>,
}

impl<'a, T> Set<'a, T>
where
    T: PrimaryKey<'a> + KeyDeserialize,
{
    pub const fn new(namespace: &'a str) -> Self {
        Set {
            namespace: namespace.as_bytes(),
            item_type: PhantomData,
        }
    }

    fn key(&self, item: T) -> Path<Empty> {
        Path::new(
            self.namespace,
            &item.key().iter().map(Key::as_ref).collect::<Vec<_>>(),
        )
    }

    fn no_prefix(&self) -> Prefix<T, Empty, T> {
        Prefix::new(self.namespace, &[])
    }

    pub fn prefix(&self, p: T::Prefix) -> Prefix<T::Suffix, Empty, T::Suffix> {
        Prefix::new(self.namespace, &p.prefix())
    }

    pub fn contains(&self, store: &dyn Storage, item: T) -> bool {
        self.key(item).has(store)
    }

    pub fn insert(&self, store: &mut dyn Storage, item: T) -> StdResult<()> {
        self.key(item).save(store, &Empty {})
    }

    pub fn remove(&self, store: &mut dyn Storage, item: T) {
        self.key(item).remove(store)
    }

    pub fn items<'c>(
        &self,
        store: &'c dyn Storage,
        min: Option<Bound<'a, T>>,
        max: Option<Bound<'a, T>>,
        order: Order,
    ) -> Box<dyn Iterator<Item = StdResult<T::Output>> + 'c>
    where
        T::Output: 'static,
    {
        self.no_prefix().keys(store, min, max, order)
    }
}
