//! A column set's width dragged in the header is kept in the set; the header's right click opens
//! the menu of the views (RC-100-90).

use iced::Point;
use rc_engine::{ColumnSetSpec, ColumnSetsOutcome, Command, Field, ListMode, Outcome, PanelId};

use crate::app::{
    Message,
    harness::Harness,
    resize::{ResizeMsg, Target},
    tabs::popup::PopupAction,
};

fn wrap(message: ResizeMsg) -> Message {
    Message::Resize(message)
}

/// A harness with the set "Photos" (name, size, created) read by the window; its id.
async fn with_set() -> (tempfile::TempDir, Harness, i64) {
    let (root, mut harness) = Harness::temp_with_db().await;
    let command = Command::CreateColumnSet {
        name: "Photos".into(),
        spec: ColumnSetSpec::of(&[Field::Name, Field::Size, Field::Created]),
    };
    let id = match harness.app.engine().dispatch(command).await {
        Ok(Outcome::ColumnSets(outcome)) => match *outcome {
            ColumnSetsOutcome::Saved { id } => id,
            other => panic!("saved, got {other:?}"),
        },
        other => panic!("column sets, got {other:?}"),
    };
    let sets = harness.app.engine().column_sets().await.expect("read");
    harness.send(wrap(ResizeMsg::Sets(sets))).await;
    (root, harness, id)
}

#[tokio::test]
async fn a_dragged_width_is_kept_in_the_set() {
    let (_root, mut harness, id) = with_set().await;
    let target = Target::SetColumn { set: id, index: 2 };
    harness.send(wrap(ResizeMsg::Grab(target))).await;
    harness
        .send(wrap(ResizeMsg::Moved(Point::new(300.0, 10.0))))
        .await;
    harness
        .send(wrap(ResizeMsg::Moved(Point::new(260.0, 10.0))))
        .await;
    let scale = crate::theme::fonts::list().column_scale();
    let wider = (f32::from(Field::Created.default_width()) + 40.0 / scale).round() as u16;
    let shown = harness.app.resize.set(id).expect("known");
    assert_eq!(
        shown.spec.columns[2].width, wider,
        "the left edge moved left by 40"
    );
    harness.send(wrap(ResizeMsg::Released)).await;
    let stored = harness.app.engine().column_sets().await.expect("read");
    assert_eq!(stored[0].spec.columns[2].width, wider, "kept in the set");

    harness.send(wrap(ResizeMsg::Reset(target))).await;
    let stored = harness.app.engine().column_sets().await.expect("read");
    assert_eq!(
        stored[0].spec.columns[2].width,
        Field::Created.default_width()
    );
}

#[tokio::test]
async fn the_header_menu_lists_the_views_and_shows_the_one_taken() {
    let (_root, mut harness, id) = with_set().await;
    harness.send(wrap(ResizeMsg::Menu(PanelId::Left))).await;
    let popup = harness.app.tabs.popup.as_ref().expect("open");
    let at = popup
        .rows
        .iter()
        .position(|row| row.action == Some(PopupAction::View(ListMode::Custom { set: id })))
        .expect("the set's row");
    assert_eq!(popup.rows[at].label, "Photos");
    harness
        .send(Message::Tabs(crate::app::tabs::TabsMessage::Pick(at)))
        .await;
    assert_eq!(
        harness.app.engine().panel(PanelId::Left).mode,
        ListMode::Custom { set: id }
    );
}
