use std::collections::{HashMap, HashSet};

use super::{RenameStep, UndoMismatch, UndoProblem, check_undo, schedule, undo_steps};
use crate::error::RenameError;
use crate::names::FsRules;
use crate::plan::{Change, RenameItem, RenamePlan};

/// A directory in memory: each entry's name and content (its original name). A rename onto a
/// name another entry holds fails, as `Vfs::rename` without overwrite does.
#[derive(Debug, Clone)]
struct Dir {
    fs: FsRules,
    entries: Vec<(String, String)>,
}

impl Dir {
    fn new(fs: FsRules, names: &[&str]) -> Self {
        Self {
            fs,
            entries: names
                .iter()
                .map(|name| ((*name).to_owned(), (*name).to_owned()))
                .collect(),
        }
    }

    fn rename(&mut self, step: &RenameStep) -> Result<(), String> {
        let from = self.fs.key(&step.from);
        let to = self.fs.key(&step.to);
        let at = self
            .entries
            .iter()
            .position(|(name, _)| self.fs.key(name) == from)
            .ok_or_else(|| format!("{} is missing", step.from))?;
        if to != from && self.entries.iter().any(|(name, _)| self.fs.key(name) == to) {
            return Err(format!("{} would overwrite {}", step.from, step.to));
        }
        self.entries[at].0.clone_from(&step.to);
        Ok(())
    }

    fn run(&mut self, steps: &[RenameStep]) {
        for step in steps {
            self.rename(step).unwrap_or_else(|error| panic!("{error}"));
        }
    }

    /// Name to content.
    fn contents(&self) -> HashMap<String, String> {
        self.entries.iter().cloned().collect()
    }

    fn names(&self) -> Vec<String> {
        self.entries.iter().map(|(name, _)| name.clone()).collect()
    }
}

/// A plan of `pairs` (old, new) in a directory that also holds `others`, as `plan` makes one.
fn plan_of(fs: FsRules, pairs: &[(&str, &str)], others: &[&str]) -> RenamePlan {
    let items = pairs
        .iter()
        .map(|(old, new)| RenameItem {
            old: (*old).to_owned(),
            new: (*new).to_owned(),
            change: if old == new {
                Change::Unchanged
            } else if fs.case_insensitive && fs.key(old) == fs.key(new) {
                Change::CaseOnly
            } else {
                Change::Rename
            },
            conflict: None,
        })
        .collect();
    let taken: HashSet<String> = pairs
        .iter()
        .map(|(old, _)| *old)
        .chain(others.iter().copied())
        .map(|name| fs.key(name))
        .collect();
    RenamePlan {
        dir: "/d".to_owned(),
        fs,
        items,
        taken,
    }
}

/// Carries `pairs` out in a directory with `others`; checks that nothing is overwritten, every
/// entry ends under its new name, no temporary name is left and every cancelled prefix is
/// undone completely. Returns the steps.
fn carried_out(fs: FsRules, pairs: &[(&str, &str)], others: &[&str]) -> Vec<RenameStep> {
    let plan = plan_of(fs, pairs, others);
    let schedule = schedule(&plan, "t").expect("schedules");
    let names: Vec<&str> = pairs
        .iter()
        .map(|(old, _)| *old)
        .chain(others.iter().copied())
        .collect();
    let before = Dir::new(fs, &names);

    let mut after = before.clone();
    after.run(&schedule.steps);
    let mut expected: HashMap<String, String> = before.contents();
    let moved: Vec<(&str, String)> = pairs
        .iter()
        .map(|(old, new)| (*new, expected.remove(*old).expect("listed")))
        .collect();
    for (new, content) in moved {
        expected.insert(new.to_owned(), content);
    }
    assert_eq!(after.contents(), expected, "{pairs:?}");

    for done in 0..=schedule.steps.len() {
        let mut dir = before.clone();
        dir.run(&schedule.steps[..done]);
        let undo = schedule.undo(done);
        assert!(
            check_undo(&undo, &dir.names(), &fs).is_empty(),
            "{pairs:?} after {done}"
        );
        dir.run(&undo);
        assert_eq!(
            dir.contents(),
            before.contents(),
            "{pairs:?} undone after {done}"
        );
    }
    schedule.steps
}

fn step(from: &str, to: &str) -> RenameStep {
    RenameStep {
        from: from.to_owned(),
        to: to.to_owned(),
    }
}

#[test]
fn a_swap_goes_through_one_temporary_name() {
    for fs in [FsRules::WINDOWS, FsRules::UNIX] {
        let steps = carried_out(fs, &[("a", "b"), ("b", "a")], &[]);
        assert_eq!(
            steps,
            [
                step("a", ".rc-rename-t-1"),
                step("b", "a"),
                step(".rc-rename-t-1", "b")
            ]
        );
    }
}

#[test]
fn a_ring_goes_through_one_temporary_name() {
    let steps = carried_out(FsRules::UNIX, &[("a", "b"), ("b", "c"), ("c", "a")], &["x"]);
    assert_eq!(steps.len(), 4);
    assert_eq!(steps[0], step("a", ".rc-rename-t-1"));
}

#[test]
fn a_chain_needs_no_temporary_name_and_renames_the_far_end_first() {
    let steps = carried_out(FsRules::UNIX, &[("1", "2"), ("2", "3"), ("3", "4")], &[]);
    assert_eq!(steps, [step("3", "4"), step("2", "3"), step("1", "2")]);
}

#[test]
fn a_case_only_change_goes_through_a_temporary_name_where_case_is_ignored() {
    let steps = carried_out(FsRules::WINDOWS, &[("readme.TXT", "README.txt")], &[]);
    assert_eq!(
        steps,
        [
            step("readme.TXT", ".rc-rename-t-1"),
            step(".rc-rename-t-1", "README.txt")
        ]
    );
    let steps = carried_out(FsRules::UNIX, &[("readme.TXT", "README.txt")], &[]);
    assert_eq!(steps, [step("readme.TXT", "README.txt")]);
}

#[test]
fn a_case_swap_on_a_case_insensitive_file_system_is_safe() {
    carried_out(
        FsRules::WINDOWS,
        &[("a", "B"), ("b", "A"), ("c", "C")],
        &["d"],
    );
}

#[test]
fn every_permutation_of_four_names_is_carried_out_and_undone() {
    let names = ["n1", "n2", "n3", "n4"];
    let mut permutations = Vec::new();
    permute(&mut names.to_vec(), 0, &mut permutations);
    assert_eq!(permutations.len(), 24);
    for targets in &permutations {
        let pairs: Vec<(&str, &str)> = names.iter().copied().zip(targets.iter().copied()).collect();
        for fs in [FsRules::WINDOWS, FsRules::UNIX] {
            carried_out(fs, &pairs, &["other"]);
        }
    }
}

fn permute<'a>(items: &mut Vec<&'a str>, from: usize, out: &mut Vec<Vec<&'a str>>) {
    if from == items.len() {
        out.push(items.clone());
        return;
    }
    for index in from..items.len() {
        items.swap(from, index);
        permute(items, from + 1, out);
        items.swap(from, index);
    }
}

#[test]
fn temporary_names_avoid_every_name_in_use() {
    let steps = carried_out(
        FsRules::WINDOWS,
        &[("a", "b"), ("b", "a")],
        &[".RC-RENAME-T-1", ".rc-rename-t-2x"],
    );
    assert_eq!(steps[0], step("a", ".rc-rename-t-2"));

    let plan = plan_of(FsRules::UNIX, &[("a", "b"), ("b", "a")], &[]);
    let untagged = schedule(&plan, "").expect("schedules");
    assert_eq!(untagged.steps[0], step("a", ".rc-rename-1"));
}

#[test]
fn unchanged_entries_have_no_step() {
    let steps = carried_out(FsRules::UNIX, &[("a", "a"), ("b", "c")], &[]);
    assert_eq!(steps, [step("b", "c")]);
}

#[test]
fn a_plan_with_conflicts_is_not_scheduled() {
    let mut plan = plan_of(FsRules::UNIX, &[("a", "x"), ("b", "x")], &[]);
    for (other, item) in [1, 0].into_iter().zip(&mut plan.items) {
        item.conflict = Some(crate::plan::Conflict::Duplicate { other });
    }
    assert_eq!(
        schedule(&plan, "t"),
        Err(RenameError::Conflicts { count: 2 })
    );
}

#[test]
fn undo_steps_reverse_each_step_last_first() {
    let done = [step("a", "t"), step("b", "a"), step("t", "b")];
    assert_eq!(
        undo_steps(&done),
        [step("b", "t"), step("a", "b"), step("t", "a")]
    );
    assert!(undo_steps(&[]).is_empty());
}

#[test]
fn an_undo_reports_the_steps_that_no_longer_fit() {
    // The run renamed `a -> x` and `b -> y`; since then `x` was deleted and a new `b` appeared.
    let undo = undo_steps(&[step("a", "x"), step("b", "y")]);
    let present = ["y".to_owned(), "b".to_owned()];
    assert_eq!(
        check_undo(&undo, &present, &FsRules::UNIX),
        [
            UndoMismatch {
                index: 0,
                step: step("y", "b"),
                problem: UndoProblem::TargetOccupied,
            },
            UndoMismatch {
                index: 1,
                step: step("x", "a"),
                problem: UndoProblem::SourceMissing,
            },
        ]
    );
    // Case is ignored where the file system ignores it.
    let undo = [step("X", "a")];
    assert!(check_undo(&undo, &["x".to_owned()], &FsRules::WINDOWS).is_empty());
    assert_eq!(
        check_undo(&undo, &["x".to_owned()], &FsRules::UNIX).len(),
        1
    );
}
