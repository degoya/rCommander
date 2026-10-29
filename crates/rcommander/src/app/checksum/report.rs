//! What a finished checksum job shows: a check's lines and a create's digests as a section of
//! the result list, a split's or a combine's summary as a line for the status bar.

use rc_engine::{JobKind, JobReport, JobSnapshot, VerifyEntry, VerifyStatus};

use crate::{
    format,
    i18n::{code_text, error_text, t},
};

/// One line of the result list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// The line did not check out: drawn as a warning.
    pub bad: bool,
    /// "OK", "Different", ...; for a created checksum file empty.
    pub status: String,
    pub name: String,
    /// Why the line is bad, or the digest written.
    pub detail: String,
}

/// One job's part of the result list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    /// What was checked or written, and how it came out.
    pub title: String,
    pub rows: Vec<Row>,
    /// Lines the job did not keep (`rc_engine::JobReport`).
    pub omitted: usize,
}

/// How a finished job shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shown {
    List(Section),
    Status(String),
}

/// What `job` shows once it completed; `None` for a job that is no checksum job or has no
/// report (failed, cancelled).
#[must_use]
pub fn shown(job: &JobSnapshot) -> Option<Shown> {
    let file = |path: &rc_engine::VfsPath| {
        path.path.file_name().map_or_else(
            || path.path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        )
    };
    Some(match (&job.kind, job.report.as_ref()?) {
        (
            JobKind::ChecksumVerify { file: path },
            JobReport::ChecksumsVerified { report, omitted },
        ) => {
            let summary = &report.summary;
            let title = t!(
                "checksum.result.verified",
                file = file(path),
                ok = summary.ok,
                mismatch = summary.mismatch,
                missing = summary.missing,
                unreadable = summary.unreadable,
                invalid = summary.invalid,
            );
            // The bad lines first, each group in the file's order (RC-020-05, RC-020-10).
            let mut rows: Vec<Row> = report.entries.iter().map(verify_row).collect();
            rows.sort_by_key(|row| !row.bad);
            Shown::List(Section {
                title,
                rows,
                omitted: *omitted,
            })
        }
        (_, JobReport::ChecksumsCreated { report, omitted }) => {
            let title = t!(
                "checksum.result.created",
                count = report.files.len() + omitted,
                files = report.written.join(", "),
            );
            let rows = report
                .files
                .iter()
                .map(|file| Row {
                    bad: false,
                    status: String::new(),
                    name: file.name.clone(),
                    detail: file.hex.clone(),
                })
                .collect();
            Shown::List(Section {
                title,
                rows,
                omitted: *omitted,
            })
        }
        (_, JobReport::Split { report }) => Shown::Status(t!(
            "checksum.result.split",
            size = format::size(report.size),
            count = report.parts.len(),
            crc = report.crc_file,
        )),
        (_, JobReport::Combined { report }) => {
            let text = if report.crc_checked {
                t!(
                    "checksum.result.combined_checked",
                    count = report.parts.len(),
                    name = report.target,
                )
            } else {
                t!(
                    "checksum.result.combined",
                    count = report.parts.len(),
                    name = report.target,
                )
            };
            Shown::Status(text)
        }
        _ => return None,
    })
}

/// A checked line: its status, and why it is bad.
fn verify_row(entry: &VerifyEntry) -> Row {
    let status = match entry.status {
        VerifyStatus::Ok => "ok",
        VerifyStatus::Mismatch => "mismatch",
        VerifyStatus::Missing => "missing",
        VerifyStatus::Unreadable => "unreadable",
        VerifyStatus::Invalid => "invalid",
    };
    let detail = match (entry.status, entry.code.as_deref()) {
        (VerifyStatus::Invalid, Some(code)) => code_text("checksum.invalid", code),
        (VerifyStatus::Unreadable, Some(code)) => error_text(code),
        _ => String::new(),
    };
    let name = if entry.name.is_empty() {
        t!("checksum.result.line", line = entry.line)
    } else {
        entry.name.clone()
    };
    Row {
        bad: entry.status != VerifyStatus::Ok,
        status: code_text("checksum.status", status),
        name,
        detail,
    }
}

#[cfg(test)]
#[path = "report_tests.rs"]
mod tests;
