//! The `tree` output: the document properties at the top, each a leaf with its value, then for
//! a workbook a `sheets` node with each sheet's rows and columns (`rows 3, columns 2`), and for
//! a presentation a `slides` node with each slide's title. Built once, at the first `children`
//! call: a sheet's size reads the start of its part, a slide's title the slide.

use crate::out::Discard;
use crate::package::Package;
use crate::sheet;
use crate::slides;
use crate::text::Section;
use crate::types::{Failure, FailureKind, Source, TreeNode};

/// What the document is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Word,
    Excel,
    PowerPoint,
}

/// Every node, each with its parent.
#[derive(Debug, Default)]
pub(crate) struct Tree {
    nodes: Vec<(Option<u64>, TreeNode)>,
}

impl Tree {
    /// The children of `parent`, the top level for `None`.
    pub(crate) fn children(&self, parent: Option<u64>) -> Result<Vec<TreeNode>, Failure> {
        if let Some(id) = parent
            && !self.nodes.iter().any(|(_, node)| node.id == id)
        {
            return Err(Failure::of(FailureKind::NotFound));
        }
        Ok(self
            .nodes
            .iter()
            .filter(|(of, _)| *of == parent)
            .map(|(_, node)| node.clone())
            .collect())
    }

    fn add(
        &mut self,
        parent: Option<u64>,
        label: String,
        value: Option<String>,
        open: bool,
    ) -> u64 {
        let id = self.nodes.len() as u64 + 1;
        self.nodes.push((
            parent,
            TreeNode {
                id,
                label,
                value,
                has_children: open,
            },
        ));
        id
    }
}

/// Builds the tree of a document.
pub(crate) fn build<S: Source>(
    package: &mut Package<S>,
    kind: Kind,
    properties: &[(&'static str, String)],
    sections: &[Section],
) -> Result<Tree, Failure> {
    let mut tree = Tree::default();
    for (label, value) in properties {
        tree.add(None, (*label).to_owned(), Some(value.clone()), false);
    }
    match kind {
        Kind::Word => {}
        Kind::Excel => {
            let group = tree.add(
                None,
                "sheets".to_owned(),
                Some(sections.len().to_string()),
                !sections.is_empty(),
            );
            for section in sections {
                let size = match section.part.as_deref() {
                    Some(part) => match package.xml(part)? {
                        Some(mut xml) => sheet::size(&mut xml)?,
                        None => None,
                    },
                    None => None,
                };
                let value =
                    size.map(|size| format!("rows {}, columns {}", size.rows, size.columns));
                tree.add(Some(group), section.name.clone(), value, false);
            }
        }
        Kind::PowerPoint => {
            let group = tree.add(
                None,
                "slides".to_owned(),
                Some(sections.len().to_string()),
                !sections.is_empty(),
            );
            for (index, section) in sections.iter().enumerate() {
                let title = match section.part.as_deref() {
                    Some(part) => match package.xml(part)? {
                        Some(mut xml) => slides::slide(&mut xml, &mut Discard)?,
                        None => None,
                    },
                    None => None,
                };
                tree.add(Some(group), format!("slide {}", index + 1), title, false);
            }
        }
    }
    Ok(tree)
}
