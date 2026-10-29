//! A `tree` output as the host keeps it: which nodes the plugin handed out and how deep each
//! lies, so only those are expanded and the tree stays within its bounds (`docs/plugins.md`,
//! "Validation of plugin output": depth and node counts capped).

use std::collections::HashMap;

use rc_plugin_api::viewer::TreeNode;
use rc_plugin_types::{Failure, FailureKind};

use crate::bindings::exports::rcommander::plugin::viewer as wit;
use crate::failure::{codes, readable};

/// Most children one expansion shows; the rest is cut and said to be.
pub const MAX_TREE_CHILDREN: usize = 10_000;
/// Most nodes one document's tree holds over all its expansions.
pub const MAX_TREE_NODES: usize = 100_000;
/// The deepest level expanded.
pub const MAX_TREE_DEPTH: u32 = 64;
/// Longest label and value shown.
pub const MAX_LABEL_CHARS: usize = 1024;
pub const MAX_VALUE_CHARS: usize = 4096;

/// The children of one expansion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TreeChildren {
    pub nodes: Vec<TreeNode>,
    /// Some were dropped: past a cap, or a node handed out before.
    pub truncated: bool,
}

/// The nodes handed out so far, with their depth.
#[derive(Debug, Default)]
pub(crate) struct TreeState {
    depths: HashMap<u64, u32>,
}

impl TreeState {
    /// The depth the children of `node` would lie at; refused for a node never handed out and
    /// below the deepest level.
    pub(crate) fn depth_below(&self, node: Option<u64>) -> Result<u32, Failure> {
        let Some(node) = node else {
            return Ok(0);
        };
        let parent = self
            .depths
            .get(&node)
            .ok_or_else(|| Failure::coded(FailureKind::NotFound, codes::PLUGIN_UNKNOWN_NODE))?;
        let depth = parent + 1;
        if depth > MAX_TREE_DEPTH {
            return Err(Failure::coded(
                FailureKind::Other,
                codes::PLUGIN_TREE_TOO_DEEP,
            ));
        }
        Ok(depth)
    }

    /// Takes the children a plugin answered at `depth`: at most [`MAX_TREE_CHILDREN`], within
    /// [`MAX_TREE_NODES`] for the document, no id twice, labels and values made readable.
    pub(crate) fn accept(&mut self, depth: u32, answered: Vec<wit::TreeNode>) -> TreeChildren {
        let room = MAX_TREE_NODES
            .saturating_sub(self.depths.len())
            .min(MAX_TREE_CHILDREN);
        let offered = answered.len();
        let mut nodes = Vec::with_capacity(offered.min(room));
        for node in answered {
            if nodes.len() == room {
                break;
            }
            if self.depths.contains_key(&node.id) {
                continue;
            }
            self.depths.insert(node.id, depth);
            nodes.push(TreeNode {
                id: node.id,
                label: readable(&node.label, MAX_LABEL_CHARS),
                value: node.value.map(|value| readable(&value, MAX_VALUE_CHARS)),
                has_children: node.has_children && depth < MAX_TREE_DEPTH,
            });
        }
        TreeChildren {
            truncated: nodes.len() < offered,
            nodes,
        }
    }
}

#[cfg(test)]
#[path = "tree_tests.rs"]
mod tests;
