//! The component: `src/lib.rs` translated into the vocabulary of `wit/rcommander.wit`. Built
//! only for `wasm32`; it holds no logic of its own, so it needs no test of its own.
#![allow(
    unsafe_code,
    reason = "the canonical-ABI glue wit-bindgen generates is the only unsafe code here"
)]

wit_bindgen::generate!({
    path: "wit",
    world: "compare-plugin",
});

use exports::rcommander::plugin::compare::{self as wit, Guest};
use rcommander::plugin::{source, types as wit_types};

use crate::types::{ChangeKind, Failure, FailureKind};

struct Component;

/// The most bytes one `read-at` hands over.
const WINDOW: u32 = 1024 * 1024;

/// A source read whole, window by window.
fn read_all(src: wit_types::Source) -> Result<Vec<u8>, Failure> {
    let size = source::info(src).size;
    let mut bytes = Vec::new();
    while (bytes.len() as u64) < size {
        if source::should_stop() {
            return Err(Failure::of(FailureKind::Io));
        }
        let window = source::read_at(src, bytes.len() as u64, WINDOW)
            .map_err(|_| Failure::of(FailureKind::Io))?;
        if window.is_empty() {
            break;
        }
        bytes.extend_from_slice(&window);
    }
    Ok(bytes)
}

impl Guest for Component {
    fn status() -> wit::CompareState {
        wit::CompareState::Ready
    }

    fn compare(
        a: wit_types::Source,
        b: wit_types::Source,
    ) -> Result<wit::Comparison, wit_types::Failure> {
        let old = read_all(a).map_err(to_wit)?;
        let new = read_all(b).map_err(to_wit)?;
        let found = crate::compare(&old, &new).map_err(to_wit)?;
        let summary = wit::CompareSummary {
            added: found.count(ChangeKind::Added),
            removed: found.count(ChangeKind::Removed),
            changed: found.count(ChangeKind::Changed),
            moved: found.count(ChangeKind::Moved),
            truncated: found.truncated,
        };
        Ok(wit::Comparison {
            summary,
            changes: found
                .changes
                .into_iter()
                .map(|change| wit::Change {
                    kind: match change.kind {
                        ChangeKind::Added => wit::ChangeKind::Added,
                        ChangeKind::Removed => wit::ChangeKind::Removed,
                        ChangeKind::Changed => wit::ChangeKind::Changed,
                        ChangeKind::Moved => wit::ChangeKind::Moved,
                    },
                    path: change.path,
                    column: change.column,
                    old_value: change.old_value,
                    new_value: change.new_value,
                    old_line: change.old_line,
                    new_line: change.new_line,
                })
                .collect(),
            notices: found
                .notices
                .into_iter()
                .map(|notice| wit::Notice {
                    code: notice.code,
                    params: notice.params,
                })
                .collect(),
        })
    }
}

fn to_wit(failure: Failure) -> wit_types::Failure {
    wit_types::Failure {
        kind: match failure.kind {
            FailureKind::BadData => wit_types::FailureKind::BadData,
            FailureKind::Io => wit_types::FailureKind::Io,
        },
        code: failure.code,
        params: Vec::new(),
    }
}

export!(Component);
