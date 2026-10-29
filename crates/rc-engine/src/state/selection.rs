//! A panel's selected names, shared copy-on-write (RC-080-45): a copy of the panel's state or a
//! snapshot of it shares the one set, so a cursor step, a command's answer or the window's copy
//! of the state never copies a selection of 100 000 names. The set is copied only when it
//! changes while a copy still holds it.

use std::{
    borrow::Borrow,
    collections::{BTreeSet, HashMap, btree_set},
    fmt,
    ops::Deref,
    sync::Arc,
};

use rc_vfs::Entry;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The selected names, sorted. Reads go through the set ([`Deref`]); a change copies it only
/// while it is shared.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Selection(Arc<BTreeSet<String>>);

impl Selection {
    /// Adds `name`; `false` when it was selected already (and nothing is copied).
    pub fn insert(&mut self, name: String) -> bool {
        if self.0.contains(&name) {
            return false;
        }
        Arc::make_mut(&mut self.0).insert(name)
    }

    /// Removes `name`; `false` when it was not selected (and nothing is copied).
    pub fn remove<Q>(&mut self, name: &Q) -> bool
    where
        String: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        if !self.0.contains(name) {
            return false;
        }
        Arc::make_mut(&mut self.0).remove(name)
    }

    /// Selects nothing.
    pub fn clear(&mut self) {
        if !self.0.is_empty() {
            *self = Self::default();
        }
    }

    /// Keeps the names `keep` answers `true` for.
    pub fn retain(&mut self, keep: impl FnMut(&String) -> bool) {
        if !self.0.is_empty() {
            Arc::make_mut(&mut self.0).retain(keep);
        }
    }

    /// Whether `other` is this very set, not a copy of it.
    #[cfg(test)]
    pub(crate) fn shares(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    /// The names that `listed` still has; the same shared set when every one of them is there.
    pub(crate) fn still_listed(&self, listed: &HashMap<&str, usize>) -> Self {
        if self.0.iter().all(|name| listed.contains_key(name.as_str())) {
            return self.clone();
        }
        self.0
            .iter()
            .filter(|name| listed.contains_key(name.as_str()))
            .cloned()
            .collect()
    }
}

/// The index of every entry by its name, the first one of a name - what
/// [`crate::PanelState::index_of`] answers for one name, built once for many (RC-080-45).
pub(crate) fn by_name(entries: &[Entry]) -> HashMap<&str, usize> {
    let mut index = HashMap::with_capacity(entries.len());
    for (at, entry) in entries.iter().enumerate() {
        index.entry(entry.name.as_str()).or_insert(at);
    }
    index
}

/// The first of `names` that no entry has: one pass over the entries for one name, an index of
/// them for several.
pub(crate) fn unlisted<'a>(entries: &[Entry], names: &'a [String]) -> Option<&'a String> {
    if let [name] = names {
        return (!entries.iter().any(|entry| entry.name == *name)).then_some(name);
    }
    let listed = by_name(entries);
    names
        .iter()
        .find(|name| !listed.contains_key(name.as_str()))
}

impl Deref for Selection {
    type Target = BTreeSet<String>;

    fn deref(&self) -> &BTreeSet<String> {
        &self.0
    }
}

impl fmt::Debug for Selection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl From<BTreeSet<String>> for Selection {
    fn from(names: BTreeSet<String>) -> Self {
        Self(Arc::new(names))
    }
}

impl<const N: usize> From<[String; N]> for Selection {
    fn from(names: [String; N]) -> Self {
        names.into_iter().collect()
    }
}

impl FromIterator<String> for Selection {
    fn from_iter<I: IntoIterator<Item = String>>(names: I) -> Self {
        Self(Arc::new(names.into_iter().collect()))
    }
}

impl IntoIterator for Selection {
    type Item = String;
    type IntoIter = btree_set::IntoIter<String>;

    /// The names; copied only when another copy still shares them.
    fn into_iter(self) -> Self::IntoIter {
        Arc::unwrap_or_clone(self.0).into_iter()
    }
}

impl<'a> IntoIterator for &'a Selection {
    type Item = &'a String;
    type IntoIter = btree_set::Iter<'a, String>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

/// The selected names compared with a list in their sorted order, as a command's answer was
/// compared when it held a `Vec`.
impl<const N: usize> PartialEq<[&str; N]> for Selection {
    fn eq(&self, other: &[&str; N]) -> bool {
        self.0.len() == N && self.0.iter().zip(other).all(|(name, other)| name == other)
    }
}

impl Serialize for Selection {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Selection {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        BTreeSet::deserialize(deserializer).map(Self::from)
    }
}

#[cfg(test)]
#[path = "selection_tests.rs"]
mod tests;
