//! What a shell says about itself between its output: its folder (OSC 7, OSC 9;9) and where
//! its prompt stands (OSC 133).
//!
//! `alacritty_terminal` ignores these sequences, so the session looks for them itself in the
//! program's output, before the terminal parses it ([`OscScanner`]). Everything here is input
//! from the program and so untrusted: a payload is bounded ([`MAX_PAYLOAD`]), a folder must be
//! UTF-8 without control characters, an OSC 7 URL naming another host (an SSH session
//! started inside the shell) is ignored, and so is a folder on a network share, however it is
//! spelled - only a WSL distribution's share passes (RC-080-10).
//!
//! The scanner is also the terminal parser's gate (RC-080-65): `alacritty_terminal`'s parser
//! (`vte` with `std`) keeps an OSC sequence in a buffer without a limit, so the scanner holds an
//! open sequence back and hands it on whole at its end - or drops it, when it grew past
//! [`MAX_SEQUENCE`] ([`MAX_CLIPBOARD_SEQUENCE`] for OSC 52). The parser then sees a `CAN` in its
//! place and never the sequence. The scanner follows the parser's own rule for where an OSC
//! starts and ends: `ESC ]` from any state, ended by `BEL`, `CAN`, `SUB` or any `ESC`.

use std::sync::OnceLock;

use rc_vfs::network_path;

/// The longest OSC payload kept; a longer one is dropped whole.
pub const MAX_PAYLOAD: usize = 4096;

/// The longest OSC sequence the terminal's parser sees; a longer one is dropped whole.
pub const MAX_SEQUENCE: usize = 64 * 1024;

/// The most text one OSC 52 may put on the clipboard (RC-080-65).
pub const MAX_CLIPBOARD: usize = 1024 * 1024;

/// The longest OSC 52 sequence the terminal's parser sees: [`MAX_CLIPBOARD`] in base64 and the
/// selection's name before it.
pub const MAX_CLIPBOARD_SEQUENCE: usize = MAX_CLIPBOARD.div_ceil(3) * 4 + 64;

const BEL: u8 = 0x07;
const ESC: u8 = 0x1b;
/// CAN and SUB abort a sequence.
const CAN: u8 = 0x18;
const SUB: u8 = 0x1a;

/// Finds OSC payloads in a byte stream, across chunk boundaries, and passes the stream on
/// without the overlong ones. Only the 7-bit forms count (`ESC ]` ... `BEL` or `ESC \`): in
/// UTF-8 output the 8-bit C1 bytes are continuation bytes, and the parser reads them so too.
#[derive(Debug, Default)]
pub struct OscScanner {
    state: ScanState,
    payload: Vec<u8>,
    /// The payload grew past [`MAX_PAYLOAD`]; it is not reported.
    overflow: bool,
    /// The open sequence from its `]` on, held back from the parser until it ends.
    held: Vec<u8>,
    /// The open sequence grew past its limit: the parser sees none of it.
    dropped: bool,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum ScanState {
    /// Anything but the two states below: from each of them only an ESC leads on.
    #[default]
    Ground,
    /// After an ESC; `closed` when that ESC ended an OSC, whose payload counts if `\` follows.
    Escape {
        closed: bool,
    },
    Osc,
}

impl OscScanner {
    /// Whether no sequence is open: output without an ESC then holds no payload.
    #[must_use]
    pub fn is_idle(&self) -> bool {
        self.state == ScanState::Ground
    }

    /// Feeds `bytes`; `found` gets every complete payload, in order, and `forward` what the
    /// terminal's parser is to see of them: every byte, but an open OSC only once it ends and
    /// an overlong one not at all.
    pub fn feed(&mut self, bytes: &[u8], forward: &mut Vec<u8>, mut found: impl FnMut(&[u8])) {
        let mut rest = bytes;
        while !rest.is_empty() {
            if self.state == ScanState::Ground {
                // Up to the next ESC nothing can start: passed on in one piece.
                let run = rest
                    .iter()
                    .position(|&byte| byte == ESC)
                    .unwrap_or(rest.len());
                let (plain, tail) = rest.split_at(run);
                forward.extend_from_slice(plain);
                rest = tail;
            }
            let Some((&byte, tail)) = rest.split_first() else {
                break;
            };
            rest = tail;
            self.state = self.step(byte, forward, &mut found);
        }
    }

    fn step(
        &mut self,
        byte: u8,
        forward: &mut Vec<u8>,
        found: &mut impl FnMut(&[u8]),
    ) -> ScanState {
        match self.state {
            ScanState::Osc => match byte {
                BEL => {
                    self.release(forward, byte);
                    self.report(found);
                    ScanState::Ground
                }
                CAN | SUB => {
                    self.release(forward, byte);
                    ScanState::Ground
                }
                ESC => {
                    self.release(forward, byte);
                    ScanState::Escape { closed: true }
                }
                _ => {
                    self.push(byte);
                    ScanState::Osc
                }
            },
            ScanState::Escape { .. } if byte == b']' => self.begin(),
            ScanState::Escape { closed } => {
                forward.push(byte);
                match byte {
                    b'\\' => {
                        if closed {
                            self.report(found);
                        }
                        ScanState::Ground
                    }
                    CAN | SUB => ScanState::Ground,
                    // The parser stays in its escape state on these, so `]` may still follow.
                    0x00..=0x1f | 0x7f..=0xff => ScanState::Escape { closed: false },
                    _ => ScanState::Ground,
                }
            }
            ScanState::Ground => {
                forward.push(byte);
                if byte == ESC {
                    ScanState::Escape { closed: false }
                } else {
                    ScanState::Ground
                }
            }
        }
    }

    fn begin(&mut self) -> ScanState {
        self.payload.clear();
        self.overflow = false;
        self.held.clear();
        self.held.push(b']');
        self.dropped = false;
        ScanState::Osc
    }

    fn push(&mut self, byte: u8) {
        if self.payload.len() < MAX_PAYLOAD {
            self.payload.push(byte);
        } else {
            self.overflow = true;
        }
        if self.dropped {
            return;
        }
        let limit = if self.held.starts_with(b"]52;") {
            MAX_CLIPBOARD_SEQUENCE
        } else {
            MAX_SEQUENCE
        };
        if self.held.len() < limit {
            self.held.push(byte);
        } else {
            self.dropped = true;
            self.held = Vec::new();
        }
    }

    /// Hands the ended sequence and its `terminator` on, or a `CAN` for a dropped one: the
    /// parser saw its ESC, and the CAN ends its escape state as if nothing had followed.
    fn release(&mut self, forward: &mut Vec<u8>, terminator: u8) {
        if self.dropped {
            forward.push(CAN);
            if terminator == ESC {
                forward.push(ESC);
            }
        } else {
            forward.extend_from_slice(&self.held);
            forward.push(terminator);
        }
        if self.held.capacity() > MAX_PAYLOAD {
            self.held = Vec::new();
        } else {
            self.held.clear();
        }
        self.dropped = false;
    }

    fn report(&mut self, found: &mut impl FnMut(&[u8])) {
        if !self.overflow {
            found(&self.payload);
        }
        self.payload.clear();
    }

    /// The bytes held back now, for the tests' memory bound.
    #[cfg(test)]
    pub(crate) fn held_capacity(&self) -> usize {
        self.held.capacity()
    }
}

/// A mark of OSC 133 (FinalTerm's semantic prompt).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptMark {
    /// `A`: the prompt starts.
    PromptStart,
    /// `B`: the prompt ends, the command line starts.
    InputStart,
    /// `C`: the command runs.
    CommandStart,
    /// `D`: the command ended.
    CommandEnd,
}

/// What one OSC payload says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OscEvent {
    /// The shell's folder as the shell names it (a path of the shell's own system).
    Folder(String),
    Prompt(PromptMark),
}

/// The meaning of `payload`, or `None` for a sequence this module does not read or one that
/// is not valid. `hosts` are the names an OSC 7 URL may give for this machine (besides an
/// empty host and `localhost`), compared ignoring ASCII case.
#[must_use]
pub fn parse_osc(payload: &[u8], hosts: &[String]) -> Option<OscEvent> {
    let text = std::str::from_utf8(payload).ok()?;
    if let Some(url) = text.strip_prefix("7;") {
        return file_url_path(url, hosts).map(OscEvent::Folder);
    }
    if let Some(path) = text.strip_prefix("9;9;") {
        let path = path
            .strip_prefix('"')
            .and_then(|inner| inner.strip_suffix('"'))
            .unwrap_or(path);
        return plain(path).map(|path| OscEvent::Folder(path.to_owned()));
    }
    let mark = text.strip_prefix("133;")?;
    let kind = mark.split(';').next()?;
    let mark = match kind {
        "A" => PromptMark::PromptStart,
        "B" => PromptMark::InputStart,
        "C" => PromptMark::CommandStart,
        "D" => PromptMark::CommandEnd,
        _ => return None,
    };
    Some(OscEvent::Prompt(mark))
}

/// The path of a `file://<host>/<path>` URL of this machine, percent-decoded.
fn file_url_path(url: &str, hosts: &[String]) -> Option<String> {
    let scheme = url.get(..7)?;
    if !scheme.eq_ignore_ascii_case("file://") {
        return None;
    }
    let rest = &url[7..];
    let slash = rest.find('/')?;
    let (host, path) = rest.split_at(slash);
    let local = host.is_empty()
        || host.eq_ignore_ascii_case("localhost")
        || hosts.iter().any(|name| name.eq_ignore_ascii_case(host));
    if !local {
        return None;
    }
    // A query or a fragment is no part of the path; a shell encodes `?` and `#` in names.
    let path = path.split(['?', '#']).next()?;
    let decoded = percent_decode(path)?;
    plain(&decoded).map(str::to_owned)
}

/// `text` with every `%XX` replaced by its byte; `None` for a broken escape or bytes that are
/// not UTF-8.
fn percent_decode(text: &str) -> Option<String> {
    let mut bytes = Vec::with_capacity(text.len());
    let mut rest = text.as_bytes();
    while let Some((&byte, tail)) = rest.split_first() {
        if byte == b'%' {
            let hex = tail
                .get(..2)
                .filter(|hex| hex.iter().all(u8::is_ascii_hexdigit))?;
            bytes.push(u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()?);
            rest = &tail[2..];
        } else {
            bytes.push(byte);
            rest = tail;
        }
    }
    String::from_utf8(bytes).ok()
}

/// `path` when it is a usable folder name: not empty, no control characters, and no share on
/// another machine ([`network_path`]; a WSL distribution's share is none). A panel that followed
/// `\\server\share` would make Windows sign in there with the person's NTLM hash (RC-080-10).
fn plain(path: &str) -> Option<&str> {
    (!path.is_empty() && !path.chars().any(char::is_control) && network_path(path).is_none())
        .then_some(path)
}

/// The most characters of a title a program sets that a session keeps (RC-080-65).
pub const MAX_TITLE: usize = 256;

/// A title as the program set it, made safe to show: no bidi embedding, override or isolate
/// controls (U+202A...U+202E, U+2066...U+2069), which could turn the text around it in a tab or
/// a list, no other control characters, and at most [`MAX_TITLE`] characters.
#[must_use]
pub fn clean_title(title: &str) -> String {
    title
        .chars()
        .filter(|&c| {
            !c.is_control() && !matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        })
        .take(MAX_TITLE)
        .collect()
}

/// The names this machine goes by, for [`parse_osc`]: `COMPUTERNAME` (Windows, also the name a
/// WSL distribution takes by default), `HOSTNAME`, and the kernel's host name on Linux. Read once.
#[must_use]
pub fn local_host_names() -> &'static [String] {
    static NAMES: OnceLock<Vec<String>> = OnceLock::new();
    NAMES.get_or_init(|| {
        let mut names: Vec<String> = ["COMPUTERNAME", "HOSTNAME"]
            .into_iter()
            .filter_map(|name| std::env::var(name).ok())
            .collect();
        if let Ok(name) = std::fs::read_to_string("/proc/sys/kernel/hostname") {
            names.push(name.trim().to_owned());
        }
        names.retain(|name| !name.is_empty());
        names
    })
}

#[cfg(test)]
#[path = "osc_tests.rs"]
mod tests;
