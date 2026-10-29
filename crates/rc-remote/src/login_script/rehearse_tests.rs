use super::*;
use crate::login_script::{Pattern, TEMPLATES};

#[tokio::test]
async fn the_test_run_shows_where_each_step_matched() {
    let sudo = TEMPLATES[0].script();
    let sample = "user@box:~$ sudo -i\r\n[sudo] password for user: \r\n\x1b[1mroot@box\x1b[0m:~# ";
    let rehearsal = rehearse(&sudo, sample).await.expect("runs");
    assert_eq!(rehearsal.outcome, ScriptOutcome::Completed);
    assert_eq!(
        rehearsal.text,
        "user@box:~$ sudo -i\r\n[sudo] password for user: \r\nroot@box:~# "
    );
    let ends: Vec<Option<usize>> = rehearsal.reached.iter().map(|step| step.end).collect();
    let password_end = rehearsal.text.find("user: ").expect("prompt") + "user:".len();
    assert_eq!(
        ends,
        [None, Some(password_end), None, Some(rehearsal.text.len())]
    );
}

#[tokio::test]
async fn a_sample_that_ends_too_early_stops_at_the_waiting_step() {
    let script = LoginScript {
        steps: vec![
            Step::WaitFor {
                pattern: Pattern::new("login:").expect("pattern"),
                timeout_ms: None,
            },
            Step::Sleep { ms: 60_000 },
            Step::WaitFor {
                pattern: Pattern::new("never").expect("pattern"),
                timeout_ms: None,
            },
        ],
        ..LoginScript::default()
    };
    let rehearsal = rehearse(&script, "login: ").await.expect("runs");
    assert_eq!(rehearsal.outcome, ScriptOutcome::StreamClosed { step: 2 });
    assert_eq!(rehearsal.reached.len(), 3);
    assert_eq!(rehearsal.reached[2].end, None);
}

#[tokio::test]
async fn a_test_run_reads_no_secret_and_refuses_an_invalid_script() {
    let secret = Step::SendSecret {
        source: SecretRef::Stored { id: 1 },
        newline: true,
    };
    let script = LoginScript {
        steps: vec![
            Step::WaitFor {
                pattern: Pattern::new("(?i)password.*:").expect("pattern"),
                timeout_ms: None,
            },
            secret.clone(),
        ],
        ..LoginScript::default()
    };
    let rehearsal = rehearse(&script, "Password: ").await.expect("runs");
    assert_eq!(rehearsal.outcome, ScriptOutcome::Completed);

    // A secret without its prompt right before it is refused (RC-080-12).
    let blind = LoginScript {
        steps: vec![secret],
        ..LoginScript::default()
    };
    assert!(rehearse(&blind, "").await.is_err());

    let invalid = LoginScript {
        step_timeout_ms: 0,
        ..LoginScript::default()
    };
    assert!(rehearse(&invalid, "").await.is_err());
}
