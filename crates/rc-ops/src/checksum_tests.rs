use rc_checksum::{Algorithm, FileChecksum, VerifyEntry, VerifySummary};

use super::*;

fn line(line: usize, status: VerifyStatus) -> VerifyEntry {
    VerifyEntry {
        line,
        name: format!("f{line}"),
        status,
        algorithm: Some(Algorithm::Crc32),
        expected: None,
        actual: None,
        code: None,
    }
}

#[test]
fn a_long_check_keeps_every_bad_line_first_and_counts_the_rest() {
    let entries: Vec<VerifyEntry> = (1..=MAX_REPORT_LINES + 5)
        .map(|number| {
            let status = if number % 1000 == 0 {
                VerifyStatus::Mismatch
            } else {
                VerifyStatus::Ok
            };
            line(number, status)
        })
        .collect();
    let report = VerifyReport {
        entries,
        summary: VerifySummary {
            ok: MAX_REPORT_LINES - 5,
            mismatch: 10,
            ..VerifySummary::default()
        },
    };
    let JobReport::ChecksumsVerified { report, omitted } = verified(report) else {
        panic!("a check's report");
    };
    assert_eq!(report.entries.len(), MAX_REPORT_LINES);
    assert_eq!(omitted, 5);
    let bad = report
        .entries
        .iter()
        .filter(|entry| entry.status == VerifyStatus::Mismatch)
        .count();
    assert_eq!(bad, 10, "no bad line is dropped");
    assert!(
        report
            .entries
            .windows(2)
            .all(|pair| pair[0].line < pair[1].line),
        "the lines stay in their order"
    );
    assert_eq!(report.summary.mismatch, 10);
}

#[test]
fn a_short_check_and_a_small_create_keep_everything() {
    let report = VerifyReport {
        entries: vec![line(1, VerifyStatus::Ok), line(2, VerifyStatus::Missing)],
        summary: VerifySummary::default(),
    };
    assert_eq!(
        verified(report.clone()),
        JobReport::ChecksumsVerified { report, omitted: 0 }
    );
    let report = CreateReport {
        algorithm: Algorithm::Sha256,
        files: vec![FileChecksum {
            name: "a.iso".into(),
            hex: "00".into(),
        }],
        written: vec!["a.iso.sha256".into()],
    };
    assert_eq!(
        created(report.clone()),
        JobReport::ChecksumsCreated { report, omitted: 0 }
    );
}

#[test]
fn a_large_create_lists_only_the_first_files() {
    let files = (0..MAX_REPORT_LINES + 3)
        .map(|index| FileChecksum {
            name: format!("f{index}"),
            hex: "00".into(),
        })
        .collect();
    let report = CreateReport {
        algorithm: Algorithm::Md5,
        files,
        written: vec!["checksums.md5".into()],
    };
    let JobReport::ChecksumsCreated { report, omitted } = created(report) else {
        panic!("a create report");
    };
    assert_eq!((report.files.len(), omitted), (MAX_REPORT_LINES, 3));
    assert_eq!(report.written, vec!["checksums.md5"]);
}
