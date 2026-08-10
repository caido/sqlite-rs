use crate::dict::types::DictId;
use std::collections::hash_map::Entry;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

const MAX_CACHED_DICTS: usize = 5;

/// Fixed-capacity LRU cache of dictionaries keyed by [`DictId`].
///
/// Holds at most [`MAX_CACHED_DICTS`] entries. [`DictLru::get`] and [`DictLru::insert`]
/// promote an id to most-recent; [`DictLru::peek`] does not.
pub struct DictLru<T> {
    map: HashMap<DictId, Arc<T>>,
    order: VecDeque<DictId>,
}

impl<T> DictLru<T> {
    pub fn new() -> Self {
        Self {
            map: HashMap::with_capacity(MAX_CACHED_DICTS),
            order: VecDeque::with_capacity(MAX_CACHED_DICTS),
        }
    }

    pub fn get(&mut self, id: DictId) -> Option<Arc<T>> {
        let value = self.map.get(&id)?.clone();
        self.touch(id);
        Some(value)
    }

    pub fn peek(&self, id: DictId) -> Option<Arc<T>> {
        self.map.get(&id).cloned()
    }

    pub fn insert(&mut self, id: DictId, value: Arc<T>) {
        if let Entry::Occupied(mut e) = self.map.entry(id) {
            e.insert(value);
            self.touch(id);
            return;
        }

        while self.map.len() >= MAX_CACHED_DICTS {
            if let Some(evicted) = self.order.pop_front() {
                self.map.remove(&evicted);
            }
        }
        self.map.insert(id, value);
        self.order.push_back(id);
    }

    fn touch(&mut self, id: DictId) {
        self.order.retain(|&x| x != id);
        self.order.push_back(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn id(n: u32) -> DictId {
        DictId::new(n)
    }
    #[test]
    fn evicts_oldest_after_five() {
        let mut cache = DictLru::new();
        for n in 1..=5 {
            cache.insert(id(n), Arc::new(n));
        }
        cache.insert(id(6), Arc::new(6));
        assert!(cache.peek(id(1)).is_none());
        assert!(cache.peek(id(2)).is_some());
        assert!(cache.peek(id(6)).is_some());
    }
    #[test]
    fn get_promotes_to_most_recent() {
        let mut cache = DictLru::new();
        for n in 1..=5 {
            cache.insert(id(n), Arc::new(n));
        }
        assert!(cache.get(id(1)).is_some());
        cache.insert(id(6), Arc::new(6));
        assert!(cache.peek(id(1)).is_some());
        assert!(cache.peek(id(2)).is_none());
    }
    #[test]
    fn peek_does_not_promote() {
        let mut cache = DictLru::new();
        for n in 1..=5 {
            cache.insert(id(n), Arc::new(n));
        }
        assert!(cache.peek(id(1)).is_some());
        cache.insert(id(6), Arc::new(6));
        assert!(cache.peek(id(1)).is_none());
    }
    #[test]
    fn reinsert_updates_value_and_promotes() {
        let mut cache = DictLru::new();
        for n in 1..=5 {
            cache.insert(id(n), Arc::new(n));
        }
        cache.insert(id(1), Arc::new(100));
        cache.insert(id(6), Arc::new(6));
        assert_eq!(*cache.peek(id(1)).unwrap(), 100);
        assert!(cache.peek(id(2)).is_none());
    }
}
