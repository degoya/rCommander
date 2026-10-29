//! The self-update's window half through `apply`, against a real engine without network: the
//! notice's texts, "What's new" opening the Updates page, the page taking the engine's status.
//! Whether a notice shows at all is the engine's `update_notice_of` (`rc-engine`'s tests).

use rc_engine::{DownloadState, UpdateNotice, UpdateStatus};

use super::{UpdateMsg, notice_line};
use crate::app::{Message, harness::Harness, settings::Page};

fn status() -> UpdateStatus {
    UpdateStatus {
        running_version: "1.0.0".to_owned(),
        channel: "stable".to_owned(),
        channel_chosen: false,
        check_enabled: true,
        interval_hours: 24,
        source: "official".to_owned(),
        last_check: None,
        last_code: Some("update.not_published".to_owned()),
        offer: None,
        download: DownloadState::None,
        install_kind: "unknown".to_owned(),
        install_reason: Some("update.kind_no_version_file".to_owned()),
        installing: false,
        queue_busy: false,
        last_update: None,
        updated_to: None,
        auto_install: "off".to_owned(),
        auto_waiting_for: None,
        leftover_bytes: 0,
    }
}

#[test]
fn every_notice_names_its_version() {
    let notices = [
        UpdateNotice::Available {
            version: "9.1.0".to_owned(),
            on_quit: false,
        },
        UpdateNotice::Available {
            version: "9.1.0".to_owned(),
            on_quit: true,
        },
        UpdateNotice::UpdatedTo {
            version: "9.1.0".to_owned(),
            notes: String::new(),
        },
        UpdateNotice::Failed {
            version: "9.1.0".to_owned(),
            code: "update.not_confirmed".to_owned(),
        },
    ];
    let mut lines = Vec::new();
    for notice in &notices {
        let (line, action) = notice_line(notice);
        assert!(line.contains("9.1.0"), "{line}");
        assert!(!action.is_empty());
        lines.push(line);
    }
    // "Installs when quitting" says so; the plain offer does not.
    assert_ne!(lines[0], lines[1]);
}

#[tokio::test]
async fn without_an_offer_there_is_no_notice() {
    let (_root, harness) = Harness::temp_with_db().await;
    assert_eq!(harness.app.engine().update_notice(), None);
}

#[tokio::test]
async fn whats_new_opens_the_updates_page_and_it_takes_the_engines_status() {
    let (_root, mut harness) = Harness::temp_with_db().await;
    harness.send(Message::Update(UpdateMsg::WhatsNew)).await;
    let dialog = harness
        .app
        .settings
        .as_ref()
        .expect("the settings are open");
    assert_eq!(dialog.page(), Page::Updates);
    harness
        .send(Message::Update(UpdateMsg::Status(Box::new(status()))))
        .await;
    let shown = harness
        .app
        .settings
        .as_ref()
        .and_then(|dialog| dialog.updates())
        .expect("the page has the status");
    assert_eq!(shown.last_code.as_deref(), Some("update.not_published"));
}

#[test]
fn every_update_code_has_a_text() {
    let codes = rc_engine::UpdateRefusal::all_codes()
        .into_iter()
        .chain(rc_update::install_kind::KIND_REASONS.iter().copied());
    for code in codes {
        let text = crate::i18n::error_text(code);
        assert_ne!(text, code, "error.{code} has no text in locales/en.json");
    }
}
