use rc_rename::RenameStep;

use super::{ParkedName, parked};

fn step(from: &str, to: &str) -> RenameStep {
    RenameStep {
        from: from.to_owned(),
        to: to.to_owned(),
    }
}

/// A swap as `rc_rename::schedule` makes it: `a` aside, `b` onto `a`, `a` back onto `b`.
fn swap() -> Vec<RenameStep> {
    vec![
        step("a", ".rc-rename-t-0"),
        step("b", "a"),
        step(".rc-rename-t-0", "b"),
    ]
}

#[test]
fn a_swap_stopped_in_its_middle_has_one_entry_parked() {
    let steps = swap();
    assert!(parked(&steps, 0).is_empty(), "nothing done, nothing parked");
    let expected = vec![ParkedName {
        temp: ".rc-rename-t-0".to_owned(),
        old: "a".to_owned(),
        new: "b".to_owned(),
    }];
    assert_eq!(parked(&steps, 1), expected);
    assert_eq!(parked(&steps, 2), expected);
    assert!(parked(&steps, 3).is_empty(), "the swap is complete");
}

#[test]
fn plain_renames_park_nothing() {
    let steps = vec![step("a", "x"), step("b", "y")];
    assert!(parked(&steps, 1).is_empty());
    assert!(
        parked(&steps, 9).is_empty(),
        "a count past the end is the end"
    );
}
