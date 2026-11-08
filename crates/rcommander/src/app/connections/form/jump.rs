//! The jump host in a connection's form (RC-110-03): the SSH connections it can go through -
//! never itself, nor one whose own chain leads through it - and the chain it then goes through,
//! as text under the field. The repository checks the same again when the form is saved.

use rc_engine::{Connection, ConnectionId, ConnectionKind, ConnectionTree, FolderNode, MAX_JUMPS};

/// Every connection of `tree`: the root's, then folder by folder.
fn all(tree: &ConnectionTree) -> Vec<&Connection> {
    fn gather<'a>(nodes: &'a [FolderNode], found: &mut Vec<&'a Connection>) {
        for node in nodes {
            found.extend(&node.connections);
            gather(&node.folders, found);
        }
    }
    let mut found: Vec<&Connection> = tree.connections.iter().collect();
    gather(&tree.folders, &mut found);
    found
}

/// The jump hosts of `start`'s chain, `start` first, as far as the tree knows them.
fn chain<'a>(all: &[&'a Connection], start: Option<ConnectionId>) -> Vec<&'a Connection> {
    let mut hops = Vec::new();
    let mut next = start;
    // One past the longest chain: a table changed behind the repository ends here too.
    while let Some(id) = next.filter(|_| hops.len() <= MAX_JUMPS) {
        let Some(hop) = all.iter().find(|connection| connection.id == id) else {
            break;
        };
        hops.push(*hop);
        next = hop.jump_id;
    }
    hops
}

/// The SSH connections `own` (`None` for a new one) can go through, by name.
pub fn jump_choices(
    tree: &ConnectionTree,
    own: Option<ConnectionId>,
) -> Vec<(ConnectionId, String)> {
    let all = all(tree);
    let mut choices: Vec<(ConnectionId, String)> = all
        .iter()
        .filter(|candidate| candidate.kind == ConnectionKind::Ssh && Some(candidate.id) != own)
        .filter(|candidate| {
            !chain(&all, candidate.jump_id)
                .iter()
                .any(|hop| Some(hop.id) == own)
        })
        .map(|candidate| (candidate.id, candidate.name.clone()))
        .collect();
    choices.sort_by_key(|(_, name)| name.to_lowercase());
    choices
}

/// The chain through `jump`, the host connected to first at the front: `bastion -> inner`;
/// `None` without a jump host.
pub fn chain_text(tree: &ConnectionTree, jump: Option<ConnectionId>) -> Option<String> {
    let all = all(tree);
    let mut names: Vec<&str> = chain(&all, jump)
        .iter()
        .map(|hop| hop.name.as_str())
        .collect();
    if names.is_empty() {
        return None;
    }
    names.reverse();
    Some(names.join(" \u{2192} "))
}

#[cfg(test)]
#[path = "jump_tests.rs"]
mod tests;
