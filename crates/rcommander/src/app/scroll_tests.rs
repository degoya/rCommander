use super::share;

#[test]
fn the_share_runs_from_the_first_line_to_the_last() {
    assert_eq!(share(0, 40), 0.0);
    assert_eq!(share(39, 40), 1.0);
    assert!((share(20, 41) - 0.5).abs() < f32::EPSILON);
    assert_eq!(share(0, 1), 0.0, "one line never scrolls");
    assert_eq!(share(3, 0), 0.0, "nor an empty list");
    assert_eq!(share(99, 40), 1.0, "past the end is the end");
}

/// The Alt+F8 list of the command line keeps its highlighted line in view: when it opens and at
/// every move. The drive menu and the tab popups are tested beside their code.
#[tokio::test]
async fn the_command_line_history_keeps_its_line_in_view() {
    let root = tempfile::tempdir().expect("temp dir");
    let mut harness = crate::app::harness::Harness::headless(root.path()).await;
    harness.app.cmdline.history = (0..40).map(|n| format!("echo {n}")).collect();
    let scrolls = |effects: Vec<crate::app::Effect>| {
        effects
            .iter()
            .filter(|effect| matches!(effect, crate::app::Effect::Task(_)))
            .count()
    };
    let key = |keys: &str| crate::app::Message::Key(keys.parse().expect("chord"));
    assert_eq!(scrolls(harness.app.apply(key("Alt+F8"))), 1, "opened");
    assert_eq!(harness.app.cmdline.list, Some(0));
    for _ in 0..30 {
        assert_eq!(scrolls(harness.app.apply(key("ArrowDown"))), 1);
    }
    assert_eq!(harness.app.cmdline.list, Some(30));
    assert_eq!(scrolls(harness.app.apply(key("ArrowUp"))), 1);
    assert_eq!(scrolls(harness.app.apply(key("Escape"))), 0);
    assert_eq!(harness.app.cmdline.list, None);
}
