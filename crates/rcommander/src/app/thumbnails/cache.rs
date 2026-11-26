//! The tiles loaded so far, bounded by their bytes: the least recently shown go when a new one
//! would pass the limit ([`MEMORY_BYTES`], fixed - Total Commander's `ThumbnailMemoryLimit` is an
//! ini value, not an option). A key is pending from the moment it is asked for until its answer
//! comes, so it is asked for once; a pending key is never dropped for room. The keys are also
//! ordered by when they were last wanted, so the eviction finds the oldest without searching the
//! whole cache (RC-110-67).

use std::collections::{BTreeMap, HashMap};

use super::key::ThumbKey;

/// The tiles kept in memory per window: 64 MiB of pixels.
pub const MEMORY_BYTES: usize = 64 * 1024 * 1024;
/// The most keys kept, tiles and misses: a bound on the map besides the bytes.
const MAX_KEYS: usize = 16 * 1024;

#[derive(Debug)]
enum State<V> {
    Pending,
    /// A tile and its bytes.
    Ready(V, usize),
    /// No tile: the icon stands in, and the key is not asked for again.
    Missing,
}

#[derive(Debug)]
struct Slot<V> {
    state: State<V>,
    /// When it was last wanted, in ticks.
    used: u64,
}

/// A bounded LRU map from [`ThumbKey`] to a tile.
#[derive(Debug)]
pub struct TileCache<V> {
    slots: HashMap<ThumbKey, Slot<V>>,
    /// Every key of `slots` by its `used` tick, oldest first; ticks are unique.
    order: BTreeMap<u64, ThumbKey>,
    /// The bytes the ready tiles may hold.
    budget: usize,
    held: usize,
    tick: u64,
}

impl<V> TileCache<V> {
    pub fn new(budget: usize) -> Self {
        Self {
            slots: HashMap::new(),
            order: BTreeMap::new(),
            budget,
            held: 0,
            tick: 0,
        }
    }

    /// Marks `key` as shown now; `true` when the cache did not know it: it is pending now, to be
    /// loaded.
    pub fn want(&mut self, key: &ThumbKey) -> bool {
        self.tick += 1;
        let tick = self.tick;
        if let Some(slot) = self.slots.get_mut(key) {
            if let Some(key) = self.order.remove(&slot.used) {
                self.order.insert(tick, key);
            }
            slot.used = tick;
            return false;
        }
        self.slots.insert(
            key.clone(),
            Slot {
                state: State::Pending,
                used: tick,
            },
        );
        self.order.insert(tick, key.clone());
        self.evict();
        true
    }

    /// The tile of `key`, if it has one.
    pub fn get(&self, key: &ThumbKey) -> Option<&V> {
        match self.slots.get(key) {
            Some(Slot {
                state: State::Ready(value, _),
                ..
            }) => Some(value),
            _ => None,
        }
    }

    /// The answer for a pending `key`: its tile and the tile's bytes, or `None` when it has none.
    /// An answer for a key no longer pending (dropped, or the cache was cleared) is dropped.
    pub fn fill(&mut self, key: &ThumbKey, value: Option<(V, usize)>) {
        let Some(slot) = self.slots.get_mut(key) else {
            return;
        };
        if !matches!(slot.state, State::Pending) {
            return;
        }
        slot.state = match value {
            Some((value, bytes)) => {
                self.held += bytes;
                State::Ready(value, bytes)
            }
            None => State::Missing,
        };
        self.evict();
    }

    /// A pending `key` the loader passed over: forgotten, so it is asked for again while shown.
    pub fn forget(&mut self, key: &ThumbKey) {
        if self
            .slots
            .get(key)
            .is_some_and(|slot| matches!(slot.state, State::Pending))
            && let Some(slot) = self.slots.remove(key)
        {
            self.order.remove(&slot.used);
        }
    }

    /// Everything goes, pending keys too: their answers are dropped when they come.
    pub fn clear(&mut self) {
        self.slots.clear();
        self.order.clear();
        self.held = 0;
    }

    /// The bytes the tiles hold.
    #[cfg(test)]
    pub fn held(&self) -> usize {
        self.held
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// The keys in the order they go, oldest first.
    #[cfg(test)]
    pub fn oldest_first(&self) -> Vec<&ThumbKey> {
        self.order.values().collect()
    }

    /// Drops the least recently wanted keys that are not pending until the tiles fit their
    /// bytes and the map its keys. The tiles of one screen are the newest, so with a budget above
    /// a screen's they stay. The walk from the oldest passes over the pending keys only - the
    /// loads of about one screen.
    fn evict(&mut self) {
        while self.held > self.budget || self.slots.len() > MAX_KEYS {
            let oldest = self
                .order
                .iter()
                .find(|(_, key)| {
                    self.slots
                        .get(*key)
                        .is_some_and(|slot| !matches!(slot.state, State::Pending))
                })
                .map(|(tick, _)| *tick);
            let Some(key) = oldest.and_then(|tick| self.order.remove(&tick)) else {
                return;
            };
            if let Some(Slot {
                state: State::Ready(_, bytes),
                ..
            }) = self.slots.remove(&key)
            {
                self.held -= bytes;
            }
        }
    }
}

#[cfg(test)]
#[path = "cache_tests.rs"]
mod tests;
