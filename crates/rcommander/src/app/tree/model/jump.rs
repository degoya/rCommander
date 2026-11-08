//! Typed letters in a folder tree (RC-100-88): the cursor jumps to the next folder read whose
//! name starts with them, as Total Commander's tree and the quick search do.

use std::time::Instant;

use super::{DirTree, ROOT, TYPING_PAUSE};

impl DirTree {
    /// Typed `text` at `now`: the cursor jumps to the next folder read whose name starts with
    /// what was typed within [`TYPING_PAUSE`], ignoring case, unfolding the way to it. Only
    /// folders read count - no level is read for a jump. Whether the cursor moved.
    pub fn jump(&mut self, text: &str, now: Instant) -> bool {
        self.target = None;
        let fresh = self
            .typed_at
            .is_none_or(|at| now.saturating_duration_since(at) > TYPING_PAUSE);
        if fresh {
            self.typed.clear();
        }
        self.typed.push_str(text);
        self.typed_at = Some(now);
        let wanted = self.typed.to_lowercase();
        let order = self.read_order();
        let at = self
            .rows
            .get(self.cursor)
            .and_then(|row| order.iter().position(|id| *id == row.id))
            .unwrap_or(0);
        // A first letter looks past the cursor; more letters may still fit the folder there.
        let from = if fresh { at + 1 } else { at };
        let found = order[from.min(order.len())..]
            .iter()
            .chain(&order[..from.min(order.len())])
            .copied()
            .find(|id| {
                self.nodes
                    .get(id)
                    .is_some_and(|node| node.name.to_lowercase().starts_with(&wanted))
            });
        let Some(found) = found else {
            return false;
        };
        let mut above = self.nodes.get(&found).and_then(|node| node.parent);
        while let Some(id) = above {
            let Some(node) = self.nodes.get_mut(&id) else {
                break;
            };
            node.open = true;
            above = node.parent;
        }
        self.rebuild();
        let before = self.cursor;
        self.cursor = self.row_of(found).unwrap_or(self.cursor);
        self.keep_in_view();
        self.cursor != before
    }

    /// Every folder read, in the tree's order, unfolded or not.
    fn read_order(&self) -> Vec<u64> {
        let mut order = Vec::with_capacity(self.nodes.len());
        let mut stack = vec![ROOT];
        while let Some(id) = stack.pop() {
            let Some(node) = self.nodes.get(&id) else {
                continue;
            };
            order.push(id);
            stack.extend(node.children.iter().rev());
        }
        order
    }
}
