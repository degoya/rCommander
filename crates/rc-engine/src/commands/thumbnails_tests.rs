//! The thumbnails' zoom (RC-100-89): its steps and the setting it writes.

use rc_db::Db;

use super::{ZOOM_STEPS, zoomed};
use crate::{Command, Outcome, ZoomStep, settings::THUMBNAIL_SIZE_KEY, test_support};

#[test]
fn the_zoom_steps_through_the_sizes_and_stops_at_both_ends() {
    assert_eq!(zoomed(96, ZoomStep::In), 128);
    assert_eq!(zoomed(96, ZoomStep::Out), 64);
    assert_eq!(zoomed(512, ZoomStep::In), 512, "the largest stays");
    assert_eq!(zoomed(64, ZoomStep::Out), 64, "the smallest step stays");
    assert_eq!(
        zoomed(32, ZoomStep::Out),
        32,
        "a size below the steps stays"
    );
    assert_eq!(zoomed(32, ZoomStep::In), 64);
    assert_eq!(
        zoomed(100, ZoomStep::Out),
        96,
        "a size between steps takes the next"
    );
    assert!(ZOOM_STEPS.is_sorted());
}

#[tokio::test]
async fn ctrl_and_the_wheel_write_the_tile_size() {
    let (_root, engine) = test_support::engine(&[]).await;
    assert!(engine.attach_db(Db::open_in_memory().await.expect("db")));
    let mut seen = Vec::new();
    for step in [ZoomStep::In, ZoomStep::In, ZoomStep::Out] {
        let Ok(Outcome::Settings { settings }) =
            engine.dispatch(Command::ThumbnailZoom { step }).await
        else {
            panic!("the setting");
        };
        assert_eq!(settings[0].key, THUMBNAIL_SIZE_KEY);
        seen.push(settings[0].value.clone());
    }
    assert_eq!(seen, [128, 192, 128]);
}

#[test]
fn the_step_reads_as_in_and_out() {
    let command: Command = serde_json::from_value(serde_json::json!({
        "command": "panel.thumbnail_zoom",
        "params": { "step": "out" }
    }))
    .expect("the command");
    assert_eq!(
        command,
        Command::ThumbnailZoom {
            step: ZoomStep::Out
        }
    );
}
