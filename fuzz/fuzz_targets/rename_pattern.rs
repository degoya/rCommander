//! A multi-rename rule as the window or an MCP call hands it to the engine (`rc_rename::plan`):
//! Total Commander's name and extension masks, the case, a search and replace (plain, with
//! wildcards or a regular expression), the counter and the clipboard of `[X]`, applied to a
//! folder of a few entries on Windows', Linux's or macOS's naming rules.
//!
//! The input: four bytes of switches - case and the search's flags, the file system, whether
//! there is a search and a clipboard, the counter's width, start and step - then the texts, one
//! per line: name mask, extension mask, search, replacement, clipboard, date format.
//!
//! It is an error or the plan, never a panic. A plan has one item per entry; one without
//! conflicts schedules, and its steps carried out on the folder's names in order rename only
//! names that are there, never onto one that is, and leave exactly the planned names.

#![no_main]

use std::collections::HashSet;

use jiff::civil::datetime;
use libfuzzer_sys::fuzz_target;
use rc_rename::{
    CaseMode, Change, CounterSettings, FsRules, PlanInput, RenameContext, RenameRule, RenameSource,
    SearchReplace, plan, schedule,
};

/// The folder's entries: the shapes the masks treat differently - an extension, two, none, a
/// leading dot, spaces, upper case, letters outside ASCII, a folder.
const NAMES: [&str; 8] = [
    "report.txt",
    "Photo 01.JPG",
    "archive.tar.gz",
    ".hidden",
    "no_extension",
    "\u{dc}ber m\u{e4}\u{df}ig.md",
    "README",
    "folder",
];

/// The case modes, by the low bits of the first switch byte.
const CASES: [CaseMode; 6] = [
    CaseMode::Unchanged,
    CaseMode::Lower,
    CaseMode::Upper,
    CaseMode::FirstUpper,
    CaseMode::WordsUpper,
    CaseMode::Title,
];

fn sources() -> Vec<RenameSource> {
    NAMES
        .iter()
        .enumerate()
        .map(|(index, name)| RenameSource {
            name: (*name).to_owned(),
            modified: Some(datetime(2026, 11, 25, 10, 30, 0, 0)),
            size: Some(1024 * index as u64),
            is_dir: *name == "folder",
        })
        .collect()
}

fuzz_target!(|data: &[u8]| {
    // `flags`: the case (bits 0-2), then regex, case-sensitive, first only, with the extension,
    // substitute. `switches`: the file system (bits 0-1), a search (2), a clipboard (3), the
    // counter's width (4-7).
    let Some((&[flags, switches, start, step], text)) = data.split_first_chunk::<4>() else {
        return;
    };
    let text = String::from_utf8_lossy(text);
    let mut fields = text.split('\n').map(str::to_owned);
    let mut field = |default: &str| fields.next().unwrap_or_else(|| default.to_owned());
    let name_mask = field("[N]");
    let extension_mask = field("[E]");
    let find = field("");
    let replace = field("");
    let clipboard = field("");
    let date_format = field("");

    let search = (switches & 0b100 != 0).then(|| SearchReplace {
        find,
        replace,
        regex: flags & 0b0000_1000 != 0,
        case_sensitive: flags & 0b0001_0000 != 0,
        first_only: flags & 0b0010_0000 != 0,
        include_extension: flags & 0b0100_0000 != 0,
        substitute: flags & 0b1000_0000 != 0,
    });
    let rule = RenameRule {
        name_mask,
        extension_mask,
        case: CASES[usize::from(flags & 0b111) % CASES.len()],
        search,
        counter: CounterSettings {
            start: i64::from(start as i8),
            step: i64::from(step as i8),
            width: u32::from(switches >> 4),
        },
        names: None,
    };
    let fs = match switches & 0b11 {
        0 => FsRules::WINDOWS,
        1 => FsRules::UNIX,
        _ => FsRules::MACOS,
    };
    let context = RenameContext {
        clipboard: (switches & 0b1000 != 0).then_some(clipboard),
        opened_at: Some(datetime(2026, 11, 25, 12, 0, 0, 0)),
        changed_at: Some(datetime(2026, 11, 25, 12, 5, 0, 0)),
        date_format: (!date_format.is_empty()).then_some(date_format),
        time_format: None,
    };
    let sources = sources();
    let mut existing: Vec<String> = NAMES.iter().map(|name| (*name).to_owned()).collect();
    existing.push("taken.txt".to_owned());
    let input = PlanInput {
        dir: "/home/fuzz/folder",
        sources: &sources,
        existing: &existing,
        fs,
        context: &context,
    };
    let Ok(plan) = plan(&input, &rule) else {
        return;
    };
    assert_eq!(plan.items.len(), sources.len(), "one item per entry");
    if plan.has_conflicts() {
        return;
    }
    let schedule = schedule(&plan, "fuzz").expect("a plan without conflicts schedules");

    let mut names: HashSet<String> = existing.iter().map(|name| fs.key(name)).collect();
    for step in &schedule.steps {
        assert!(
            names.remove(&fs.key(&step.from)),
            "{} is renamed but is not there",
            step.from
        );
        assert!(
            names.insert(fs.key(&step.to)),
            "{} -> {} overwrites an entry",
            step.from,
            step.to
        );
    }
    let mut expected: HashSet<String> = existing.iter().map(|name| fs.key(name)).collect();
    let changed = plan
        .items
        .iter()
        .filter(|item| item.change != Change::Unchanged);
    for item in changed.clone() {
        expected.remove(&fs.key(&item.old));
    }
    for item in changed {
        expected.insert(fs.key(&item.new));
    }
    assert_eq!(names, expected, "the run does not leave the planned names");
});
