//! The lists of recent entries, newest first, each kept in one settings value - bounded, no
//! table of its own: the command line's lines ([`CMDLINE`]), the masks typed for Num+ and Num-
//! as Total Commander's "Select group" offers them again ([`MASKS`]), and the names the find-files
//! dialog searched for ([`SEARCH`]). None is a setting of the schema, so `settings.get` and
//! `settings.set` (and so MCP) never read or change them: a command line may hold a password.

use iced::Task;
use rc_db::Db;

use super::Message;

/// The lines run from the command line: at most 30, of at most 2048 bytes, on one line.
pub const CMDLINE: Mru = Mru {
    key: "cmdline.history",
    limit: 30,
    longest: 2048,
    one_line: true,
};

/// The masks of Num+ and Num-: at most 20, of at most 1024 bytes.
pub const MASKS: Mru = Mru {
    key: "panels.mask_history",
    limit: 20,
    longest: 1024,
    one_line: false,
};

/// The names searched for, kept as the masks are.
pub const SEARCH: Mru = Mru {
    key: "search.history",
    ..MASKS
};

/// One list of recent entries and the setting it is stored under.
#[derive(Debug, Clone, Copy)]
pub struct Mru {
    /// The setting the list is stored under.
    pub key: &'static str,
    /// The entries kept.
    pub limit: usize,
    /// An entry longer than this is used but not kept; a stored one is dropped.
    longest: usize,
    /// Whether an entry with a control character (a line break) is refused.
    one_line: bool,
}

impl Mru {
    /// Whether `entry` may be kept: not blank, not overlong, on one line where that is asked.
    fn keepable(self, entry: &str) -> bool {
        !entry.trim().is_empty()
            && entry.len() <= self.longest
            && !(self.one_line && entry.chars().any(char::is_control))
    }

    /// `entry` moved to the front of `history`, once, and the oldest beyond the limit dropped.
    pub fn remember(self, history: &mut Vec<String>, entry: &str) {
        if !self.keepable(entry) {
            return;
        }
        history.retain(|kept| kept != entry);
        history.insert(0, entry.to_owned());
        history.truncate(self.limit);
    }

    /// A stored list as far as it can be trusted: nothing [`Mru::remember`] would not keep, no
    /// repeats, at most the limit.
    pub fn sanitized(self, stored: Vec<String>) -> Vec<String> {
        let mut history: Vec<String> = Vec::with_capacity(self.limit);
        for entry in stored {
            if self.keepable(&entry) && !history.contains(&entry) && history.len() < self.limit {
                history.push(entry);
            }
        }
        history
    }

    /// The stored list; empty when there is none or it does not read.
    pub async fn load(self, db: &Db) -> Vec<String> {
        match db.get::<Vec<String>>(self.key).await {
            Ok(Some(stored)) => self.sanitized(stored),
            _ => Vec::new(),
        }
    }

    /// Stores `history`.
    ///
    /// # Errors
    ///
    /// The database write fails.
    pub async fn save(self, db: &Db, history: &[String]) -> rc_db::Result<()> {
        db.set(self.key, history).await
    }

    /// Stores `history` off the window's thread; a list that cannot be stored is only lost at
    /// the next start, with a warning that names the key.
    pub fn save_task(self, db: Db, history: Vec<String>) -> Task<Message> {
        Task::future(async move {
            if let Err(error) = self.save(&db, &history).await {
                tracing::warn!(key = self.key, %error, "a history was not stored");
            }
        })
        .discard()
    }
}

#[cfg(test)]
#[path = "mru_tests.rs"]
mod tests;
