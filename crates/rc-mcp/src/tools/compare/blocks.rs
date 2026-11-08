//! `files_compare_apply`'s parameters as a `compare.apply_blocks` request (RC-100-94).

use rc_engine::compare::{ApplyBlocksRequest, Direction, FileStamp, TextOptions};
use rmcp::schemars;
use serde::Deserialize;

/// Which way the blocks go.
#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DirectionArg {
    /// The left file's lines replace the right file's.
    LeftToRight,
    /// The right file's lines replace the left file's.
    RightToLeft,
}

impl From<DirectionArg> for Direction {
    fn from(direction: DirectionArg) -> Self {
        match direction {
            DirectionArg::LeftToRight => Self::LeftToRight,
            DirectionArg::RightToLeft => Self::RightToLeft,
        }
    }
}

#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct FilesCompareApplyParams {
    /// The left file: absolute, or relative to the left panel's directory.
    pub left: String,
    /// The right file: absolute, or relative to the right panel's directory.
    pub right: String,
    /// The blocks of differing lines to copy: their places, from 0, among the changes of the
    /// files_compare answer, counted under the same ignore options.
    pub blocks: Vec<usize>,
    /// left_to_right or right_to_left.
    pub direction: DirectionArg,
    /// The ignore options of the files_compare call the blocks were counted in.
    #[serde(default)]
    pub ignore_whitespace: bool,
    #[serde(default)]
    pub ignore_case: bool,
    #[serde(default)]
    pub ignore_line_endings: bool,
    /// left_size of the files_compare answer.
    pub expect_left_size: u64,
    /// left_modified_unix of the files_compare answer; omitted when it had none.
    #[serde(default)]
    pub expect_left_modified_unix: Option<u64>,
    /// right_size of the files_compare answer.
    pub expect_right_size: u64,
    /// right_modified_unix of the files_compare answer; omitted when it had none.
    #[serde(default)]
    pub expect_right_modified_unix: Option<u64>,
}

impl FilesCompareApplyParams {
    pub(crate) fn request(self) -> ApplyBlocksRequest {
        ApplyBlocksRequest {
            left: self.left,
            right: self.right,
            options: TextOptions {
                ignore_whitespace: self.ignore_whitespace,
                ignore_case: self.ignore_case,
                ignore_line_endings: self.ignore_line_endings,
            },
            blocks: self.blocks,
            direction: self.direction.into(),
            expect_left: FileStamp {
                size: self.expect_left_size,
                modified_unix: self.expect_left_modified_unix,
            },
            expect_right: FileStamp {
                size: self.expect_right_size,
                modified_unix: self.expect_right_modified_unix,
            },
        }
    }
}
