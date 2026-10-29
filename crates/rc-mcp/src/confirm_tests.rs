use std::time::{Duration, Instant};

use rc_engine::McpArgument;

use super::{Confirmations, MAX_OPEN, MAX_PER_SESSION, VALID_FOR, question, subject_of};

/// The questions without a subject, which only these tests ask: the gate always names one
/// (`ask_for`, `redeem_for`), so these stay out of the shipped code (RC-080-54).
impl Confirmations {
    fn ask(&self, session: &str, tool: &str) -> String {
        self.ask_at(session, tool, Instant::now())
    }

    fn ask_at(&self, session: &str, tool: &str, now: Instant) -> String {
        self.ask_for_at(session, tool, None, None, now)
    }

    fn redeem(&self, session: &str, tool: &str, code: &str) -> bool {
        self.redeem_at(session, tool, code, Instant::now())
    }

    fn redeem_at(&self, session: &str, tool: &str, code: &str, now: Instant) -> bool {
        self.redeem_for_at(session, tool, code, |_| None, now)
            .is_some()
    }

    /// Whether `code` answers the question about `tool` acting on `subject`.
    fn redeem_subject(&self, session: &str, tool: &str, subject: &str, code: &str) -> bool {
        self.redeem_for(session, tool, code, |_| Some(subject.to_owned()))
            .is_some()
    }
}

#[test]
fn a_code_answers_its_own_tool_and_session_once() {
    let confirmations = Confirmations::default();
    let code = confirmations.ask("s1", "delete");
    assert_eq!(code.len(), 32, "{code}");
    assert!(confirmations.redeem("s1", "delete", &code));
    assert!(
        !confirmations.redeem("s1", "delete", &code),
        "a code is spent by its first use"
    );
}

#[test]
fn a_code_does_not_answer_another_tool_and_is_spent_trying() {
    let confirmations = Confirmations::default();
    let code = confirmations.ask("s1", "delete");
    assert!(!confirmations.redeem("s1", "ssh_exec", &code));
    assert!(!confirmations.redeem("s1", "delete", &code));
    assert!(!confirmations.redeem("s1", "delete", "not-a-code"));
}

#[test]
fn a_code_does_not_answer_in_another_session() {
    let confirmations = Confirmations::default();
    let code = confirmations.ask("s1", "delete");
    assert!(!confirmations.redeem("s2", "delete", &code));
    assert!(!confirmations.redeem("s1", "delete", &code), "spent trying");
}

#[test]
fn a_code_expires_after_ten_minutes() {
    let confirmations = Confirmations::default();
    let asked = Instant::now();
    let fresh = confirmations.ask_at("s1", "delete", asked);
    let stale = confirmations.ask_at("s1", "delete", asked);
    assert!(confirmations.redeem_at(
        "s1",
        "delete",
        &fresh,
        asked + VALID_FOR - Duration::from_secs(1)
    ));
    assert!(!confirmations.redeem_at("s1", "delete", &stale, asked + VALID_FOR));
    assert_eq!(VALID_FOR, Duration::from_secs(600));
}

#[test]
fn two_questions_get_two_codes_and_a_clone_shares_them() {
    let confirmations = Confirmations::default();
    let first = confirmations.ask("s1", "delete");
    let second = confirmations.clone().ask("s1", "delete");
    assert_ne!(first, second);
    assert!(confirmations.redeem("s1", "delete", &second));
    assert!(confirmations.redeem("s1", "delete", &first));
}

#[test]
fn the_question_names_the_tool_and_the_code() {
    let named = [McpArgument {
        name: "sources".to_owned(),
        value: "[\"a.txt\"]".to_owned(),
    }];
    let asked = question("delete", "abc", "destructive", &named);
    assert_eq!(asked["confirmation_required"], true);
    assert_eq!(asked["confirmation"], "abc");
    assert_eq!(
        asked["arguments"],
        serde_json::json!([{ "name": "sources", "value": "[\"a.txt\"]" }]),
        "what it acts on (RC-080-55)"
    );
    let text = asked["question"].as_str().expect("text");
    assert!(text.contains("confirmation=\"abc\""), "{text}");
}

#[test]
fn a_code_for_one_subject_does_not_answer_for_another() {
    let confirmations = Confirmations::default();
    let code = confirmations.ask_for("s1", "terminal_send_input", Some("terminal=1"), None);
    assert!(!confirmations.redeem_subject("s1", "terminal_send_input", "terminal=2", &code));
    assert!(
        !confirmations.redeem_subject("s1", "terminal_send_input", "terminal=1", &code),
        "spent trying"
    );
    let code = confirmations.ask_for("s1", "terminal_send_input", Some("terminal=1"), None);
    assert!(
        !confirmations.redeem("s1", "terminal_send_input", &code),
        "no subject"
    );
    let code = confirmations.ask_for("s1", "terminal_send_input", Some("terminal=1"), None);
    assert!(confirmations.redeem_subject("s1", "terminal_send_input", "terminal=1", &code));
}

/// RC-080-03: the code is bound to every argument but itself - the text typed, the files
/// deleted - with keys sorted at every depth, so only their order may differ.
#[test]
fn the_subject_is_every_argument_but_the_code() {
    let arguments = serde_json::json!({ "terminal": 7, "text": "ls", "confirmation": "x" });
    assert_eq!(
        subject_of(arguments.as_object()).as_deref(),
        Some(r#"{"terminal":7,"text":"ls"}"#)
    );
    let typed = serde_json::json!({ "terminal": 7, "text": "rm -rf ~" });
    assert_ne!(
        subject_of(typed.as_object()),
        subject_of(arguments.as_object())
    );
    let open = serde_json::json!({ "shell": "cmd", "cwd": "C:\\", "rows": 30 });
    assert_eq!(
        subject_of(open.as_object()).as_deref(),
        Some(r#"{"cwd":"C:\\","rows":30,"shell":"cmd"}"#)
    );
    let delete = serde_json::json!({ "sources": ["a.txt"], "panel": "left" });
    let reordered = serde_json::json!({ "panel": "left", "sources": ["a.txt"] });
    assert_eq!(
        subject_of(delete.as_object()),
        subject_of(reordered.as_object())
    );
    let more = serde_json::json!({ "sources": ["a.txt", "b.txt"], "panel": "left" });
    assert_ne!(subject_of(delete.as_object()), subject_of(more.as_object()));
    let nested = serde_json::json!({ "actions": [{ "path": "x", "action": "delete_right" }] });
    assert_eq!(
        subject_of(nested.as_object()).as_deref(),
        Some(r#"{"actions":[{"action":"delete_right","path":"x"}]}"#)
    );
    let only_code = serde_json::json!({ "confirmation": "x" });
    assert_eq!(subject_of(only_code.as_object()), None);
    assert_eq!(subject_of(Some(&serde_json::Map::new())), None);
    assert_eq!(subject_of(None), None);
}

/// RC-080-42: a session asking in a loop spends its own oldest questions, and all sessions
/// together hold at most `MAX_OPEN`.
#[test]
fn open_questions_are_bounded_per_session_and_in_all() {
    let confirmations = Confirmations::default();
    let start = Instant::now();
    let at = |step: usize| start + Duration::from_millis(u64::try_from(step).expect("small"));
    let codes: Vec<String> = (0..=MAX_PER_SESSION)
        .map(|step| confirmations.ask_at("s1", "delete", at(step)))
        .collect();
    let other = confirmations.ask_at("s2", "delete", at(MAX_PER_SESSION + 1));
    let now = at(MAX_PER_SESSION + 2);
    assert!(
        !confirmations.redeem_at("s1", "delete", &codes[0], now),
        "the oldest was spent"
    );
    for code in &codes[1..] {
        assert!(confirmations.redeem_at("s1", "delete", code, now));
    }
    assert!(
        confirmations.redeem_at("s2", "delete", &other, now),
        "another session keeps its own"
    );

    let all = Confirmations::default();
    let first = all.ask_at("s0", "delete", at(0));
    for step in 1..=MAX_OPEN {
        all.ask_at(&format!("s{step}"), "delete", at(step));
    }
    assert!(!all.redeem_at("s0", "delete", &first, at(MAX_OPEN + 1)));
    assert_eq!(all.0.lock().expect("lock").len(), MAX_OPEN);
}
