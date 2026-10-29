use super::*;

fn chord(text: &str) -> Chord {
    text.parse().expect("a chord")
}

#[test]
fn the_text_reads_and_prints_the_same() {
    let overrides = Overrides::parse("Ctrl+K=copy  F5=\tctrl+shift+j=move").expect("reads");
    assert_eq!(
        overrides.rows(),
        [
            Override::Bind {
                chord: chord("Ctrl+K"),
                name: "copy".to_owned()
            },
            Override::Unbind { chord: chord("F5") },
            Override::Bind {
                chord: chord("Ctrl+Shift+J"),
                name: "move".to_owned()
            },
        ]
    );
    assert_eq!(overrides.to_string(), "Ctrl+K=copy F5= Ctrl+Shift+J=move");
    assert_eq!(
        Overrides::parse(&overrides.to_string()).expect("again"),
        overrides
    );
    assert!(Overrides::parse("").expect("empty").is_empty());
    assert!(Overrides::parse("  ").expect("blank").is_empty());
}

/// A chord may end in `=`; a name never holds one.
#[test]
fn a_chord_ending_in_equals_reads() {
    let overrides = Overrides::parse("Ctrl+==copy Ctrl+Shift+==").expect("reads");
    assert_eq!(
        overrides.rows(),
        [
            Override::Bind {
                chord: chord("Ctrl+="),
                name: "copy".to_owned()
            },
            Override::Unbind {
                chord: chord("Ctrl+Shift+=")
            },
        ]
    );
}

#[test]
fn a_wrong_word_is_refused() {
    for text in [
        "F5",
        "Hyper+F5=copy",
        "F5=Copy",
        "F5=copy!",
        "Ctrl+K=copy F99=",
    ] {
        assert!(
            matches!(Overrides::parse(text), Err(KeymapError::Override(_))),
            "{text}: {:?}",
            Overrides::parse(text)
        );
    }
    assert!(matches!(
        Overrides::parse("F5= F5=copy"),
        Err(KeymapError::Duplicate { .. })
    ));
}

/// The window's own keys never reach the setting, not even through an import.
#[test]
fn a_locked_key_is_refused() {
    for text in [
        "Tab=copy",
        "Shift+Tab=",
        "Escape=copy",
        "Alt+F=copy",
        "Alt+Shift+7=move",
        "Ctrl+Shift+P=copy",
        "Ctrl+C=",
    ] {
        assert!(
            matches!(Overrides::parse(text), Err(KeymapError::Locked { .. })),
            "{text}: {:?}",
            Overrides::parse(text)
        );
    }
}

#[test]
fn too_many_words_or_too_long_a_text_are_refused() {
    // One CJK character after another: each a chord of its own.
    let many: Vec<String> = (0x4e00_u32..)
        .filter_map(char::from_u32)
        .take(MAX_OVERRIDES + 1)
        .map(|key| format!("Ctrl+{key}="))
        .collect();
    assert!(matches!(
        Overrides::parse(&many.join(" ")),
        Err(KeymapError::TooLong)
    ));
    let long = " ".repeat(MAX_OVERRIDE_TEXT + 1);
    assert!(matches!(Overrides::parse(&long), Err(KeymapError::TooLong)));
}

#[test]
fn the_standard_differs_from_itself_in_nothing() {
    let standard = Keymap::standard();
    assert!(Overrides::between(&standard, &standard).is_empty());
    assert!(standard.overrides().is_empty());
}
