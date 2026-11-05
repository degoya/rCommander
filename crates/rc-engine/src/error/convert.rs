//! The engine error's helpers: a short list of undo mismatches, the retention error mapped.

use rc_db::RetentionError;
use rc_rename::UndoMismatch;

use super::EngineError;

/// The first few mismatches as `from -> to (problem)`.
pub(super) fn describe_mismatches(mismatches: &[UndoMismatch]) -> String {
    const SHOWN: usize = 5;
    let mut text: Vec<String> = mismatches
        .iter()
        .take(SHOWN)
        .map(|mismatch| {
            let problem = match mismatch.problem {
                rc_rename::UndoProblem::SourceMissing => "gone or renamed since",
                rc_rename::UndoProblem::TargetOccupied => "old name taken",
            };
            format!("{} -> {} ({problem})", mismatch.step.from, mismatch.step.to)
        })
        .collect();
    if mismatches.len() > SHOWN {
        text.push(format!("and {} more", mismatches.len() - SHOWN));
    }
    text.join(", ")
}

impl From<RetentionError> for EngineError {
    fn from(error: RetentionError) -> Self {
        match error {
            RetentionError::UnknownTable { table } => Self::RetentionUnknownTable { table },
            RetentionError::OutOfRange {
                key,
                value,
                min,
                max,
            } => Self::RetentionOutOfRange {
                key,
                value,
                min,
                max,
            },
        }
    }
}
