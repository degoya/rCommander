use super::offer_order;

fn listed(agents: &[&[&str]]) -> Vec<Vec<String>> {
    agents
        .iter()
        .map(|keys| keys.iter().map(|key| (*key).to_owned()).collect())
        .collect()
}

#[test]
fn every_key_in_the_agents_order_within_the_limit() {
    let listed = listed(&[&["a", "b"], &["c"]]);
    assert_eq!(
        offer_order(&listed, None, None, 5),
        [(0, 0), (0, 1), (1, 0)]
    );
    assert_eq!(offer_order(&listed, None, None, 2), [(0, 0), (0, 1)]);
}

/// RC-080-43 (S8): an agent with many keys does not use up the server's tries before the
/// connection's own key: it comes first, wherever it is listed.
#[test]
fn the_connections_own_key_comes_first() {
    let listed = listed(&[&["a", "b", "c", "d", "e", "f"], &["own"]]);
    assert_eq!(
        offer_order(&listed, None, Some("own"), 3),
        [(1, 0), (0, 0), (0, 1)]
    );
    // Not held by any agent: the order stays.
    assert_eq!(
        offer_order(&listed, None, Some("other"), 2),
        [(0, 0), (0, 1)]
    );
}

#[test]
fn a_key_two_agents_hold_is_offered_once() {
    let listed = listed(&[&["a", "own"], &["own", "b"]]);
    assert_eq!(
        offer_order(&listed, None, Some("own"), 5),
        [(0, 1), (0, 0), (1, 1)]
    );
}

#[test]
fn a_chosen_key_is_the_only_one_offered() {
    let listed = listed(&[&["a", "b"], &["b"]]);
    assert_eq!(offer_order(&listed, Some("b"), Some("a"), 5), [(0, 1)]);
    assert!(offer_order(&listed, Some("x"), None, 5).is_empty());
}

#[test]
fn a_limit_of_zero_offers_nothing() {
    assert!(offer_order(&listed(&[&["a"]]), None, None, 0).is_empty());
}
