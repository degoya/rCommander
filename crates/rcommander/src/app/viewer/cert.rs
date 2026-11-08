//! Certificates and keys in the built-in viewer (RC-110-08): a certificate or key file opens as
//! a document of two views, its report as text and the same as a tree, switched with `7` or the
//! list of views as a plugin's outputs are. The core's own reader (`rc_engine::viewer::cert`,
//! `rc_cert`) takes every such file, sensitive or not: decision 8 (`rc_mcp::sensitive_path`)
//! hands none of them to a plugin, and stays untouched.
//!
//! The report is read once, when the file opens, and put into words here (`cert_words.rs`) in
//! the interface's language. Both views are held in memory, as the `.env` source's colours are
//! (RC-110-07): the text through `rc_engine::viewer::memory_text`, which the Lister reads in place
//! of the original as it reads a file, and the tree - the checks and each object at the top,
//! their fields below. Nothing is written to a disk, and no private key material is in either.
//! `viewer.certificates` off, or a file that is no certificate or key, opens as before.

use std::{path::Path, sync::Arc};

use rc_db::Db;
use rc_engine::viewer::{ViewMode, cert::read_report, memory_text::open_text};

use super::{
    cert_words::{self, Line},
    document::{Answer, DocView, Document, DocumentSource, Token, TreeNode},
    load::{self, Opened},
};

/// The first id of a field node; a section's id is its index plus one.
const FIELD_IDS: u64 = 1 << 32;
/// The name the report's text has in memory: no extension, so no setting reads it as DOS text.
const REPORT_NAME: &str = "certificate report";

/// `file`, opened as `name`, as its certificate report, when it is a certificate or key file and
/// `viewer.certificates` is on: the report's text opened for the Lister, and the document's
/// source. `None` leaves the file as it is.
pub async fn open(
    name: &Path,
    file: &Opened,
    db: Option<&Db>,
) -> Option<(Opened, Arc<CertSource>)> {
    let report = read_report(&file.file, name, &file.head, db).await?;
    let sections = cert_words::sections(&report);
    let text = open_text(REPORT_NAME, cert_words::text(&sections))
        .await
        .ok()?;
    let mut opened = load::from_window(text, db.cloned()).await.ok()?;
    opened.mode = ViewMode::Text;
    Some((opened, Arc::new(CertSource::new(sections))))
}

/// The document of a certificate report: its text first, the tree on `7`.
pub fn document(source: Arc<CertSource>) -> Option<Document> {
    let source: Arc<dyn DocumentSource> = source;
    Document::new(vec![DocView::Text, DocView::Tree], source)
}

/// The tree of one report, built once.
#[derive(Debug)]
pub struct CertSource {
    /// Each section's node and its fields' nodes, in the report's order.
    sections: Vec<(TreeNode, Vec<TreeNode>)>,
}

impl CertSource {
    fn new(sections: Vec<cert_words::Section>) -> Self {
        let mut next_field = FIELD_IDS;
        let sections = sections
            .into_iter()
            .zip(1u64..)
            .map(|(section, id)| {
                let fields = section
                    .lines
                    .into_iter()
                    .map(|Line { label, value }| {
                        let node = TreeNode {
                            id: next_field,
                            label,
                            value: Some(value),
                            has_children: false,
                        };
                        next_field += 1;
                        node
                    })
                    .collect::<Vec<_>>();
                let node = TreeNode {
                    id,
                    label: section.title,
                    value: section.value,
                    has_children: !fields.is_empty(),
                };
                (node, fields)
            })
            .collect();
        Self { sections }
    }

    fn nodes(&self, node: Option<u64>) -> Vec<TreeNode> {
        match node {
            None => self
                .sections
                .iter()
                .map(|(section, _)| section.clone())
                .collect(),
            Some(id) => self
                .sections
                .iter()
                .find(|(section, _)| section.id == id)
                .map(|(_, fields)| fields.clone())
                .unwrap_or_default(),
        }
    }
}

impl DocumentSource for CertSource {
    // The report offers no coloured text; nothing asks.
    fn tokens(&self, starts: Vec<u64>) -> Answer<Vec<Vec<Token>>> {
        let lines = vec![Vec::new(); starts.len()];
        Box::pin(async move { Ok(lines) })
    }

    fn children(&self, node: Option<u64>) -> Answer<Vec<TreeNode>> {
        let nodes = self.nodes(node);
        Box::pin(async move { Ok(nodes) })
    }
}

#[cfg(test)]
#[path = "cert_tests.rs"]
mod tests;
