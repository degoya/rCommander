//! Properties of the rendering (RC-110-70): any name, any extension, any range - the examples
//! of `render_tests.rs` are the cases Total Commander documents, these hold for every input.

use proptest::prelude::*;

use super::{Fields, Switches, Times, path_levels, render, slice};
use crate::clock::{DEFAULT_DATE_FORMAT, DEFAULT_TIME_FORMAT};
use crate::pattern::{Range, parse};
use crate::plan::CounterSettings;

/// `name.extension` in `C:\a\b`, without a time: a mask with a date does not render.
fn fields<'a>(name: &'a str, extension: &'a str, file_name: &'a str) -> Fields<'a> {
    Fields {
        name,
        extension,
        file_name,
        levels: &["a", "b"],
        from_root: &["C", "a", "b"],
        size: None,
        is_dir: false,
        index: 0,
        count: 1,
        counter: CounterSettings::default(),
        times: Times::default(),
        clipboard: "",
        date_format: DEFAULT_DATE_FORMAT,
        time_format: DEFAULT_TIME_FORMAT,
    }
}

/// `mask` rendered for `name.extension`.
fn rendered(mask: &str, name: &str, extension: &str) -> String {
    let pattern = parse(mask).unwrap_or_else(|error| panic!("{mask:?}: {error}"));
    let file_name = format!("{name}.{extension}");
    render(
        &pattern,
        &fields(name, extension, &file_name),
        &mut Switches::default(),
    )
    .unwrap_or_else(|missing| panic!("{mask:?}: {missing:?}"))
}

/// `text` as a mask that renders it literally: `[` and `]` written as `[[]` and `[]]`.
fn escaped(text: &str) -> String {
    let mut mask = String::new();
    for c in text.chars() {
        match c {
            '[' => mask.push_str("[[]"),
            ']' => mask.push_str("[]]"),
            c => mask.push(c),
        }
    }
    mask
}

fn chars(text: &str) -> Vec<char> {
    text.chars().collect()
}

proptest! {
    /// Any text, brackets escaped, is a mask of literals that renders as the text.
    #[test]
    fn an_escaped_text_renders_as_itself(text in any::<String>()) {
        prop_assert_eq!(rendered(&escaped(&text), "n", "e"), text);
    }

    /// `[N].[E]` gives the name back whatever its characters; `[U]` and `[L]` change only the
    /// case, as Rust's Unicode mapping does.
    #[test]
    fn name_and_extension_render_unchanged_or_in_one_case(
        name in any::<String>(),
        extension in any::<String>(),
    ) {
        prop_assert_eq!(
            rendered("[N].[E]", &name, &extension),
            format!("{name}.{extension}")
        );
        prop_assert_eq!(rendered("[U][N]", &name, &extension), name.to_uppercase());
        prop_assert_eq!(rendered("[L][E]", &name, &extension), extension.to_lowercase());
    }

    /// A range takes a run of the text's characters, never more than it asks for.
    #[test]
    fn a_range_is_a_run_of_the_text(
        text in any::<String>(),
        from in -40_i64..40,
        to in proptest::option::of(-40_i64..40),
        count in 0_u64..40,
    ) {
        let all = chars(&text);
        for range in [
            Range::At(from),
            Range::Span(from, to),
            Range::Count(from, count),
        ] {
            let part = chars(&slice(&text, range));
            prop_assert!(
                part.is_empty() || all.windows(part.len()).any(|run| run == part.as_slice()),
                "{:?} of {:?} is {:?}", range, text, part
            );
            if let Range::Count(_, count) = range {
                prop_assert!(part.len() as u64 <= count);
            }
            if let Range::At(_) = range {
                prop_assert!(part.len() <= 1);
            }
        }
    }

    /// Positions count from 1 on the left and from -1 on the right; the whole text and a cut
    /// anywhere give the text back.
    #[test]
    fn positions_count_from_both_ends(text in ".{1,30}", pick in any::<prop::sample::Index>()) {
        let all = chars(&text);
        let len = all.len();
        let at = pick.index(len);
        let left = i64::try_from(at + 1).expect("small");
        let right = i64::try_from(len - at).expect("small");
        prop_assert_eq!(slice(&text, Range::All), text.clone());
        prop_assert_eq!(slice(&text, Range::Span(1, None)), text.clone());
        prop_assert_eq!(slice(&text, Range::At(left)), all[at].to_string());
        prop_assert_eq!(slice(&text, Range::At(-right)), all[at].to_string());
        let cut = slice(&text, Range::Span(1, Some(left)))
            + &slice(&text, Range::Span(left + 1, None));
        prop_assert_eq!(cut, text.clone());
        prop_assert_eq!(
            chars(&slice(&text, Range::Count(left, u64::MAX))),
            all[at..].to_vec()
        );
    }

    /// The folders of a path are never empty, `.`, `?` or hold a separator, whatever the path.
    #[test]
    fn path_levels_are_plain_names(dir in "[a-z:?./\\\\]{0,30}") {
        for level in path_levels(&dir) {
            prop_assert!(!level.is_empty() && level != "." && level != "?", "{:?}", dir);
            prop_assert!(!level.contains(['/', '\\']), "{:?}", dir);
        }
    }
}
