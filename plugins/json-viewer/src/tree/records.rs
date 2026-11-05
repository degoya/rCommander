//! The lines of a JSON Lines file as the tree's list (RC-100-80): `[i]` for the i-th value, a
//! broken line in its place with its error, blank lines left out.

use super::{anchor, node};
use crate::structure::Structure;
use crate::types::{Failure, Source, TreeNode};
use crate::walk::Walker;

/// Values `lo..hi` of the list, at most a page: from the nearest anchor at or before `lo`, or
/// from the text's start.
pub(super) fn items<S: Source>(
    src: &mut S,
    structure: &Structure,
    lo: u64,
    hi: u64,
) -> Result<Vec<TreeNode>, Failure> {
    let Some(list) = &structure.records else {
        return Ok(Vec::new());
    };
    let (offset, mut index) = anchor(list, lo).unwrap_or((list.start, 0));
    let mut walker = Walker::new(src, structure, offset);
    let mut nodes = Vec::new();
    while index < hi {
        if walker.peek()?.is_none() {
            break;
        }
        let label = format!("[{index}]");
        let at = walker.offset();
        if let Some(broken) = structure.broken_at(at) {
            if index >= lo {
                let error = &broken.error;
                nodes.push(TreeNode {
                    id: at,
                    label,
                    value: Some(format!(
                        "invalid: line {}, column {}: {}",
                        error.line, error.column, error.message
                    )),
                    has_children: false,
                });
            }
            if !walker.skip_line()? {
                break;
            }
        } else {
            let Some((start, shape)) = walker.value()? else {
                break;
            };
            if index >= lo {
                nodes.push(node(start, label, shape));
            }
        }
        index += 1;
    }
    Ok(nodes)
}
