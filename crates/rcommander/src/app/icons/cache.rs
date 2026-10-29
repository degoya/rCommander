//! The icons loaded so far, bounded: the least recently shown goes when a new one would pass
//! the limit. A key is pending from the moment it is asked for until its answer comes, so it is
//! asked for once.

use std::collections::{HashMap, hash_map::Entry};

use super::key::IconKey;

/// Where one key stands.
#[derive(Debug)]
enum State<V> {
    /// Asked for; the answer is on its way.
    Pending,
    Ready(V),
    /// The provider had none: the generic icon stands in, and the key is not asked for again.
    Missing,
}

#[derive(Debug)]
struct Slot<V> {
    state: State<V>,
    /// When it was last wanted, in [`IconCache::tick`]s.
    used: u64,
}

/// A bounded LRU map from [`IconKey`] to a loaded icon.
#[derive(Debug)]
pub struct IconCache<V> {
    slots: HashMap<IconKey, Slot<V>>,
    capacity: usize,
    tick: u64,
}

impl<V> IconCache<V> {
    /// A cache of at most `capacity` keys (at least one).
    pub fn new(capacity: usize) -> Self {
        Self {
            slots: HashMap::new(),
            capacity: capacity.max(1),
            tick: 0,
        }
    }

    /// Marks `key` as shown now. A key the cache does not know becomes pending and comes back,
    /// to be loaded; a known one (pending, loaded or missing) gives `None`.
    pub fn want(&mut self, key: IconKey) -> Option<IconKey> {
        self.tick += 1;
        let tick = self.tick;
        match self.slots.entry(key) {
            Entry::Occupied(mut slot) => {
                slot.get_mut().used = tick;
                None
            }
            Entry::Vacant(slot) => {
                let key = slot.key().clone();
                slot.insert(Slot {
                    state: State::Pending,
                    used: tick,
                });
                self.evict();
                Some(key)
            }
        }
    }

    /// The loaded icon of `key`, if there is one.
    pub fn get(&self, key: &IconKey) -> Option<&V> {
        match self.slots.get(key) {
            Some(Slot {
                state: State::Ready(value),
                ..
            }) => Some(value),
            _ => None,
        }
    }

    /// The answer for a pending `key`: its icon, or `None` when the provider had none. An answer
    /// for a key no longer pending (evicted, or the cache was cleared) is dropped.
    pub fn fill(&mut self, key: &IconKey, value: Option<V>) {
        if let Some(slot) = self.slots.get_mut(key)
            && matches!(slot.state, State::Pending)
        {
            slot.state = value.map_or(State::Missing, State::Ready);
        }
    }

    /// A pending `key` the loader passed over: forgotten, so it is asked for again while shown.
    pub fn forget(&mut self, key: &IconKey) {
        if self
            .slots
            .get(key)
            .is_some_and(|slot| matches!(slot.state, State::Pending))
        {
            self.slots.remove(key);
        }
    }

    /// Everything goes, pending keys too: their answers are dropped when they come.
    pub fn clear(&mut self) {
        self.slots.clear();
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// Drops the least recently wanted keys until the cache fits. The keys of one screen are
    /// always the newest, so with a capacity above a screen's rows they stay.
    fn evict(&mut self) {
        while self.slots.len() > self.capacity {
            let oldest = self
                .slots
                .iter()
                .min_by_key(|(_, slot)| slot.used)
                .map(|(key, _)| key.clone());
            let Some(key) = oldest else {
                return;
            };
            self.slots.remove(&key);
        }
    }
}

#[cfg(test)]
#[path = "cache_tests.rs"]
mod tests;
