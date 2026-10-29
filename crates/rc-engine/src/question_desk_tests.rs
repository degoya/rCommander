//! The desk on its own: ids, the duplicate rule, the full queue, the grants and their expiry.

use std::time::{Duration, Instant};

use super::{ALLOWANCE_VALID_FOR, Allowances, MAX_QUESTIONS, Question, QuestionDesk};

/// A question as small as the desk needs; `note` stands for what does not make two different.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Ask {
    id: u64,
    host: String,
    port: u16,
    fingerprint: String,
    kind: u8,
    note: &'static str,
}

impl Question for Ask {
    fn id(&self) -> u64 {
        self.id
    }

    fn set_id(&mut self, id: u64) {
        self.id = id;
    }

    fn same_as(&self, other: &Self) -> bool {
        self.host == other.host
            && self.port == other.port
            && self.fingerprint == other.fingerprint
            && self.kind == other.kind
    }

    fn grant(&self) -> (&str, u16, &str) {
        (&self.host, self.port, &self.fingerprint)
    }
}

fn ask(host: &str, fingerprint: &str) -> Ask {
    Ask {
        id: 0,
        host: host.to_owned(),
        port: 22,
        fingerprint: fingerprint.to_owned(),
        kind: 0,
        note: "",
    }
}

fn ids(desk: &QuestionDesk<Ask>) -> Vec<u64> {
    desk.open().iter().map(|question| question.id).collect()
}

#[test]
fn ids_count_up_from_one_and_the_same_question_is_not_stacked() {
    let desk = QuestionDesk::default();
    let first = desk.ask(ask("a.example", "SHA256:a"));
    assert_eq!(first.id, 1);
    assert_eq!(desk.ask(ask("b.example", "SHA256:b")).id, 2);

    // The same again, with another note: the open one, unchanged, no new id.
    let again = desk.ask(Ask {
        note: "later",
        ..ask("a.example", "SHA256:a")
    });
    assert_eq!(again, first);
    assert_eq!(again.note, "", "the first one's note stays");
    assert_eq!(ids(&desk), [1, 2]);

    // Another port, fingerprint or kind is another question.
    for other in [
        Ask {
            port: 2222,
            ..ask("a.example", "SHA256:a")
        },
        ask("a.example", "SHA256:other"),
        Ask {
            kind: 1,
            ..ask("a.example", "SHA256:a")
        },
    ] {
        desk.ask(other);
    }
    assert_eq!(ids(&desk), [1, 2, 3, 4, 5]);
}

#[test]
fn a_full_desk_drops_the_oldest_and_never_reuses_an_id() {
    let desk = QuestionDesk::default();
    for n in 0..MAX_QUESTIONS {
        desk.ask(ask(&format!("host{n}.example"), "SHA256:x"));
    }
    assert_eq!(desk.open().len(), MAX_QUESTIONS);
    assert!(desk.get(1).is_some());

    let newest = desk.ask(ask("one-more.example", "SHA256:x"));
    assert_eq!(newest.id, MAX_QUESTIONS as u64 + 1);
    assert_eq!(desk.open().len(), MAX_QUESTIONS, "still full, not more");
    assert!(desk.get(1).is_none(), "the oldest went");
    assert_eq!(desk.open().first().map(|question| question.id), Some(2));
    assert_eq!(desk.open().last(), Some(&newest));

    // The dropped one asked again is new: the next id, at the end.
    let back = desk.ask(ask("host0.example", "SHA256:x"));
    assert_eq!(back.id, MAX_QUESTIONS as u64 + 2);
    assert!(desk.get(2).is_none());
}

#[test]
fn close_removes_one_question_and_a_closed_one_is_asked_anew() {
    let desk = QuestionDesk::default();
    let first = desk.ask(ask("a.example", "SHA256:a"));
    desk.ask(ask("b.example", "SHA256:b"));
    desk.close(99);
    assert_eq!(ids(&desk), [1, 2], "an unknown id closes nothing");

    desk.close(first.id);
    assert_eq!(ids(&desk), [2]);
    assert!(desk.get(first.id).is_none());
    assert_eq!(desk.ask(ask("a.example", "SHA256:a")).id, 3);
}

#[test]
fn a_grant_is_for_its_server_and_fingerprint_and_is_spent_once() {
    let desk = QuestionDesk::default();
    let question = desk.ask(ask("a.example", "SHA256:a"));
    desk.allow_once(&question);
    // Answering does not close the question; the command does.
    assert_eq!(ids(&desk), [1]);

    assert!(!desk.take_allowance("b.example", 22, "SHA256:a"));
    assert!(!desk.take_allowance("a.example", 2222, "SHA256:a"));
    assert!(!desk.take_allowance("a.example", 22, "SHA256:other"));
    assert!(desk.take_allowance("a.example", 22, "SHA256:a"));
    assert!(!desk.take_allowance("a.example", 22, "SHA256:a"), "spent");
}

#[test]
fn a_grant_for_the_server_names_the_fingerprint_it_allows() {
    let desk = QuestionDesk::default();
    desk.allow_once(&ask("a.example", "SHA256:a"));
    assert_eq!(desk.take_allowance_for("a.example", 2222), None);
    assert_eq!(
        desk.take_allowance_for("a.example", 22).as_deref(),
        Some("SHA256:a")
    );
    assert_eq!(desk.take_allowance_for("a.example", 22), None, "spent");
}

#[test]
fn two_grants_are_two_connections() {
    let desk = QuestionDesk::default();
    let question = ask("a.example", "SHA256:a");
    desk.allow_once(&question);
    desk.allow_once(&question);
    assert!(desk.take_allowance("a.example", 22, "SHA256:a"));
    assert!(desk.take_allowance("a.example", 22, "SHA256:a"));
    assert!(!desk.take_allowance("a.example", 22, "SHA256:a"));
}

#[test]
fn a_grant_expires_exactly_at_its_lifetime() {
    let granted = Instant::now();
    let just_before = granted + ALLOWANCE_VALID_FOR - Duration::from_nanos(1);
    let at_the_edge = granted + ALLOWANCE_VALID_FOR;

    let mut grants = Allowances::default();
    grants.grant("a.example", 22, "SHA256:a", granted);
    assert!(grants.take_exact("a.example", 22, "SHA256:a", just_before));

    grants.grant("a.example", 22, "SHA256:a", granted);
    assert!(!grants.take_exact("a.example", 22, "SHA256:a", at_the_edge));
    assert_eq!(grants.count(), 0, "the expired grant is gone");

    grants.grant("a.example", 22, "SHA256:a", granted);
    assert_eq!(grants.take_for("a.example", 22, at_the_edge), None);
    grants.grant("a.example", 22, "SHA256:a", granted);
    assert_eq!(
        grants.take_for("a.example", 22, just_before).as_deref(),
        Some("SHA256:a")
    );
}

#[test]
fn any_take_sweeps_expired_grants_of_other_servers() {
    let granted = Instant::now();
    let mut grants = Allowances::default();
    grants.grant("old.example", 22, "SHA256:old", granted);
    grants.grant(
        "new.example",
        22,
        "SHA256:new",
        granted + ALLOWANCE_VALID_FOR,
    );

    let now = granted + ALLOWANCE_VALID_FOR;
    assert!(!grants.take_exact("none.example", 22, "SHA256:x", now));
    assert_eq!(grants.count(), 1, "only the expired one went");
    assert!(grants.take_exact("new.example", 22, "SHA256:new", now));
}

#[test]
fn a_clock_behind_the_grant_does_not_expire_it() {
    let now = Instant::now();
    let mut grants = Allowances::default();
    grants.grant("a.example", 22, "SHA256:a", now + Duration::from_secs(1));
    assert!(grants.take_exact("a.example", 22, "SHA256:a", now));
}
