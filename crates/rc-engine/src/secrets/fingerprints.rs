//! What [`crate::Engine::scrub_secrets`] knows of the secrets it hides: each secret stored or read
//! in this process as its length and a keyed hash - never its value - so a password that reached
//! a terminal screen is still found in an MCP answer after the vault locked (RC-080-11, V3).
//!
//! The keyed hash is SipHash under a random key drawn once per process (`std`'s `RandomState`);
//! the key stays in memory and goes nowhere, so the fingerprints say nothing outside this process,
//! and a memory dump holds no secret's value, only fingerprints each guess has to be hashed
//! against. Finding them is one pass over the text per distinct length: a rolling polynomial hash
//! (Rabin-Karp modulo 2^61 - 1, its base random per process too) marks the windows worth a look
//! by 16 of its bits - too few to say anything of a secret - and only those are hashed and
//! compared. At most [`MAX_FINGERPRINTS`] are kept; the one seen longest ago goes first, a secret
//! seen again counting as new (RC-080-47).

use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    hash::{BuildHasher, RandomState},
    ops::Range,
};

/// The most fingerprints kept: a few dozen passwords and keys, each key by its lines too.
pub(crate) const MAX_FINGERPRINTS: usize = 1024;

/// The rolling hash's modulus, the Mersenne prime 2^61 - 1.
const MODULUS: u64 = (1 << 61) - 1;

/// One secret (or one line of a key): its length in bytes and its hashes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Fingerprint {
    len: usize,
    /// The low 16 bits of the rolling hash: which windows are worth hashing.
    tag: u16,
    /// The keyed hash that decides.
    hash: u64,
}

/// The fingerprints of one length.
#[derive(Default)]
struct Group {
    hashes: HashSet<u64>,
    /// How many fingerprints carry each tag.
    tags: HashMap<u16, usize>,
}

/// The fingerprints of the secrets this process has seen, by length.
pub(crate) struct Fingerprints {
    keys: RandomState,
    base: u64,
    groups: BTreeMap<usize, Group>,
    order: VecDeque<Fingerprint>,
}

impl Default for Fingerprints {
    fn default() -> Self {
        let keys = RandomState::new();
        // A base in [256, MODULUS): larger than any byte, drawn from the same random keys.
        let base = 256 + keys.hash_one(MODULUS) % (MODULUS - 256);
        Self {
            keys,
            base,
            groups: BTreeMap::new(),
            order: VecDeque::new(),
        }
    }
}

impl Fingerprints {
    /// Keeps `fragment` (as its fingerprint) for [`Fingerprints::scrub`], or makes it the newest
    /// when it is kept already; the one seen longest ago goes when [`MAX_FINGERPRINTS`] are kept.
    pub(crate) fn remember(&mut self, fragment: &str) {
        let bytes = fragment.as_bytes();
        if bytes.is_empty() {
            return;
        }
        let print = Fingerprint {
            len: bytes.len(),
            tag: tag(self.rolling(bytes)),
            hash: self.keys.hash_one(bytes),
        };
        let group = self.groups.entry(print.len).or_default();
        if !group.hashes.insert(print.hash) {
            // Seen again: it moves to the back, so a secret in use is not the next to go. At most
            // MAX_FINGERPRINTS to walk.
            if let Some(at) = self.order.iter().position(|kept| *kept == print) {
                self.order.remove(at);
                self.order.push_back(print);
            }
            return;
        }
        *group.tags.entry(print.tag).or_default() += 1;
        self.order.push_back(print);
        while self.order.len() > MAX_FINGERPRINTS {
            if let Some(oldest) = self.order.pop_front() {
                self.forget(oldest);
            }
        }
    }

    /// How many fingerprints are kept.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.order.len()
    }

    fn forget(&mut self, print: Fingerprint) {
        let Some(group) = self.groups.get_mut(&print.len) else {
            return;
        };
        group.hashes.remove(&print.hash);
        if let Some(count) = group.tags.get_mut(&print.tag) {
            *count -= 1;
            if *count == 0 {
                group.tags.remove(&print.tag);
            }
        }
        if group.hashes.is_empty() {
            self.groups.remove(&print.len);
        }
    }

    /// `text` with every remembered fragment replaced by `replacement`; `None` when none occurs.
    pub(crate) fn scrub(&self, text: &str, replacement: &str) -> Option<String> {
        let found = self.find(text);
        if found.is_empty() {
            return None;
        }
        let mut scrubbed = String::with_capacity(text.len());
        let mut done = 0;
        for range in found {
            scrubbed.push_str(&text[done..range.start]);
            scrubbed.push_str(replacement);
            done = range.end;
        }
        scrubbed.push_str(&text[done..]);
        Some(scrubbed)
    }

    /// Where remembered fragments occur in `text`: sorted, overlaps merged, on char boundaries.
    fn find(&self, text: &str) -> Vec<Range<usize>> {
        let bytes = text.as_bytes();
        let mut found = Vec::new();
        for (&len, group) in &self.groups {
            if len > bytes.len() {
                break;
            }
            // base^(len - 1): what the byte leaving the window weighed.
            let top = (1..len).fold(1, |power, _| mul(power, self.base));
            let mut rolling = self.rolling(&bytes[..len]);
            for start in 0..=bytes.len() - len {
                if start > 0 {
                    let out = mul(u64::from(bytes[start - 1]) + 1, top);
                    let into = u64::from(bytes[start + len - 1]) + 1;
                    rolling = add(mul(sub(rolling, out), self.base), into);
                }
                let end = start + len;
                if group.tags.contains_key(&tag(rolling))
                    && text.is_char_boundary(start)
                    && text.is_char_boundary(end)
                    && group
                        .hashes
                        .contains(&self.keys.hash_one(&bytes[start..end]))
                {
                    found.push(start..end);
                }
            }
        }
        found.sort_by_key(|range| range.start);
        let mut merged: Vec<Range<usize>> = Vec::with_capacity(found.len());
        for range in found {
            match merged.last_mut() {
                Some(last) if range.start < last.end => last.end = last.end.max(range.end),
                _ => merged.push(range),
            }
        }
        merged
    }

    /// The polynomial hash of `bytes` (each byte plus one, so a leading zero byte counts).
    fn rolling(&self, bytes: &[u8]) -> u64 {
        bytes.iter().fold(0, |hash, &byte| {
            add(mul(hash, self.base), u64::from(byte) + 1)
        })
    }
}

fn tag(rolling: u64) -> u16 {
    (rolling & 0xFFFF) as u16
}

/// `a * b` modulo [`MODULUS`], both below it: 2^61 is 1 there, so the high bits fold onto the
/// low ones.
fn mul(a: u64, b: u64) -> u64 {
    let product = u128::from(a) * u128::from(b);
    // The low 61 bits and the rest, each below 2^61.
    reduce((product as u64 & MODULUS) + (product >> 61) as u64)
}

fn add(a: u64, b: u64) -> u64 {
    reduce(a + b)
}

fn sub(a: u64, b: u64) -> u64 {
    reduce(a + MODULUS - b)
}

/// `value` modulo [`MODULUS`], for a value below 2^63.
fn reduce(value: u64) -> u64 {
    let folded = (value & MODULUS) + (value >> 61);
    if folded >= MODULUS {
        folded - MODULUS
    } else {
        folded
    }
}

#[cfg(test)]
#[path = "fingerprints_tests.rs"]
mod tests;
