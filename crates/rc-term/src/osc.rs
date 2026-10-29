//! What a shell says about itself between its output: its folder (OSC 7, OSC 9;9) and where
//! its prompt stands (OSC 133).
//!
//! `alacritty_terminal` ignores these sequences, so the session looks for them itself in the
//! program's output, before the terminal parses it ([`OscScanner`]). Everything here is input
//! from the program and so untrusted: a payload is bounded ([`MAX_PAYLOAD`]), a folder must be
//! UTF-8 without control characters, an OSC 7 URL naming another host (an SSH session
//! started inside the shell) is ignored, and so is a folder on a network share, however it is
//! spelled - only a WSL distribution's share passes (RC-080-10).

use std::sync::OnceLock;

use rc_vfs::network_path;

/// The longest OSC payload kept; a longer one is dropped whole.
pub const MAX_PAYLOAD: usize = 4096;

const BEL: u8 = 0x07;
const ESC: u8 = 0x1b;
/// CAN and SUB abort a sequence.
const CAN: u8 = 0x18;
const SUB: u8 = 0x1a;

/// Finds OSC payloads in a byte stream, across chunk boundaries. Only the 7-bit forms count
/// (`ESC ]` ... `BEL` or `ESC \`): in UTF-8 output the 8-bit C1 bytes are continuation bytes.
#[derive(Debug, Default)]
pub struct OscScanner {
    state: ScanState,
    payload: Vec<u8>,
    /// The payload grew past [`MAX_PAYLOAD`]; it is dropped at its end.
    overflow: bool,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum ScanState {
    #[default]
    Ground,
    Escape,
    Osc,
    /// An ESC inside an OSC: `\` ends it, anything else aborts it.
    OscEscape,
}

impl OscScanner {
    /// Whether no sequence is open: output without an ESC then holds no payload.
    #[must_use]
    pub fn is_idle(&self) -> bool {
        self.state == ScanState::Ground
    }

    /// Feeds `bytes`; `found` gets every complete payload, in order.
    pub fn feed(&mut self, bytes: &[u8], mut found: impl FnMut(&[u8])) {
        for &byte in bytes {
            self.state = match self.state {
                ScanState::Ground | ScanState::Escape if byte == ESC => ScanState::Escape,
                ScanState::Escape if byte == b']' => self.begin(),
                ScanState::Ground | ScanState::Escape => ScanState::Ground,
                ScanState::Osc => match byte {
                    BEL => self.end(&mut found),
                    ESC => ScanState::OscEscape,
                    CAN | SUB => ScanState::Ground,
                    _ => {
                        self.push(byte);
                        ScanState::Osc
                    }
                },
                ScanState::OscEscape => match byte {
                    b'\\' => self.end(&mut found),
                    b']' => self.begin(),
                    ESC => ScanState::Escape,
                    _ => ScanState::Ground,
                },
            };
        }
    }

    fn begin(&mut self) -> ScanState {
        self.payload.clear();
        self.overflow = false;
        ScanState::Osc
    }

    fn push(&mut self, byte: u8) {
        if self.payload.len() < MAX_PAYLOAD {
            self.payload.push(byte);
        } else {
            self.overflow = true;
        }
    }

    fn end(&mut self, found: &mut impl FnMut(&[u8])) -> ScanState {
        if !self.overflow {
            found(&self.payload);
        }
        self.payload.clear();
        ScanState::Ground
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
