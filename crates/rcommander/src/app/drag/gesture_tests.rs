use super::*;

fn at(x: f32, y: f32) -> Point {
    Point::new(x, y)
}

#[test]
fn a_press_is_tracked_until_its_release() {
    let mut gesture = Gesture::press(PanelId::Left, 3);
    assert!(gesture.tracking());
    assert_eq!(gesture.release(), None, "a click drags nothing");
    assert_eq!(gesture, Gesture::Idle);
    assert!(!gesture.tracking());
}

#[test]
fn a_move_past_the_threshold_starts_the_drag_once() {
    let mut gesture = Gesture::press(PanelId::Right, 2);
    assert_eq!(gesture.moved(at(100.0, 100.0)), None, "the first position");
    assert_eq!(
        gesture.moved(at(100.0 + THRESHOLD, 100.0)),
        None,
        "not past it"
    );
    assert_eq!(
        gesture.moved(at(100.0, 100.0 + THRESHOLD + 1.0)),
        Some((PanelId::Right, 2))
    );
    assert_eq!(
        gesture,
        Gesture::Dragging {
            panel: PanelId::Right
        }
    );
    assert_eq!(gesture.moved(at(300.0, 300.0)), None, "started already");
    assert_eq!(gesture.release(), Some(PanelId::Right));
    assert_eq!(gesture, Gesture::Idle);
}

#[test]
fn moves_without_a_press_do_nothing() {
    let mut gesture = Gesture::Idle;
    assert_eq!(gesture.moved(at(0.0, 0.0)), None);
    assert_eq!(gesture.moved(at(500.0, 0.0)), None);
    assert_eq!(gesture, Gesture::Idle);
}

#[test]
fn a_new_press_ends_a_drag_that_missed_its_release() {
    let mut gesture = Gesture::press(PanelId::Left, 1);
    gesture.moved(at(0.0, 0.0));
    gesture.moved(at(50.0, 0.0));
    gesture.pressed();
    assert_eq!(gesture, Gesture::Idle);
    // A press on a row arms first; the window's own press event then keeps it armed.
    let mut armed = Gesture::press(PanelId::Left, 1);
    armed.pressed();
    assert!(armed.tracking());
}
