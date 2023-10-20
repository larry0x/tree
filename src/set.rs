use {
    cosmwasm_std::{Empty, Order, StdResult, Storage},
    cw_storage_plus::{
        namespaced_prefix_range, Bound, Key, KeyDeserialize, Path, Prefix, PrefixBound, PrimaryKey,
    },
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

    pub fn insert(&self, store: &mut dyn Storage, item: T) -> StdResult<()> {
        self.key(item).save(store, &Empty {})
    }

    pub fn remove(&self, store: &mut dyn Storage, item: T) {
        self.key(item).remove(store)
    }

    pub fn items<'b>(
        &self,
        store: &'b dyn Storage,
        min: Option<Bound<'a, T>>,
        max: Option<Bound<'a, T>>,
        order: Order,
    ) -> Box<dyn Iterator<Item = StdResult<T::Output>> + 'b>
    where
        T::Output: 'static,
    {
        self.no_prefix().keys(store, min, max, order)
    }

    pub fn prefix_range<'c>(
        &self,
        store: &'c dyn Storage,
        min: Option<PrefixBound<'a, T::Prefix>>,
        max: Option<PrefixBound<'a, T::Prefix>>,
        order: Order,
    ) -> Box<dyn Iterator<Item = StdResult<T::Output>> + 'c>
    where
        T::Output: 'static,
    {
        let mapped = namespaced_prefix_range(
            store, self.namespace,
            min,
            max,
            order,
        )
        .map(|(k, _)| T::from_vec(k));

        Box::new(mapped)
    }
}
