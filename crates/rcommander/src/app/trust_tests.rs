use rc_engine::Chord;

use super::{MAX_QUESTIONS, Pressed, Question, QuestionQueue};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Choice {
    Trust,
    Cancel,
}

#[derive(Debug)]
struct Asked(u64);

impl Question for Asked {
    type Step = ();
    type Choice = Choice;

    fn id(&self) -> u64 {
        self.0
    }

    fn buttons(&self, (): ()) -> &'static [Choice] {
        &[Choice::Trust, Choice::Cancel]
    }
}

fn key(text: &str) -> Chord {
    text.parse().expect("a chord")
}

#[test]
fn the_keyboard_starts_on_the_safe_button_and_enter_takes_only_it() {
    let mut queue = QuestionQueue::default();
    queue.ask(Asked(1));
    assert_eq!(queue.focused(), Some(Choice::Cancel));
    assert_eq!(
        queue.key(&key("Enter")),
        Some(Pressed::Chosen(Choice::Cancel))
    );
    assert_eq!(queue.key(&key("Tab")), None);
    assert_eq!(queue.focused(), Some(Choice::Trust));
    assert_eq!(queue.key(&key("Ctrl+Enter")), None);
    assert_eq!(queue.key(&key("Escape")), Some(Pressed::Escape));
}

#[test]
fn the_next_question_starts_on_the_safe_button_again() {
    let mut queue = QuestionQueue::default();
    queue.ask(Asked(1));
    queue.ask(Asked(1));
    queue.ask(Asked(2));
    assert_eq!(queue.waiting(), 1, "a question is filed once");
    queue.key(&key("ArrowLeft"));
    assert_eq!(queue.focused(), Some(Choice::Trust));
    queue.close(1);
    assert_eq!(queue.shown_id(), Some(2));
    assert_eq!(queue.focused(), Some(Choice::Cancel));
    queue.resync(vec![Asked(3)]);
    assert_eq!((queue.shown_id(), queue.waiting()), (Some(3), 0));
    for id in 10..10 + MAX_QUESTIONS as u64 {
        queue.ask(Asked(id));
    }
    assert_eq!(queue.waiting() + 1, MAX_QUESTIONS, "the oldest goes");
    assert_eq!(queue.shown_id(), Some(10));
}

#[test]
fn an_answer_on_its_way_holds_the_dialog_and_its_failure_stays_in_it() {
    let mut queue = QuestionQueue::default();
    queue.ask(Asked(1));
    assert!(queue.answerable(Choice::Trust).is_some());
    queue.answering(1);
    assert!(queue.busy());
    assert!(queue.answerable(Choice::Cancel).is_none());
    assert_eq!(queue.key(&key("Escape")), None, "busy");

    assert_eq!(queue.finished(1, Err("connect_failed"), "gone"), None);
    assert!(!queue.busy());
    assert!(queue.note().is_some(), "the reason stays in the dialog");
    assert_eq!(
        queue
            .finished(1, Ok("trusted".to_owned()), "gone")
            .as_deref(),
        Some("trusted")
    );
    assert!(queue.shown().is_none());

    queue.ask(Asked(2));
    assert!(queue.finished(2, Err("gone"), "gone").is_some());
    assert!(queue.shown().is_none(), "a question the engine lost goes");
    assert!(
        queue.finished(3, Err("connect_failed"), "gone").is_some(),
        "an answer to a question not shown goes to the status bar"
    );
}
