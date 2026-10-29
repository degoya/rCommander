//! The desk of trust questions that wait for a person: one for SSH host keys (`hostkeys.rs`,
//! RC-050-07), one for FTPS certificates (`certs.rs`, RC-070-02), the same rules for both
//! (RC-080-22).
//!
//! A refused connection files a question; the same server, key and kind asked again finds the
//! open one instead of stacking another. At most [`MAX_QUESTIONS`] wait, the oldest goes when
//! another comes; ids count up from 1 and are never handed out twice. A "once" answer leaves an
//! [`Allowances`] grant for exactly that server and fingerprint, spent by the next connection and
//! void [`ALLOWANCE_VALID_FOR`] after it was given - at that instant already.
//!
//! The desk only keeps state. Who may answer stays with the commands (`GuiOnly`, never MCP), and
//! the announcement (`EngineEvent`) with its user.

use std::{
    collections::VecDeque,
    sync::{Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant},
};

/// How many questions wait at most; the oldest goes when another comes.
pub(crate) const MAX_QUESTIONS: usize = 32;

/// How long a "once" grant waits for the connection it allows.
pub(crate) const ALLOWANCE_VALID_FOR: Duration = Duration::from_secs(300);

/// A question the desk can keep: its id, what makes two the same, and what a "once" grants.
pub(crate) trait Question: Clone {
    fn id(&self) -> u64;
    fn set_id(&mut self, id: u64);
    /// Whether `other` asks the same thing - server, key or certificate, kind - ids aside.
    fn same_as(&self, other: &Self) -> bool;
    /// The host, port and fingerprint a "once" answer grants.
    fn grant(&self) -> (&str, u16, &str);
}

/// The open questions and the "once" grants not used yet, behind one lock.
pub(crate) struct QuestionDesk<Q> {
    inner: Mutex<Desk<Q>>,
}

struct Desk<Q> {
    next_id: u64,
    questions: VecDeque<Q>,
    allowances: Allowances,
}

impl<Q> Default for QuestionDesk<Q> {
    fn default() -> Self {
        Self {
            inner: Mutex::new(Desk {
                next_id: 0,
                questions: VecDeque::new(),
                allowances: Allowances::default(),
            }),
        }
    }
}

impl<Q: Question> QuestionDesk<Q> {
    fn lock(&self) -> MutexGuard<'_, Desk<Q>> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Files `question` under the next id, or returns the open one that asks the same.
    pub(crate) fn ask(&self, mut question: Q) -> Q {
        let mut desk = self.lock();
        if let Some(open) = desk.questions.iter().find(|open| open.same_as(&question)) {
            return open.clone();
        }
        desk.next_id += 1;
        question.set_id(desk.next_id);
        if desk.questions.len() >= MAX_QUESTIONS {
            desk.questions.pop_front();
        }
        desk.questions.push_back(question.clone());
        question
    }

    pub(crate) fn get(&self, id: u64) -> Option<Q> {
        self.lock()
            .questions
            .iter()
            .find(|question| question.id() == id)
            .cloned()
    }

    pub(crate) fn close(&self, id: u64) {
        self.lock().questions.retain(|question| question.id() != id);
    }

    /// The open questions, oldest first.
    pub(crate) fn open(&self) -> Vec<Q> {
        self.lock().questions.iter().cloned().collect()
    }

    /// Grants the next connection to the question's server with its fingerprint.
    pub(crate) fn allow_once(&self, question: &Q) {
        let (host, port, fingerprint) = question.grant();
        self.lock()
            .allowances
            .grant(host, port, fingerprint, Instant::now());
    }

    /// Spends a grant for exactly this host, port and fingerprint (a host key).
    pub(crate) fn take_allowance(&self, host: &str, port: u16, fingerprint: &str) -> bool {
        self.lock()
            .allowances
            .take_exact(host, port, fingerprint, Instant::now())
    }

    /// Spends a grant for this host and port: the fingerprint it allows (a certificate, which
    /// the handshake compares later).
    pub(crate) fn take_allowance_for(&self, host: &str, port: u16) -> Option<String> {
        self.lock().allowances.take_for(host, port, Instant::now())
    }
}

/// The "once" grants not used yet. Time is a parameter so the expiry edge can be tested.
#[derive(Default)]
pub(crate) struct Allowances {
    grants: Vec<Allowance>,
}

struct Allowance {
    host: String,
    port: u16,
    fingerprint: String,
    granted: Instant,
}

impl Allowances {
    pub(crate) fn grant(&mut self, host: &str, port: u16, fingerprint: &str, granted: Instant) {
        self.grants.push(Allowance {
            host: host.to_owned(),
            port,
            fingerprint: fingerprint.to_owned(),
            granted,
        });
    }

    pub(crate) fn take_exact(
        &mut self,
        host: &str,
        port: u16,
        fingerprint: &str,
        now: Instant,
    ) -> bool {
        self.take(now, |grant| {
            grant.host == host && grant.port == port && grant.fingerprint == fingerprint
        })
        .is_some()
    }

    pub(crate) fn take_for(&mut self, host: &str, port: u16, now: Instant) -> Option<String> {
        self.take(now, |grant| grant.host == host && grant.port == port)
    }

    /// Drops the grants `now` has outlived, then spends the first that `matches`.
    fn take(&mut self, now: Instant, matches: impl Fn(&Allowance) -> bool) -> Option<String> {
        self.grants
            .retain(|grant| now.saturating_duration_since(grant.granted) < ALLOWANCE_VALID_FOR);
        let found = self.grants.iter().position(matches)?;
        Some(self.grants.swap_remove(found).fingerprint)
    }

    #[cfg(test)]
    pub(crate) fn count(&self) -> usize {
        self.grants.len()
    }
}

#[cfg(test)]
#[path = "question_desk_tests.rs"]
mod tests;
