use iced::{Point, Size, window::Position};
use rc_db::Db;

use super::{GEOMETRY_KEY, Geometry, WindowChange, WindowState, load, save};

fn geometry(width: f32, height: f32, position: Option<(f32, f32)>) -> Geometry {
    Geometry {
        width,
        height,
        position,
        maximized: false,
    }
}

#[test]
fn a_first_start_is_centred_at_the_default_size() {
    let settings = WindowState::new(None).geometry().window_settings();
    assert_eq!(settings.size, Size::new(1280.0, 800.0));
    assert!(matches!(settings.position, Position::Centered));
    assert!(!settings.maximized);
    assert!(settings.min_size.is_some());
}

#[test]
fn a_stored_geometry_opens_where_it_was() {
    let stored = Geometry {
        maximized: true,
        ..geometry(1000.0, 700.0, Some((120.0, 80.0)))
    };
    let settings = WindowState::new(Some(stored)).geometry().window_settings();
    assert_eq!(settings.size, Size::new(1000.0, 700.0));
    assert!(matches!(settings.position, Position::Specific(at) if at == Point::new(120.0, 80.0)));
    assert!(settings.maximized);
}

#[test]
fn a_stored_geometry_is_made_safe() {
    let tiny = geometry(10.0, 10.0, Some((-32_000.0, -32_000.0)))
        .sanitized()
        .expect("sizes are numbers");
    assert_eq!((tiny.width, tiny.height), (640.0, 400.0));
    assert_eq!(tiny.position, None, "a parked window's place is no place");
    assert_eq!(geometry(f32::NAN, 600.0, None).sanitized(), None);
    // A size no screen has - a hand-edited or broken value - is cut to one that can be.
    let huge = geometry(1.0e30, 9.0e9, None)
        .sanitized()
        .expect("sizes are numbers");
    assert_eq!((huge.width, huge.height), (16_000.0, 16_000.0));
    assert_eq!(
        WindowState::new(Some(geometry(f32::INFINITY, 1.0, None))).geometry(),
        Geometry::default()
    );
}

#[test]
fn moves_and_sizes_count_only_while_not_maximized_or_minimized() {
    let mut state = WindowState::new(None);
    state.observe(WindowChange::Moved(Point::new(40.0, 30.0)), false);
    state.observe(WindowChange::Resized(Size::new(900.0, 600.0)), false);
    state.observe(WindowChange::Resized(Size::new(2560.0, 1400.0)), true);
    state.observe(WindowChange::Moved(Point::new(-8.0, -8.0)), true);
    assert_eq!(
        state.geometry(),
        Geometry {
            maximized: true,
            ..geometry(900.0, 600.0, Some((40.0, 30.0)))
        }
    );
    state.observe(WindowChange::Moved(Point::new(-32_000.0, -32_000.0)), false);
    state.observe(WindowChange::Resized(Size::new(0.0, 0.0)), false);
    assert_eq!(
        state.geometry(),
        geometry(900.0, 600.0, Some((40.0, 30.0))),
        "minimized: not maximized, but nothing measured"
    );
}

#[tokio::test]
async fn the_geometry_survives_a_restart() {
    let db = Db::open_in_memory().await.expect("db");
    assert_eq!(load(&db).await, None);
    let mut window = WindowState::new(load(&db).await);
    window.observe(WindowChange::Resized(Size::new(1100.0, 720.0)), false);
    window.observe(WindowChange::Moved(Point::new(200.0, 150.0)), false);
    save(&db, window.geometry()).await.expect("stored");

    let restarted = WindowState::new(load(&db).await);
    assert_eq!(
        restarted.geometry(),
        geometry(1100.0, 720.0, Some((200.0, 150.0)))
    );
}

#[tokio::test]
async fn a_damaged_setting_opens_the_default_window() {
    let db = Db::open_in_memory().await.expect("db");
    db.set(GEOMETRY_KEY, "not a geometry")
        .await
        .expect("stored");
    assert_eq!(load(&db).await, None);
}
