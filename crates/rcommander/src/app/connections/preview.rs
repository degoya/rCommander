//! What the ssh_config, Total Commander and Remote Desktop Manager import pages share
//! ([`ImportPage`]): where the page stands, what went wrong, and what its preview was read from
//! (RC-080-39) - the import that follows a preview takes the paths held here, never the fields'
//! text of the moment, and a preview's answer counts only for the read asked last. A path
//! changed meanwhile drops the preview, which is read again. Escape and Enter mean the same on
//! each page ([`page_key`]).

use rc_engine::Chord;

use crate::i18n::error_text;

/// Where an import stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// The preview is on its way.
    Reading,
    Ready,
    Importing,
    Done,
}

/// An import page's own state, `S` its paths as the page's fields hold them.
#[derive(Debug, Clone)]
pub struct ImportPage<S> {
    pub phase: Phase,
    pub error: Option<String>,
    /// The paths the shown preview was read from.
    pub held: Held<S>,
}

impl<S> ImportPage<S> {
    pub fn new(phase: Phase) -> Self {
        Self {
            phase,
            error: None,
            held: Held {
                reads: 0,
                source: None,
            },
        }
    }

    /// A preview of `source` is asked: the number its answer brings back.
    pub fn read(&mut self, source: S) -> u32 {
        self.phase = Phase::Reading;
        self.error = None;
        self.held.ask(source)
    }

    /// A path changed: the preview no longer counts, nothing is to import until it is read
    /// again.
    pub fn changed(&mut self) {
        self.held.forget();
        self.error = None;
        self.phase = Phase::Done;
    }

    /// Whether the sources hold still: the import runs.
    pub fn importing(&self) -> bool {
        self.phase == Phase::Importing
    }

    /// The paths the import takes: those of the preview shown, once it is ready.
    pub fn ready_source(&self) -> Option<&S> {
        self.held.source().filter(|_| self.phase == Phase::Ready)
    }

    /// The engine refused with `code`: the import (`import`) can be tried again, a preview
    /// leaves nothing to import.
    pub fn failed(&mut self, code: &'static str, import: bool) {
        self.error = Some(error_text(code));
        self.phase = if import { Phase::Ready } else { Phase::Done };
    }
}

/// What a key does on an import page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageKey {
    /// Escape: back to where the page came from.
    Leave,
    /// Enter: the import.
    Import,
}

/// A plain Escape or Enter no field took; `None` for any other key.
pub fn page_key(chord: &Chord, captured: bool) -> Option<PageKey> {
    if !chord.is_plain() || captured {
        return None;
    }
    match chord.key.as_str() {
        "Escape" => Some(PageKey::Leave),
        "Enter" => Some(PageKey::Import),
        _ => None,
    }
}

/// The paths of the shown preview, `S` as the page's fields hold them.
#[derive(Debug, Clone, Default)]
pub struct Held<S> {
    /// Counts the reads asked; a preview's answer brings its number back.
    reads: u32,
    /// The paths of the read asked last, until a field changes.
    source: Option<S>,
}

impl<S> Held<S> {
    /// A read of `source` is asked: the number its answer brings back.
    pub fn ask(&mut self, source: S) -> u32 {
        self.reads = self.reads.wrapping_add(1);
        self.source = Some(source);
        self.reads
    }

    /// Whether the answer of read `read` is the one awaited: the last asked, not dropped since.
    pub fn answers(&self, read: u32) -> bool {
        self.source.is_some() && read == self.reads
    }

    /// The paths the preview was read from, which the import takes.
    pub fn source(&self) -> Option<&S> {
        self.source.as_ref()
    }

    /// A path changed: the preview, and a read still on its way, no longer count.
    pub fn forget(&mut self) {
        self.reads = self.reads.wrapping_add(1);
        self.source = None;
    }
}

#[cfg(test)]
#[path = "preview_tests.rs"]
mod tests;
