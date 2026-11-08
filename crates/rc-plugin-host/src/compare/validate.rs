//! What a compare plugin answers, held to the host's rules (`docs/plugins.md`, "Validation of
//! plugin output"): the list as a whole bounded - more than [`MAX_CHANGES`] is refused, not cut,
//! so a comparison never looks complete when it is not - every text cut and spelled out, the
//! counts no lower than the list, the notices held to a failure's rules.

use rc_plugin_api::compare::{
    Change, ChangeKind, CompareState, CompareSummary, Comparison, MAX_CHANGES, MAX_NOTICES, Notice,
};
use rc_plugin_types::{Failure, FailureKind};

use crate::{
    bindings::compare_world::exports::rcommander::plugin::compare as wit,
    failure::{self, codes, readable},
    viewer::{plugin_code, plugin_params},
};

/// The longest path kept (a JSON Pointer, a row's key values); a longer one is cut.
pub const MAX_PATH_CHARS: usize = 1024;
/// The longest column name kept.
pub const MAX_COLUMN_CHARS: usize = 256;
/// The longest old or new value kept: a change shows a value, the files hold the rest.
pub const MAX_CHANGE_VALUE_CHARS: usize = 1024;

/// The comparison `compare` answered.
pub(super) fn comparison(raw: wit::Comparison) -> Result<Comparison, Failure> {
    if raw.changes.len() > MAX_CHANGES {
        return Err(
            failure::coded(FailureKind::BadData, codes::PLUGIN_COMPARE_TOO_LARGE)
                .with_param("count", raw.changes.len().to_string())
                .with_param("limit", MAX_CHANGES.to_string()),
        );
    }
    let changes: Vec<Change> = raw.changes.into_iter().map(change).collect();
    let mut listed = CompareSummary::default();
    for change in &changes {
        listed.count(change.kind);
    }
    let summary = CompareSummary {
        added: raw.summary.added,
        removed: raw.summary.removed,
        changed: raw.summary.changed,
        moved: raw.summary.moved,
        truncated: raw.summary.truncated,
    };
    // A count below the changes listed is the plugin contradicting itself.
    if ChangeKind::ALL
        .iter()
        .any(|kind| summary.of(*kind) < listed.of(*kind))
    {
        return Err(failure::coded(
            FailureKind::BadData,
            codes::PLUGIN_BAD_OUTPUT,
        ));
    }
    let summary = CompareSummary {
        // More counted than listed: the list is a part, whatever the plugin says.
        truncated: summary.truncated || summary.total() > listed.total(),
        ..summary
    };
    let notices = raw
        .notices
        .into_iter()
        .filter(|notice| plugin_code(&notice.code))
        .take(MAX_NOTICES)
        .map(|notice| Notice {
            code: notice.code,
            params: plugin_params(notice.params),
        })
        .collect();
    Ok(Comparison {
        summary,
        changes,
        notices,
    })
}

/// One change with its texts cut and spelled out; a line 0 is no line.
fn change(raw: wit::Change) -> Change {
    let value = |text: Option<String>| text.map(|text| readable(&text, MAX_CHANGE_VALUE_CHARS));
    Change {
        kind: kind(raw.kind),
        path: readable(&raw.path, MAX_PATH_CHARS),
        column: raw.column.map(|column| readable(&column, MAX_COLUMN_CHARS)),
        old_value: value(raw.old_value),
        new_value: value(raw.new_value),
        old_line: raw.old_line.filter(|line| *line > 0),
        new_line: raw.new_line.filter(|line| *line > 0),
    }
}

fn kind(kind: wit::ChangeKind) -> ChangeKind {
    match kind {
        wit::ChangeKind::Added => ChangeKind::Added,
        wit::ChangeKind::Removed => ChangeKind::Removed,
        wit::ChangeKind::Changed => ChangeKind::Changed,
        wit::ChangeKind::Moved => ChangeKind::Moved,
    }
}

pub(super) fn state(state: wit::CompareState) -> CompareState {
    match state {
        wit::CompareState::Ready => CompareState::Ready,
        wit::CompareState::Unlicensed => CompareState::Unlicensed,
    }
}

#[cfg(test)]
#[path = "validate_tests.rs"]
mod tests;
