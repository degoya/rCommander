use rc_engine::{Command, Outcome};

use super::{AboutMsg, AboutTab, facts, shown, version_line, window_title};
use crate::app::{
    Message,
    harness::Harness,
    palette::PaletteMsg,
    settings::{Page, SettingsMessage},
};

/// `app.about` answers MCP what the page shows.
#[tokio::test]
async fn app_about_answers_what_the_page_shows() {
    let (_root, harness) = Harness::temp_with_db().await;
    let outcome = harness
        .app
        .engine()
        .dispatch(Command::About {})
        .await
        .expect("answers");
    let Outcome::About(answer) = outcome else {
        panic!("{outcome:?}");
    };
    assert_eq!(answer, shown());
    let facts = facts(&answer);
    assert_eq!(facts[0].1, env!("CARGO_PKG_VERSION"));
    assert_eq!(facts[3].1, answer.platform);
    let line = version_line(&answer);
    assert!(line.starts_with("rCommander "), "{line}");
    for (_, value) in &facts {
        assert!(line.contains(value.as_str()), "{line} lacks {value}");
    }
}

/// The title names the program as Total Commander's does, never a path.
#[test]
fn the_window_title_is_name_version_and_author() {
    let about = shown();
    assert_eq!(
        window_title(),
        format!(
            "rCommander {} - \u{a9} Alexander Herling",
            env!("CARGO_PKG_VERSION")
        )
    );
    assert_eq!(about.version, env!("CARGO_PKG_VERSION"));
}

#[tokio::test]
async fn the_page_opens_from_the_settings_and_keeps_its_tab_and_filter() {
    let (_root, mut harness) = Harness::temp_with_db().await;
    harness.send(Message::Settings(SettingsMessage::Open)).await;
    harness
        .send(Message::Settings(SettingsMessage::Page(Page::About)))
        .await;
    let dialog = harness.app.settings.as_ref().expect("open");
    assert_eq!(dialog.page, Page::About);
    assert_eq!(Page::ALL.last(), Some(&Page::About), "the last page");

    harness
        .send(Message::Settings(SettingsMessage::About(AboutMsg::Tab(
            AboutTab::Licenses,
        ))))
        .await;
    harness
        .send(Message::Settings(SettingsMessage::About(AboutMsg::Filter(
            "mit".into(),
        ))))
        .await;
    let dialog = harness.app.settings.as_ref().expect("open");
    assert_eq!(dialog.about.tab, AboutTab::Licenses);
    assert_eq!(dialog.about.filter, "mit");

    harness
        .send(Message::Settings(SettingsMessage::About(
            AboutMsg::CopyVersion,
        )))
        .await;
    let dialog = harness.app.settings.as_ref().expect("open");
    let (note, error) = dialog.note.as_ref().expect("a note");
    assert!(!error);
    assert_eq!(note, crate::i18n::t!("settings.about.version_copied"));
}

#[tokio::test]
async fn the_palette_opens_the_about_page() {
    let (_root, mut harness) = Harness::temp_with_db().await;
    harness.press("Ctrl+Shift+P").await;
    let palette = harness.app.palette.as_ref().expect("open");
    let at = palette
        .shown
        .iter()
        .position(|index| {
            palette
                .entry(*index)
                .is_some_and(|entry| entry.name == "about")
        })
        .expect("an about row");
    harness.send(Message::Palette(PaletteMsg::Pick(at))).await;
    assert!(harness.app.palette.is_none());
    let dialog = harness.app.settings.as_ref().expect("the settings open");
    assert_eq!(dialog.page, Page::About);
}

/// Ctrl+Tab and Ctrl+Shift+Tab turn the page's tabs, as on the other pages (RC-100-20).
#[tokio::test]
async fn ctrl_tab_turns_the_tabs() {
    let (_root, mut harness) = Harness::temp_with_db().await;
    harness
        .send(Message::Settings(SettingsMessage::OpenPage(Page::About)))
        .await;
    let tab = |harness: &Harness| harness.app.settings.as_ref().expect("open").about.tab;
    harness.press("Ctrl+Tab").await;
    assert_eq!(tab(&harness), AboutTab::Licenses);
    harness.press("Ctrl+Tab").await;
    assert_eq!(tab(&harness), AboutTab::About, "round");
    harness.press("Ctrl+Shift+Tab").await;
    assert_eq!(tab(&harness), AboutTab::Licenses);
}
