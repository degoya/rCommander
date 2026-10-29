use rc_engine::{
    Algorithm, CombineReport, CreateReport, FileChecksum, JobOptions, JobState, Progress,
    SplitReport, VerifyReport, VerifySummary, VfsPath,
};

use super::*;

fn job(kind: JobKind, report: JobReport) -> JobSnapshot {
    JobSnapshot {
        id: 1,
        kind,
        options: JobOptions::default(),
        state: JobState::Completed,
        progress: Progress::default(),
        question: None,
        skipped: 0,
        failures: Vec::new(),
        report: Some(report),
        resumable: false,
        batch: None,
    }
}

fn entry(line: usize, name: &str, status: VerifyStatus, code: Option<&str>) -> VerifyEntry {
    VerifyEntry {
        line,
        name: name.into(),
        status,
        algorithm: Some(Algorithm::Sha256),
        expected: None,
        actual: None,
        code: code.map(str::to_owned),
    }
}

#[test]
fn a_check_lists_every_line_with_its_status_and_reason() {
    let report = VerifyReport {
        entries: vec![
            entry(1, "a.iso", VerifyStatus::Ok, None),
            entry(2, "b.iso", VerifyStatus::Mismatch, None),
            entry(3, "../x", VerifyStatus::Invalid, Some("parent_reference")),
            entry(4, "", VerifyStatus::Invalid, Some("unknown_format")),
        ],
        summary: VerifySummary {
            ok: 1,
            mismatch: 1,
            invalid: 2,
            ..VerifySummary::default()
        },
    };
    let kind = JobKind::ChecksumVerify {
        file: VfsPath::local("/d/disk.sha256"),
    };
    let Some(Shown::List(section)) = shown(&job(
        kind,
        JobReport::ChecksumsVerified { report, omitted: 2 },
    )) else {
        panic!("a section");
    };
    assert!(section.title.contains("disk.sha256"), "{}", section.title);
    assert_eq!(section.omitted, 2);
    let bad: Vec<bool> = section.rows.iter().map(|row| row.bad).collect();
    // The bad lines first, in the file's order; the good ones after them.
    assert_eq!(bad, vec![true, true, true, false]);
    assert_eq!(section.rows[3].status, t!("checksum.status.ok"));
    assert_eq!(
        section.rows[1].detail,
        t!("checksum.invalid.parent_reference")
    );
    assert!(
        section.rows[2].name.contains('4'),
        "the line's number names it"
    );
}

#[test]
fn a_create_lists_its_digests_and_split_and_combine_say_it_in_a_line() {
    let created = CreateReport {
        algorithm: Algorithm::Sha256,
        files: vec![FileChecksum {
            name: "a.iso".into(),
            hex: "abc123".into(),
        }],
        written: vec!["a.iso.sha256".into()],
    };
    let kind = JobKind::ChecksumCreate {
        dir: VfsPath::local("/d"),
        names: vec!["a.iso".into()],
        algorithm: Algorithm::Sha256,
        layout: rc_engine::Layout::PerFile,
    };
    let Some(Shown::List(section)) = shown(&job(
        kind.clone(),
        JobReport::ChecksumsCreated {
            report: created,
            omitted: 0,
        },
    )) else {
        panic!("a section");
    };
    assert_eq!(section.rows[0].detail, "abc123");
    assert!(section.title.contains("a.iso.sha256"), "{}", section.title);

    let split = JobReport::Split {
        report: SplitReport {
            parts: vec!["disk.001".into(), "disk.002".into()],
            crc_file: "disk.crc".into(),
            size: 10,
            crc32: "0".into(),
        },
    };
    let Some(Shown::Status(line)) = shown(&job(kind.clone(), split)) else {
        panic!("a status line");
    };
    assert!(line.contains("disk.crc"), "{line}");
    let combined = JobReport::Combined {
        report: CombineReport {
            target: "disk.iso".into(),
            parts: vec!["disk.001".into()],
            size: 10,
            crc32: "0".into(),
            crc_checked: true,
        },
    };
    let Some(Shown::Status(line)) = shown(&job(kind, combined)) else {
        panic!("a status line");
    };
    assert!(line.contains("disk.iso"), "{line}");
}
