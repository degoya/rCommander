use rc_engine::{Drive, DriveGroup, DriveKind, DriveState, PanelId, VfsPath};

use super::{MapForm, NetworkForm, NetworkMsg, Phase, UnmapForm, free_letters};
use crate::app::{
    Effect, Message,
    drive_bar::{DriveMessage, actions::DriveAction},
    harness::Harness,
};

fn drive(id: &str, kind: DriveKind, remote: Option<&str>) -> Drive {
    Drive {
        id: id.to_owned(),
        group: DriveGroup::Drives,
        kind,
        label: id.to_owned(),
        root: Some(VfsPath::local(format!("{id}\\"))),
        state: DriveState::Unknown,
        volume: None,
        space: None,
        folder: Vec::new(),
        favourite: false,
        remote: remote.map(str::to_owned),
    }
}

/// `C:`, and the network drives `Y:` and `Z:`.
fn drives() -> Vec<Drive> {
    vec![
        drive("C:", DriveKind::Fixed, None),
        drive("Y:", DriveKind::Network, Some(r"\\nas\media")),
        drive("Z:", DriveKind::Network, Some(r"\\nas\backup")),
    ]
}

async fn harness() -> (tempfile::TempDir, Harness) {
    let (root, mut harness) = Harness::temp().await;
    // The menus offer the actions as on Windows.
    harness.app.drive_bar.network = true;
    harness
        .send(Message::Drive(DriveMessage::Listed(Ok(drives()))))
        .await;
    (root, harness)
}

fn network(message: NetworkMsg) -> Message {
    Message::Drive(DriveMessage::Network(message))
}

/// The engine's answer to the open dialog.
fn finished(harness: &Harness, result: Result<(), &'static str>) -> Message {
    let dialog = harness.app.drive_bar.dialog.as_ref().expect("a dialog");
    network(NetworkMsg::Finished(dialog.generation, result))
}

fn form(harness: &Harness) -> &NetworkForm {
    &harness
        .app
        .drive_bar
        .dialog
        .as_ref()
        .expect("a dialog")
        .form
}

#[test]
fn the_map_form_offers_the_free_letters_and_proposes_the_last() {
    let letters = free_letters(&drives());
    assert_eq!(letters.len(), 23);
    assert!(!letters.contains(&'C') && !letters.contains(&'Z'));
    let form = MapForm::new(&drives());
    assert_eq!(form.letter, Some('X'));
    assert!(
        form.reconnect,
        "kept across sign-ins, as Windows' dialog does"
    );
    let full: Vec<Drive> = ('A'..='Z')
        .map(|letter| drive(&format!("{letter}:"), DriveKind::Fixed, None))
        .collect();
    assert_eq!(MapForm::new(&full).letter, None);
}

#[test]
fn the_unmap_form_lists_the_network_drives_with_the_chosen_one() {
    let form = UnmapForm::new(&drives(), Some("Z:"));
    assert_eq!(
        form.drives,
        [
            ("Y:".to_owned(), r"\\nas\media".to_owned()),
            ("Z:".to_owned(), r"\\nas\backup".to_owned()),
        ]
    );
    assert_eq!(form.chosen(), Some("Z:"));
    assert_eq!(UnmapForm::new(&drives(), Some("C:")).chosen(), Some("Y:"));
    assert_eq!(UnmapForm::new(&drives()[..1], None).chosen(), None);
}

/// Acceptance (window half): open files - the question, then the forced disconnect.
#[tokio::test]
async fn open_files_turn_the_dialog_into_the_question_to_disconnect_anyway() {
    let (_root, mut harness) = harness().await;
    harness
        .send(Message::Drive(DriveMessage::Run(
            PanelId::Left,
            DriveAction::UnmapNetwork,
            Some("Z:".into()),
        )))
        .await;
    let effects = harness.app.apply(network(NetworkMsg::Confirm));
    assert!(matches!(effects.as_slice(), [Effect::Task(_)]));
    let NetworkForm::Unmap(unmap) = form(&harness) else {
        panic!("the disconnect dialog");
    };
    assert_eq!(unmap.phase, Phase::Running);
    assert!(
        harness.app.apply(network(NetworkMsg::Confirm)).is_empty(),
        "once while on its way"
    );

    harness
        .send(finished(&harness, Err("network_drive_in_use")))
        .await;
    let NetworkForm::Unmap(unmap) = form(&harness) else {
        panic!("the disconnect dialog");
    };
    assert!(unmap.in_use && unmap.phase == Phase::Editing, "{unmap:?}");

    // Another drive chosen: the question was about the last one.
    harness.press("ArrowUp").await;
    let NetworkForm::Unmap(unmap) = form(&harness) else {
        panic!("the disconnect dialog");
    };
    assert_eq!((unmap.chosen(), unmap.in_use), (Some("Y:"), false));

    harness.send(finished(&harness, Ok(()))).await;
    assert!(harness.app.drive_bar.dialog.is_none());
}

#[tokio::test]
async fn a_refusal_shows_in_the_map_dialog_and_a_closed_credential_dialog_does_not() {
    let (_root, mut harness) = harness().await;
    harness
        .send(Message::Drive(DriveMessage::Run(
            PanelId::Right,
            DriveAction::MapNetwork,
            None,
        )))
        .await;
    harness
        .send(network(NetworkMsg::Remote(r"\\nas\photos".into())))
        .await;
    harness.send(network(NetworkMsg::Letter('P'))).await;
    harness.send(network(NetworkMsg::Reconnect(false))).await;
    let NetworkForm::Map(map) = form(&harness) else {
        panic!("the map dialog");
    };
    assert_eq!(
        (map.remote.as_str(), map.letter, map.reconnect),
        (r"\\nas\photos", Some('P'), false)
    );

    harness
        .send(finished(&harness, Err("network_path_not_found")))
        .await;
    let NetworkForm::Map(map) = form(&harness) else {
        panic!("the map dialog");
    };
    assert!(matches!(&map.phase, Phase::Failed(text) if !text.contains("network_path")));

    harness.send(finished(&harness, Err("cancelled"))).await;
    let NetworkForm::Map(map) = form(&harness) else {
        panic!("the map dialog");
    };
    assert_eq!(map.phase, Phase::Editing);

    harness.press("Escape").await;
    assert!(harness.app.drive_bar.dialog.is_none());
}

/// A right click on a network drive opens its context menu; Enter on its second entry opens the
/// disconnect dialog on that drive.
#[tokio::test]
async fn a_network_drive_s_context_menu_disconnects_it() {
    let (_root, mut harness) = harness().await;
    harness
        .send(Message::Drive(DriveMessage::Context(
            PanelId::Left,
            "Y:".into(),
        )))
        .await;
    let context = harness
        .app
        .drive_bar
        .context
        .as_ref()
        .expect("a context menu");
    assert_eq!(
        context.actions,
        [DriveAction::MapNetwork, DriveAction::UnmapNetwork]
    );
    harness.press("ArrowDown").await;
    harness.press("Enter").await;
    assert!(harness.app.drive_bar.context.is_none());
    let NetworkForm::Unmap(unmap) = form(&harness) else {
        panic!("the disconnect dialog");
    };
    assert_eq!(unmap.chosen(), Some("Y:"));
}

/// RC-080-16: in the drive menu (Alt+F1) Shift+F10 opens the context menu of the drive under the
/// cursor in its place; on an action under the drives it does nothing, and the context-menu key
/// does the same as Shift+F10.
#[tokio::test]
async fn shift_f10_in_the_drive_menu_opens_the_drive_s_context_menu() {
    let (_root, mut harness) = harness().await;
    harness
        .send(Message::Drive(DriveMessage::OpenMenu(PanelId::Left)))
        .await;
    harness.press("End").await;
    harness.press("Shift+F10").await;
    assert!(
        harness.app.drive_bar.context.is_none(),
        "an action has none"
    );
    assert!(harness.app.drive_bar.menu.is_some());

    harness.press("Home").await;
    harness.press("ArrowDown").await;
    harness.press("Shift+F10").await;
    assert!(harness.app.drive_bar.menu.is_none());
    let context = harness
        .app
        .drive_bar
        .context
        .as_ref()
        .expect("a context menu");
    assert_eq!(
        (context.panel, context.drive.as_str()),
        (PanelId::Left, "Y:")
    );
    harness.press("Escape").await;
    assert!(harness.app.drive_bar.context.is_none());

    harness
        .send(Message::Drive(DriveMessage::OpenMenu(PanelId::Left)))
        .await;
    harness.press("ContextMenu").await;
    assert!(harness.app.drive_bar.context.is_some());
}

/// The audit's D8: a dialog closed while its command ran, and the answer closed or failed the
/// dialog opened next. It is the old dialog's and leaves the new one alone.
#[tokio::test]
async fn the_answer_of_a_closed_dialog_leaves_the_next_one_alone() {
    let (_root, mut harness) = harness().await;
    let open = Message::Drive(DriveMessage::Run(
        PanelId::Left,
        DriveAction::MapNetwork,
        None,
    ));
    harness.send(open.clone()).await;
    let old = finished(&harness, Err("network_path_not_found"));
    harness.press("Escape").await;
    harness.send(open).await;
    harness.send(old).await;
    let NetworkForm::Map(map) = form(&harness) else {
        panic!("the map dialog");
    };
    assert_eq!(map.phase, Phase::Editing, "{map:?}");
}
