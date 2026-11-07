//! Which stored secret answers which question of a `keyboard-interactive` round (RC-060-08).
//!
//! A question the connection's code pattern matches gets the one-time code; one hidden question
//! besides gets the stored password. Anything else - a second hidden question, an echoed one the
//! pattern does not match - is a person's to answer, and the round is not guessed at: the method
//! is refused, as it was before codes were answered at all.

use crate::login_script::Pattern;

/// What one question is answered with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Answer {
    Password,
    /// The one-time code of the connection's seed.
    Code,
}

/// The answers to a round of `questions` (text, echoed), in their order; `None` when one of
/// them is no question rCommander answers. An empty round is answered empty.
pub(super) fn answers<'a>(
    questions: impl IntoIterator<Item = (&'a str, bool)>,
    code_prompt: &Pattern,
    has_password: bool,
) -> Option<Vec<Answer>> {
    let mut password_given = false;
    let mut answers = Vec::new();
    for (question, echo) in questions {
        if code_prompt.match_end(question).is_some() {
            answers.push(Answer::Code);
        } else if !echo && has_password && !password_given {
            password_given = true;
            answers.push(Answer::Password);
        } else {
            return None;
        }
    }
    Some(answers)
}

#[cfg(test)]
#[path = "auth_prompts_tests.rs"]
mod tests;
