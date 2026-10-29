//! The viewer's bar: search (F7 or Ctrl+F, then F3 and Shift+F3) and "go to offset" (Ctrl+G).
//!
//! A search looks for text in the file's encoding, with or without case, or for hex bytes
//! (`4F 6B 0A`), forward from the view's first byte or backward from its end, and goes on from
//! the last match. It runs over the whole file in the background with its progress in the
//! status line; Escape stops it. A match is shown at the top of the view, selected.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use rc_engine::viewer::{
    Direction, Match, Needle, SearchOutcome, ViewMode, hex::parse_offset, search::parse_hex,
};

use super::{
    lister::{Lister, Request},
    load::{Found, Seek},
};
use crate::i18n::{self, t};

/// What the bar asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarKind {
    Search,
    GoTo,
}

/// A switch of the search bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindOption {
    Case,
    Hex,
    Backward,
}

#[derive(Debug, Default)]
pub struct Find {
    /// The open bar, if any.
    pub bar: Option<BarKind>,
    pub text: String,
    pub offset: String,
    pub case: bool,
    pub hex: bool,
    pub backward: bool,
    /// What F3 looks for again.
    needle: Option<Needle>,
    last: Option<Match>,
    /// The running search's stop switch.
    pub running: Option<Arc<AtomicBool>>,
    /// Bytes the running search went through.
    pub searched: u64,
}

impl Lister {
    pub(super) fn open_bar(&mut self, kind: BarKind) -> Vec<Request> {
        self.find.bar = Some(kind);
        Vec::new()
    }

    /// The bar's text changed.
    pub fn bar_input(&mut self, text: String) {
        match self.find.bar {
            Some(BarKind::GoTo) => self.find.offset = text,
            _ => self.find.text = text,
        }
    }

    pub fn toggle(&mut self, option: FindOption) {
        let find = &mut self.find;
        match option {
            FindOption::Case => find.case = !find.case,
            FindOption::Hex => find.hex = !find.hex,
            FindOption::Backward => find.backward = !find.backward,
        }
    }

    /// Enter in the bar: the search starts, or the view goes to the offset. While a search
    /// runs the bar stays open with its text, the status line says it runs and that Escape
    /// stops it; Enter then searches (RC-080-44).
    pub fn submit(&mut self) -> Vec<Request> {
        match self.find.bar {
            Some(BarKind::Search) if self.find.running.is_some() => Vec::new(),
            Some(BarKind::Search) => {
                let needle = if self.find.hex {
                    parse_hex(&self.find.text).and_then(|bytes| Needle::bytes(&bytes))
                } else {
                    Needle::text(&self.find.text, self.find.case, self.detected)
                };
                match needle {
                    Ok(needle) => {
                        self.find.bar = None;
                        self.find.needle = Some(needle);
                        self.find.last = None;
                        let direction = if self.find.backward {
                            Direction::Backward
                        } else {
                            Direction::Forward
                        };
                        self.start(direction)
                    }
                    Err(error) => {
                        self.failed(error.code());
                        Vec::new()
                    }
                }
            }
            Some(BarKind::GoTo) => match parse_offset(&self.find.offset) {
                Some(offset) => {
                    self.find.bar = None;
                    self.show(Seek::At(offset.min(self.size())), None)
                }
                None => {
                    self.note = Some(t!("viewer.bad_offset").to_owned());
                    Vec::new()
                }
            },
            None => Vec::new(),
        }
    }

    /// F3 and Shift+F3: the last search again from its match; without one, the bar opens.
    pub(super) fn find_again(&mut self, direction: Direction) -> Vec<Request> {
        if self.find.needle.is_none() {
            return self.open_bar(BarKind::Search);
        }
        self.start(direction)
    }

    fn start(&mut self, direction: Direction) -> Vec<Request> {
        let (Some(needle), None) = (self.find.needle.clone(), &self.find.running) else {
            return Vec::new();
        };
        let from = match (direction, self.find.last) {
            (Direction::Forward, Some(last)) => last.offset + 1,
            (Direction::Forward, None) => self.top,
            (Direction::Backward, Some(last)) => last.offset,
            (Direction::Backward, None) => self.end.max(self.top),
        };
        let cancel = Arc::new(AtomicBool::new(false));
        self.find.running = Some(Arc::clone(&cancel));
        self.find.searched = 0;
        self.note = None;
        vec![Request::Search {
            needle,
            from,
            direction,
            cancel,
        }]
    }

    /// News of the running search.
    pub fn found(&mut self, news: Found) -> Vec<Request> {
        let outcome = match news {
            Found::Progress(bytes) => {
                self.find.searched = bytes;
                return Vec::new();
            }
            Found::Done(outcome) => outcome,
        };
        self.find.running = None;
        match outcome {
            Ok(SearchOutcome::Found(found)) => {
                self.find.last = Some(found);
                if self.mode == ViewMode::Image {
                    self.mode = ViewMode::Text;
                }
                self.show(Seek::At(found.offset), Some(found))
            }
            Ok(SearchOutcome::NotFound) => {
                self.note = Some(t!("viewer.not_found").to_owned());
                Vec::new()
            }
            Ok(SearchOutcome::Cancelled) => {
                self.note = Some(t!("viewer.search_cancelled").to_owned());
                Vec::new()
            }
            Err(code) => {
                self.note = Some(i18n::error_text(code));
                Vec::new()
            }
        }
    }

    /// Escape: a running search stops, then an open bar closes; `true` when neither was there
    /// and the viewer closes.
    pub fn escape(&mut self) -> bool {
        if let Some(cancel) = &self.find.running {
            cancel.store(true, Ordering::Relaxed);
            return false;
        }
        self.find.bar.take().is_none()
    }

    /// The running search's progress in percent of the file.
    pub fn search_percent(&self) -> Option<u64> {
        self.find.running.as_ref()?;
        let size = self.size().max(1);
        Some((self.find.searched.min(size) * 100) / size)
    }
}

#[cfg(test)]
#[path = "find_tests.rs"]
mod tests;
