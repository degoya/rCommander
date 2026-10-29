use rc_engine::Chord;

use super::{Held, ImportPage, PageKey, Phase, page_key};

#[test]
fn only_the_read_asked_last_is_answered_and_a_change_drops_it() {
    let mut held = Held::default();
    assert!(!held.answers(0), "nothing asked yet");
    let first = held.ask("a".to_owned());
    let second = held.ask("b".to_owned());
    assert!(!held.answers(first), "an older read is late");
    assert!(held.answers(second));
    assert_eq!(held.source().map(String::as_str), Some("b"));

    held.forget();
    assert!(
        !held.answers(second),
        "a changed path drops the read on its way"
    );
    assert_eq!(held.source(), None);
}

#[test]
fn a_page_imports_only_the_ready_preview_and_a_failed_import_can_be_tried_again() {
    let mut page = ImportPage::new(Phase::Done);
    let read = page.read("a".to_owned());
    assert_eq!(page.phase, Phase::Reading);
    assert_eq!(page.ready_source(), None, "not read yet");
    assert!(page.held.answers(read));
    page.phase = Phase::Ready;
    assert_eq!(page.ready_source().map(String::as_str), Some("a"));
    page.failed("ssh_config_path_refused", true);
    assert_eq!(page.phase, Phase::Ready);
    assert!(page.error.is_some());
    page.failed("ssh_config_path_refused", false);
    assert_eq!(page.phase, Phase::Done);
    page.changed();
    assert_eq!((page.phase, page.error.as_deref()), (Phase::Done, None));
    assert!(
        !page.held.answers(read),
        "the read on its way no longer counts"
    );
}

#[test]
fn only_a_plain_escape_or_enter_no_field_took_works_the_page() {
    let chord = |text: &str| text.parse::<Chord>().expect("a chord");
    assert_eq!(page_key(&chord("Escape"), false), Some(PageKey::Leave));
    assert_eq!(page_key(&chord("Enter"), false), Some(PageKey::Import));
    assert_eq!(page_key(&chord("Enter"), true), None);
    assert_eq!(page_key(&chord("Ctrl+Enter"), false), None);
    assert_eq!(page_key(&chord("Tab"), false), None);
}
