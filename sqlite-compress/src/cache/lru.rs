use std::{
    collections::{hash_map::Entry, HashMap, VecDeque},
    sync::Arc,
};

use crate::dict::DictKey;

const MAX_CACHED_DICTS: usize = 5;

/// Fixed-capacity LRU cache of dictionaries keyed by [`DictKey`].
///
/// Holds at most [`MAX_CACHED_DICTS`] entries. [`DictLru::get`] and [`DictLru::insert`]
/// promote a key to most-recent; [`DictLru::peek`] does not.
pub(crate) struct DictLru<T> {
    map: HashMap<DictKey, Arc<T>>,
    order: VecDeque<DictKey>,
}

impl<T> DictLru<T> {
    pub(crate) fn new() -> Self {
        Self {
            map: HashMap::with_capacity(MAX_CACHED_DICTS),
            order: VecDeque::with_capacity(MAX_CACHED_DICTS),
        }
    }

    pub(crate) fn get(&mut self, key: &DictKey) -> Option<Arc<T>> {
        let value = self.map.get(key)?.clone();
        self.touch(key);
        Some(value)
    }

    pub(crate) fn peek(&self, key: &DictKey) -> Option<Arc<T>> {
        self.map.get(key).cloned()
    }

    pub(crate) fn insert(&mut self, key: DictKey, value: Arc<T>) {
        if let Entry::Occupied(mut e) = self.map.entry(key.clone()) {
            e.insert(value);
            self.touch(&key);
            return;
        }

        while self.map.len() >= MAX_CACHED_DICTS {
            if let Some(evicted) = self.order.pop_front() {
                self.map.remove(&evicted);
            }
        }
        self.map.insert(key.clone(), value);
        self.order.push_back(key);
    }

    fn touch(&mut self, key: &DictKey) {
        self.order.retain(|x| x != key);
        self.order.push_back(key.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dict::DictId;

    fn key(schema: &str, n: u32) -> DictKey {
        DictKey::new(schema, DictId::new(n))
    }

    #[test]
    fn evicts_oldest_after_five() {
        let mut cache = DictLru::new();
        for n in 1..=5 {
            cache.insert(key("main", n), Arc::new(n));
        }
        cache.insert(key("main", 6), Arc::new(6));
        assert!(cache.peek(&key("main", 1)).is_none());
        assert!(cache.peek(&key("main", 2)).is_some());
        assert!(cache.peek(&key("main", 6)).is_some());
    }

    #[test]
    fn get_promotes_to_most_recent() {
        let mut cache = DictLru::new();
        for n in 1..=5 {
            cache.insert(key("main", n), Arc::new(n));
        }
        assert!(cache.get(&key("main", 1)).is_some());
        cache.insert(key("main", 6), Arc::new(6));
        assert!(cache.peek(&key("main", 1)).is_some());
        assert!(cache.peek(&key("main", 2)).is_none());
    }

    #[test]
    fn peek_does_not_promote() {
        let mut cache = DictLru::new();
        for n in 1..=5 {
            cache.insert(key("main", n), Arc::new(n));
        }
        assert!(cache.peek(&key("main", 1)).is_some());
        cache.insert(key("main", 6), Arc::new(6));
        assert!(cache.peek(&key("main", 1)).is_none());
    }

    #[test]
    fn reinsert_updates_value_and_promotes() {
        let mut cache = DictLru::new();
        for n in 1..=5 {
            cache.insert(key("main", n), Arc::new(n));
        }
        cache.insert(key("main", 1), Arc::new(100));
        cache.insert(key("main", 6), Arc::new(6));
        assert_eq!(*cache.peek(&key("main", 1)).unwrap(), 100);
        assert!(cache.peek(&key("main", 2)).is_none());
    }

    #[test]
    fn same_id_different_schema_are_distinct() {
        let mut cache = DictLru::new();
        cache.insert(key("raw", 1), Arc::new(10));
        cache.insert(key("archive", 1), Arc::new(20));
        assert_eq!(*cache.peek(&key("raw", 1)).unwrap(), 10);
        assert_eq!(*cache.peek(&key("archive", 1)).unwrap(), 20);
    }
}
