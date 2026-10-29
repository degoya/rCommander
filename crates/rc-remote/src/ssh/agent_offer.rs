//! Which agent keys a login offers, in which order (RC-080-43): the connection's own key first,
//! then the others in the agents' order, each key once and at most as many as the setting
//! allows - a server counts every key offered against its `MaxAuthTries` and learns each one.

use std::collections::HashSet;

/// The keys to offer as `(agent, key)` indices into `listed`, the fingerprints each agent lists:
/// only `chosen` when it is set, `own` first when an agent holds it, at most `max`.
pub(super) fn offer_order(
    listed: &[Vec<String>],
    chosen: Option<&str>,
    own: Option<&str>,
    max: u32,
) -> Vec<(usize, usize)> {
    let keys: Vec<(usize, usize, &str)> = listed
        .iter()
        .enumerate()
        .flat_map(|(agent, keys)| {
            keys.iter()
                .enumerate()
                .map(move |(key, fingerprint)| (agent, key, fingerprint.as_str()))
        })
        .filter(|(_, _, fingerprint)| chosen.is_none_or(|chosen| chosen == *fingerprint))
        .collect();
    let max = usize::try_from(max).unwrap_or(usize::MAX);
    let mut seen = HashSet::new();
    let mut order = Vec::new();
    let own_first = keys
        .iter()
        .filter(|(_, _, fingerprint)| own == Some(*fingerprint));
    for (agent, key, fingerprint) in own_first.chain(keys.iter()) {
        if order.len() == max {
            break;
        }
        if seen.insert(*fingerprint) {
            order.push((*agent, *key));
        }
    }
    order
}

#[cfg(test)]
#[path = "agent_offer_tests.rs"]
mod tests;
