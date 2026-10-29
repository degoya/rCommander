//! The judge of `viewer_empty_source`, fed the answers a working and a broken viewer give.

use rc_plugin_types::{Failure, FailureKind};

use super::judge_empty_source;
use crate::failure::codes;

#[test]
fn an_answer_or_a_refusal_passes() {
    assert_eq!(judge_empty_source("open", Ok(())), Ok(()));
    for refusal in [
        Failure::new(FailureKind::BadData),
        Failure::new(FailureKind::NotSupported),
        Failure::coded(
            FailureKind::BadData,
            "019d0000-0000-7000-8000-0000000000a1.empty",
        ),
        Failure::coded(FailureKind::Cancelled, codes::PLUGIN_CANCELLED),
    ] {
        assert_eq!(judge_empty_source("open", Err(refusal)), Ok(()));
    }
}

#[test]
fn a_trap_an_exhausted_budget_or_a_broken_answer_fails() {
    for code in [
        codes::PLUGIN_CRASHED,
        codes::PLUGIN_TIMED_OUT,
        codes::PLUGIN_OUT_OF_FUEL,
        codes::PLUGIN_OUT_OF_MEMORY,
        codes::PLUGIN_BAD_OUTPUT,
    ] {
        let failure = Failure::coded(FailureKind::Other, code).with_param("reason", "unreachable");
        let refused = judge_empty_source("text", Err(failure)).expect_err(code);
        assert!(refused.contains(code), "{refused}");
        assert!(refused.contains("`text`"), "{refused}");
        assert!(refused.contains("(unreachable)"), "{refused}");
    }
}
