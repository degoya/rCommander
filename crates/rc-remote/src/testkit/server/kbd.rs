//! The test server's `keyboard-interactive` method: the password, and with a [`SecondFactor`]
//! a one-time code after it in a second round, as `sshd` with PAM and Google Authenticator's
//! module asks (RC-060-08).

use std::borrow::Cow;

use russh::server::{Auth, Response};
use secrecy::ExposeSecret as _;

use super::ServerOptions;
use crate::totp::{TotpSeed, unix_now};

/// The password prompt of the first round.
const PASSWORD_PROMPT: &str = "Password: ";

/// A code the server asks for after the password.
#[derive(Clone)]
pub struct SecondFactor {
    /// The seed the codes are checked against: a Base32 secret or an `otpauth://` URI.
    pub seed: String,
    /// The question of the second round.
    pub prompt: String,
}

impl SecondFactor {
    /// Google Authenticator's question for the seed `seed`.
    #[must_use]
    pub fn verification_code(seed: &str) -> Self {
        Self {
            seed: seed.to_owned(),
            prompt: "Verification code: ".to_owned(),
        }
    }

    /// Whether `answer` is the code of now, or of the period before (a code typed at the turn).
    fn accepts(&self, answer: &[u8]) -> bool {
        let Ok(seed) = TotpSeed::parse(&self.seed) else {
            return false;
        };
        let now = unix_now();
        [now, now.saturating_sub(seed.period())]
            .into_iter()
            .any(|at| seed.code_at(at).expose_secret().as_bytes() == answer)
    }
}

/// Where one connection's `keyboard-interactive` login stands.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Stage {
    #[default]
    Start,
    /// The password was asked for.
    Password,
    /// The password was right; the code was asked for.
    Code,
}

/// The server's turn: a question to ask, or the verdict on the answer to the last one.
pub(super) fn respond(
    options: &ServerOptions,
    stage: &mut Stage,
    response: Option<Response<'_>>,
) -> Auth {
    let Some(mut response) = response else {
        *stage = Stage::Password;
        return ask(PASSWORD_PROMPT.to_owned());
    };
    let answer = response.next();
    match (*stage, answer) {
        (Stage::Password, Some(answer))
            if options.password.as_deref().map(str::as_bytes) == Some(&answer[..]) =>
        {
            match &options.second_factor {
                None => Auth::Accept,
                Some(factor) => {
                    *stage = Stage::Code;
                    ask(factor.prompt.clone())
                }
            }
        }
        (Stage::Code, Some(answer))
            if options
                .second_factor
                .as_ref()
                .is_some_and(|factor| factor.accepts(&answer)) =>
        {
            Auth::Accept
        }
        _ => {
            *stage = Stage::Start;
            Auth::reject()
        }
    }
}

/// One hidden question.
fn ask(prompt: String) -> Auth {
    Auth::Partial {
        name: Cow::Borrowed(""),
        instructions: Cow::Borrowed(""),
        prompts: Cow::Owned(vec![(Cow::Owned(prompt), false)]),
    }
}
