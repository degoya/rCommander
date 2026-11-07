use super::{Answer, answers};
use crate::{login_script::Pattern, totp::DEFAULT_PROMPT};

fn pattern() -> Pattern {
    Pattern::new(DEFAULT_PROMPT).expect("the default compiles")
}

#[test]
fn password_and_code_in_one_round_are_both_answered() {
    let round = [("Password: ", false), ("Verification code: ", false)];
    assert_eq!(
        answers(round, &pattern(), true),
        Some(vec![Answer::Password, Answer::Code])
    );
}

#[test]
fn an_echoed_code_question_is_answered_too() {
    assert_eq!(
        answers([("Verification code: ", true)], &pattern(), false),
        Some(vec![Answer::Code])
    );
}

#[test]
fn a_question_the_pattern_misses_goes_to_the_person() {
    let own = Pattern::new("(?i)passcode").expect("compiles");
    // Under its own pattern the connection does not take "Verification code" for a code: the
    // question is a person's, so the round is not answered.
    let round = [("Password: ", false), ("Verification code: ", false)];
    assert_eq!(answers(round, &own, true), None);
    assert_eq!(
        answers([("Passcode: ", false)], &own, false),
        Some(vec![Answer::Code])
    );
}

#[test]
fn a_second_hidden_question_or_an_echoed_one_is_not_guessed() {
    assert_eq!(
        answers([("Password: ", false), ("PIN: ", false)], &pattern(), true),
        None
    );
    assert_eq!(answers([("Username: ", true)], &pattern(), true), None);
    assert_eq!(answers([("Password: ", false)], &pattern(), false), None);
}

#[test]
fn an_empty_round_is_answered_empty() {
    assert_eq!(
        answers(Vec::<(&str, bool)>::new(), &pattern(), false),
        Some(Vec::new())
    );
}
