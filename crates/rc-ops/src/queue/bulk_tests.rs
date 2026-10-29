use super::shift;
use crate::Reorder;

/// Items `(name, list)`; the chosen ones are those named in `chosen`.
fn moved(items: &[(char, u64)], chosen: &str, step: Reorder) -> String {
    let mut items = items.to_vec();
    let marks = items
        .iter()
        .map(|(name, _)| chosen.contains(*name))
        .collect();
    shift(&mut items, marks, step, |(_, list)| *list);
    items.iter().map(|(name, _)| *name).collect()
}

fn one_list(names: &str) -> Vec<(char, u64)> {
    names.chars().map(|name| (name, 0)).collect()
}

#[test]
fn up_and_down_move_each_chosen_one_place_keeping_their_order() {
    let items = one_list("abcde");
    assert_eq!(moved(&items, "bd", Reorder::Up), "badce");
    assert_eq!(moved(&items, "bd", Reorder::Down), "acbed");
    // A block moves as one.
    assert_eq!(moved(&items, "cd", Reorder::Up), "acdbe");
    assert_eq!(moved(&items, "cd", Reorder::Down), "abecd");
}

#[test]
fn a_chosen_one_at_the_edge_holds_the_ones_behind_it() {
    let items = one_list("abcde");
    assert_eq!(moved(&items, "abd", Reorder::Up), "abdce");
    assert_eq!(moved(&items, "bde", Reorder::Down), "acbde");
}

#[test]
fn front_and_back_move_the_chosen_as_one_block_in_their_order() {
    let items = one_list("abcde");
    assert_eq!(moved(&items, "dbe", Reorder::Front), "bdeac");
    assert_eq!(moved(&items, "ad", Reorder::Back), "bcead");
    assert_eq!(moved(&items, "", Reorder::Front), "abcde");
}

/// Up and down pass over the jobs of other lists: each list keeps its own order.
#[test]
fn up_and_down_pass_over_other_lists() {
    // a, b and c of list 1, p and q of list 2, side by side in one lane.
    let items = [('a', 1), ('p', 2), ('b', 1), ('q', 2), ('c', 1)];
    assert_eq!(moved(&items, "c", Reorder::Up), "apcqb");
    assert_eq!(moved(&items, "bc", Reorder::Up), "bpcqa");
    assert_eq!(moved(&items, "pa", Reorder::Down), "bqapc");
}
