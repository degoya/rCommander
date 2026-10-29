//! The `<use>` budget: a few kilobytes of `<use>` elements, each repeating a group of the ones
//! before it ten times, expand into billions of elements - the SVG form of the billion laughs.
//! usvg builds every copy, so the document is counted first, in one pass over its elements, as
//! if each `<use>` were replaced by what it references, and refused beyond the budget.

use std::collections::HashMap;

use roxmltree::{Document, Node, NodeId};

/// The most elements a document may have with its `<use>` references expanded: far beyond a
/// diagram or a map, far below what fills the store's memory.
pub const MAX_EXPANDED_ELEMENTS: u64 = 1 << 20;

const XLINK: &str = "http://www.w3.org/1999/xlink";

enum State {
    /// Entered and not left: a reference back to it is a cycle, which usvg skips; it counts 0.
    Open,
    Done(u64),
}

/// The elements of `doc` with every `<use>` counted as what it references, up to `limit + 1`:
/// a result above `limit` means "too many". Iterative, so a deeply nested document cannot
/// overflow the stack here; each element is visited once.
#[must_use]
pub fn expanded_elements(doc: &Document, limit: u64) -> u64 {
    let ids: HashMap<&str, Node> = doc
        .descendants()
        .filter(Node::is_element)
        .filter_map(|node| node.attribute("id").map(|id| (id, node)))
        .collect();
    let cap = limit.saturating_add(1);
    let root = doc.root_element();
    let mut states: HashMap<NodeId, State> = HashMap::new();
    let mut stack = vec![(root, false)];
    while let Some((node, leaving)) = stack.pop() {
        if leaving {
            let total = dependencies(node, &ids)
                .fold(1u64, |sum, dependency| {
                    sum.saturating_add(match states.get(&dependency.id()) {
                        Some(State::Done(count)) => *count,
                        _ => 0,
                    })
                })
                .min(cap);
            if total >= cap {
                // Every element counted here is reached from the root, which counts at least as
                // many.
                return cap;
            }
            states.insert(node.id(), State::Done(total));
            continue;
        }
        if states.contains_key(&node.id()) {
            continue;
        }
        states.insert(node.id(), State::Open);
        stack.push((node, true));
        for dependency in dependencies(node, &ids) {
            if !states.contains_key(&dependency.id()) {
                stack.push((dependency, false));
            }
        }
    }
    match states.get(&root.id()) {
        Some(State::Done(count)) => *count,
        _ => 0,
    }
}

/// What one element is made of: its child elements and, for a `<use>`, the element its `href`
/// or `xlink:href` names in this document.
fn dependencies<'a, 'input>(
    node: Node<'a, 'input>,
    ids: &HashMap<&'a str, Node<'a, 'input>>,
) -> impl Iterator<Item = Node<'a, 'input>> {
    let target = (node.tag_name().name() == "use")
        .then(|| {
            node.attribute((XLINK, "href"))
                .or_else(|| node.attribute("href"))
        })
        .flatten()
        .and_then(|href| href.strip_prefix('#'))
        .and_then(|id| ids.get(id).copied());
    node.children().filter(Node::is_element).chain(target)
}

#[cfg(test)]
#[path = "budget_tests.rs"]
mod tests;
