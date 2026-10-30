use super::{Lang, choose, variable_tags};

fn tags(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

#[test]
fn a_tag_is_read_by_its_language_whatever_follows_it() {
    for english in ["en_GB.UTF-8", "en-US", "EN", "en@euro"] {
        assert_eq!(Lang::of_tag(english), Some(Lang::En), "{english}");
    }
    for german in ["de_DE.UTF-8", "de-AT", "DE", "de_CH@euro"] {
        assert_eq!(Lang::of_tag(german), Some(Lang::De), "{german}");
    }
    for french in ["fr_FR.UTF-8", "fr-CA", "fr"] {
        assert_eq!(Lang::of_tag(french), Some(Lang::Fr), "{french}");
    }
    for spanish in ["es_ES.UTF-8", "es-419", "es"] {
        assert_eq!(Lang::of_tag(spanish), Some(Lang::Es), "{spanish}");
    }
    for other in ["C", "POSIX", "C.UTF-8", "it_IT.UTF-8", "ja-JP", "deu", ""] {
        assert_eq!(Lang::of_tag(other), None, "{other}");
    }
}

/// The setting's values are the row's choices: `auto` and anything else leave the desktop's.
#[test]
fn the_setting_names_a_language_or_leaves_it_to_the_desktop() {
    assert_eq!(Lang::of_setting("en"), Some(Lang::En));
    assert_eq!(Lang::of_setting("de"), Some(Lang::De));
    assert_eq!(Lang::of_setting("fr"), Some(Lang::Fr));
    assert_eq!(Lang::of_setting("es"), Some(Lang::Es));
    for none in ["auto", "", "de_DE", "it"] {
        assert_eq!(Lang::of_setting(none), None, "{none}");
    }
}

#[test]
fn a_variable_naming_one_of_ours_wins_and_the_system_is_not_asked() {
    let chosen = choose(&tags(&["de_DE.UTF-8"]), || {
        panic!("the system is asked although a variable names a language of ours")
    });
    assert_eq!(chosen, Lang::De);
}

#[test]
fn without_a_variable_of_ours_the_system_decides() {
    for variables in [tags(&[]), tags(&["C.UTF-8"]), tags(&["it_IT.UTF-8", "C"])] {
        assert_eq!(
            choose(&variables, || tags(&["ja-JP", "fr-FR", "en-GB"])),
            Lang::Fr,
            "{variables:?}"
        );
    }
}

/// The fallback: a language rCommander does not ship - Italian on an Italian Windows - reads as
/// English.
#[test]
fn a_language_neither_side_knows_is_english() {
    assert_eq!(
        choose(&tags(&["it_IT"]), || tags(&["it-IT", "ja-JP"])),
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
