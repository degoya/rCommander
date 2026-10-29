use super::{Lang, choose, variable_tags};

fn tags(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

#[test]
fn a_tag_is_read_by_its_language_whatever_follows_it() {
    for english in ["en_GB.UTF-8", "en-US", "EN", "en@euro"] {
        assert_eq!(Lang::of_tag(english), Some(Lang::En), "{english}");
    }
    for other in ["C", "POSIX", "C.UTF-8", "de_DE.UTF-8", "fr-FR", ""] {
        assert_eq!(Lang::of_tag(other), None, "{other}");
    }
}

#[test]
fn a_variable_naming_one_of_ours_wins_and_the_system_is_not_asked() {
    let chosen = choose(&tags(&["en_US.UTF-8"]), || {
        panic!("the system is asked although a variable names a language of ours")
    });
    assert_eq!(chosen, Lang::En);
}

#[test]
fn without_a_variable_of_ours_the_system_decides() {
    for variables in [tags(&[]), tags(&["C.UTF-8"]), tags(&["de_DE.UTF-8", "C"])] {
        assert_eq!(
            choose(&variables, || tags(&["de-DE", "en-GB"])),
            Lang::En,
            "{variables:?}"
        );
    }
}

/// The fallback: a language rCommander does not ship - German on a German Windows today - reads
/// as English.
#[test]
fn a_language_neither_side_knows_is_english() {
    assert_eq!(
        choose(&tags(&["de_DE"]), || tags(&["de-DE", "ja-JP"])),
        Lang::En
    );
    assert_eq!(choose(&[], Vec::new), Lang::En);
}

#[test]
fn the_variables_are_read_in_gettexts_order_and_language_lists_split() {
    let read = variable_tags(|name| match name {
        "LANG" => Some("de_DE.UTF-8".to_owned()),
        "LANGUAGE" => Some("fr:en".to_owned()),
        _ => None,
    });
    assert_eq!(read, tags(&["fr", "en", "de_DE.UTF-8"]));
    assert!(variable_tags(|_| None).is_empty());
}
