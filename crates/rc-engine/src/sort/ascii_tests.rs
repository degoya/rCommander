use std::cmp::Ordering;

use proptest::prelude::*;

use super::{KEYS, compare};
use crate::sort::{Names, caseless_cmp, class, compare_names, symbol_rank, word_order};

/// Characters that tie in the first pass and differ in a later one: case, hyphens and
/// apostrophes, leading zeros, symbols of every rank, a control character and DEL.
const ASCII: &[char] = &[
    'a', 'A', 'b', 'e', 'E', 's', 'S', '0', '1', '9', '-', '\'', '_', ' ', '.', '~', '+', '\u{1}',
    '\u{7F}',
];

/// [`ASCII`] and the characters the slow path folds: accents composed and alone, the sharp s,
/// a kana voicing mark and a Hangul syllable, which keeps its order.
const WIDER: &[char] = &[
    'a', 'A', 'e', 'E', 's', 'S', '0', '1', '-', '\'', '_', ' ', '\u{E4}', '\u{C4}', '\u{E9}',
    '\u{C9}', '\u{301}', '\u{308}', '\u{DF}', '\u{1E9E}', '\u{3099}', '\u{AC00}',
];

fn name(alphabet: &'static [char]) -> impl Strategy<Value = String> {
    prop::collection::vec(prop::sample::select(alphabet), 0..10)
        .prop_map(|chars| chars.into_iter().collect::<String>())
}

/// The table holds for every pair of ASCII characters what class, symbol rank and case-folded
/// comparison decide one after the other.
#[test]
fn the_table_orders_every_ascii_pair_as_the_three_rules_do() {
    for x in 0u8..128 {
        for y in 0u8..128 {
            let (cx, cy) = (char::from(x), char::from(y));
            let rules = class(cx)
                .cmp(&class(cy))
                .then_with(|| symbol_rank(cx).cmp(&symbol_rank(cy)))
                .then_with(|| caseless_cmp(cx, cy));
            let table = KEYS[usize::from(x)].cmp(&KEYS[usize::from(y)]);
            assert_eq!(table, rules, "{cx:?} against {cy:?}");
        }
    }
}

#[test]
fn a_name_with_another_character_is_handed_back_where_it_is_reached() {
    assert_eq!(compare("b\u{E4}", "a\u{E4}", true), Some(Ordering::Greater));
    assert_eq!(compare("a\u{E4}", "a\u{E4}", true), None);
    assert_eq!(compare("abc", "abc\u{301}", true), None);
    assert_eq!(compare("don't", "dont", true), Some(Ordering::Greater));
    assert_eq!(compare("file9", "file010", true), Some(Ordering::Less));
    assert_eq!(compare("file9", "file010", false), Some(Ordering::Greater));
}

proptest! {
    /// The ASCII path decides every pair of ASCII names, as the three passes do.
    #[test]
    fn ascii_names_compare_as_the_three_passes_do(
        a in name(ASCII),
        b in name(ASCII),
        numbers in any::<bool>(),
    ) {
        prop_assert_eq!(compare(&a, &b, numbers), Some(word_order(&a, &b, numbers)));
    }

    /// Whatever the ASCII path decides before it hands a name back, the three passes decide
    /// alike; and the order names follow is theirs.
    #[test]
    fn any_names_compare_as_the_three_passes_do(
        a in name(WIDER),
        b in name(WIDER),
        numbers in any::<bool>(),
    ) {
        let slow = word_order(&a, &b, numbers);
        if let Some(fast) = compare(&a, &b, numbers) {
            prop_assert_eq!(fast, slow);
        }
        let names = Names { linux: false, numbers };
        prop_assert_eq!(compare_names(&a, &b, names), slow);
    }
}
