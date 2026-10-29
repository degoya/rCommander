use std::borrow::Cow;

use super::{
    ELLIPSIS, clusters, end_ellipsis_by, estimate_mono_width, estimate_width, middle_ellipsis,
    middle_ellipsis_by,
};

const SIZE: f32 = 13.0;

/// Ten pixels a character, the ellipsis included.
fn monospace(part: &str) -> f32 {
    part.chars().count() as f32 * 10.0
}

#[test]
fn a_short_text_stays_as_it_is() {
    let shown = middle_ellipsis("a.txt", 200.0, SIZE);
    assert!(matches!(shown, Cow::Borrowed("a.txt")), "{shown}");
    let exact = middle_ellipsis_by("abcde", 50.0, monospace);
    assert!(
        matches!(exact, Cow::Borrowed("abcde")),
        "fits exactly: {exact}"
    );
}

#[test]
fn the_start_and_the_end_stay() {
    let path = r"C:\Users\someone\Music\--- incoming ---\Melanie C\03-melanie_c-what_if.mp3";
    let shown = middle_ellipsis(path, 220.0, SIZE);
    assert!(shown.starts_with(r"C:\Users"), "{shown}");
    assert!(shown.ends_with(".mp3"), "{shown}");
    assert!(shown.contains(ELLIPSIS), "{shown}");
    assert!(estimate_width(&shown, SIZE) <= 220.0, "{shown}");
}

#[test]
fn the_end_gets_half_the_room_and_the_start_the_rest() {
    let shown = middle_ellipsis_by("abcdefghijklmnopqrstuvwxyz0123", 100.0, monospace);
    assert_eq!(shown, "abcde\u{2026}0123");
}

#[test]
fn the_cut_follows_the_measured_width_not_the_count() {
    let kept = |text: &str| middle_ellipsis(text, 200.0, SIZE).chars().count() - 1;
    let wide = kept(&"W".repeat(60));
    let narrow = kept(&"i".repeat(60));
    assert!(
        narrow > 2 * wide,
        "{narrow} narrow, {wide} wide characters kept"
    );
    assert!(estimate_width("W", SIZE) > estimate_width("i", SIZE));
    assert!(estimate_width("\u{6587}", SIZE) > estimate_width("a", SIZE));
    assert_eq!(estimate_width("ab", 26.0), 2.0 * estimate_width("ab", SIZE));
}

#[test]
fn a_cluster_is_never_split() {
    let one = "\u{e4}\u{f6}\u{fc}-e\u{301}-\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}-\u{6587}\u{4EF6}-x\u{FE0F}-\u{1F44D}\u{1F3FD}";
    let text = one.repeat(3);
    let mut bounds: Vec<usize> = clusters(&text).iter().map(|&(start, _)| start).collect();
    bounds.push(text.len());
    for step in 0..=80 {
        let max = step as f32 * 5.0;
        let shown = middle_ellipsis(&text, max, SIZE);
        assert!(estimate_width(&shown, SIZE) <= max + 1e-3 || shown == ELLIPSIS);
        let Some((head, tail)) = shown.split_once(ELLIPSIS) else {
            assert_eq!(shown, text);
            continue;
        };
        assert!(text.starts_with(head) && text.ends_with(tail), "{shown}");
        assert!(
            bounds.contains(&head.len()),
            "start cut inside a cluster: {shown}"
        );
        assert!(
            bounds.contains(&(text.len() - tail.len())),
            "end cut inside a cluster: {shown}"
        );
    }
}

#[test]
fn joining_characters_stay_with_their_base() {
    assert_eq!(clusters("e\u{301}a").len(), 2);
    assert_eq!(
        clusters("\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}").len(),
        1
    );
    assert_eq!(clusters("x\u{FE0F}\u{1F44D}\u{1F3FD}").len(), 2);
    assert_eq!(clusters("\u{e4}\u{6587}").len(), 2);
    assert!(clusters("").is_empty());
}

#[test]
fn without_room_only_the_ellipsis_is_left() {
    assert_eq!(middle_ellipsis("a long file name.txt", 1.0, SIZE), ELLIPSIS);
    assert_eq!(middle_ellipsis_by("abcdef", 10.0, monospace), ELLIPSIS);
}

#[test]
fn a_monospaced_estimate_counts_cells_not_shapes() {
    assert_eq!(
        estimate_mono_width("iiii", SIZE),
        estimate_mono_width("MMMM", SIZE),
        "every character one advance"
    );
    assert!(estimate_mono_width("illi", SIZE) > estimate_width("illi", SIZE));
    assert_eq!(
        estimate_mono_width("\u{6587}", SIZE),
        estimate_mono_width("ab", SIZE),
        "a wide character takes two cells"
    );
    assert_eq!(
        estimate_mono_width("e\u{301}", SIZE),
        estimate_mono_width("e", SIZE)
    );
}

/// Cut at the end: the start stays as far as it fits beside the ellipsis, never a part of a
/// character; a name that fits stays as it is.
#[test]
fn the_end_cut_keeps_the_start() {
    let fits = end_ellipsis_by("abcde", 50.0, monospace);
    assert!(matches!(fits, Cow::Borrowed("abcde")), "{fits}");
    assert_eq!(
        end_ellipsis_by("abcdefgh", 50.0, monospace),
        format!("abcd{ELLIPSIS}")
    );
    assert_eq!(
        end_ellipsis_by("e\u{301}xyz", 35.0, monospace),
        format!("e\u{301}{ELLIPSIS}")
    );
    assert_eq!(end_ellipsis_by("abcdef", 5.0, monospace), ELLIPSIS);
}
