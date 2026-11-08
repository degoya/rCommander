//! A photo's EXIF beside the built-in image view (RC-100-99): a JPEG, PNG or WebP with an EXIF
//! block opens as a document of two views, the picture and the EXIF tree, switched with `7` or
//! the list of views as a plugin's outputs are. JPEG, PNG and WebP never reach a viewer plugin
//! (`rc_engine::plugins`, `claims`), so this is the core's own (owner, 2026-11-07).
//!
//! The picture stays the Lister's file, decoded as always ([`Document::with_file_picture`]); the
//! tree is read once, when the file opens, from the EXIF block only (`rc_engine::viewer::exif`),
//! and held in memory - its groups at the top, their fields below. `viewer.exif` off, or a photo
//! without EXIF, opens as before.

use std::sync::Arc;

use rc_db::Db;
use rc_engine::{
    settings::{VIEWER_EXIF_KEY, stored_bool},
    viewer::{
        ImageFormat,
        exif::{ExifGroup, PhotoExif, read_exif},
    },
};

use super::{
    document::{Answer, DocView, Document, DocumentSource, Token, TreeNode},
    load::Opened,
};
use crate::i18n::t;

/// The first id of a field node; a group's id is its index plus one.
const FIELD_IDS: u64 = 1 << 32;

/// The photo's EXIF when the file is a JPEG, PNG or WebP that has one and `viewer.exif` is on
/// (on without a database). Reads only the EXIF block; any failure is no EXIF.
pub async fn read(opened: &Opened, db: Option<&Db>) -> Option<Arc<PhotoExif>> {
    let format = opened.image?;
    if !matches!(
        format,
        ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::WebP
    ) {
        return None;
    }
    if let Some(db) = db
        && !stored_bool(db, VIEWER_EXIF_KEY).await
    {
        return None;
    }
    let exif = read_exif(&opened.file, format).await.ok()??;
    Some(Arc::new(exif))
}

/// The document of a photo with `exif`: the picture first, the EXIF tree second.
pub fn document(exif: &PhotoExif) -> Option<Document> {
    let source: Arc<dyn DocumentSource> = Arc::new(ExifSource::new(exif));
    Document::new(vec![DocView::Image, DocView::Exif], source).map(Document::with_file_picture)
}

/// The tree of one photo's EXIF, built once.
#[derive(Debug)]
struct ExifSource {
    /// Each group's node and its fields' nodes, in the groups' order.
    groups: Vec<(TreeNode, Vec<TreeNode>)>,
}

impl ExifSource {
    fn new(exif: &PhotoExif) -> Self {
        let mut groups: Vec<(TreeNode, Vec<TreeNode>)> = Vec::new();
        let mut last: Option<ExifGroup> = None;
        let mut next_field = FIELD_IDS;
        for field in &exif.fields {
            if last != Some(field.group) {
                last = Some(field.group);
                let id = groups.len() as u64 + 1;
                groups.push((
                    TreeNode {
                        id,
                        label: group_label(field.group).to_owned(),
                        value: None,
                        has_children: true,
                    },
                    Vec::new(),
                ));
            }
            if let Some((_, fields)) = groups.last_mut() {
                fields.push(TreeNode {
                    id: next_field,
                    label: field.tag.clone(),
                    value: Some(field.value.clone()),
                    has_children: false,
                });
                next_field += 1;
            }
        }
        for (node, fields) in &mut groups {
            node.value = Some(fields.len().to_string());
        }
        Self { groups }
    }

    fn nodes(&self, node: Option<u64>) -> Vec<TreeNode> {
        match node {
            None => self.groups.iter().map(|(group, _)| group.clone()).collect(),
            Some(id) => self
                .groups
                .iter()
                .find(|(group, _)| group.id == id)
                .map(|(_, fields)| fields.clone())
                .unwrap_or_default(),
        }
    }
}

impl DocumentSource for ExifSource {
    // The document offers no coloured text; nothing asks.
    fn tokens(&self, starts: Vec<u64>) -> Answer<Vec<Vec<Token>>> {
        let lines = vec![Vec::new(); starts.len()];
        Box::pin(async move { Ok(lines) })
    }

    fn children(&self, node: Option<u64>) -> Answer<Vec<TreeNode>> {
        let nodes = self.nodes(node);
        Box::pin(async move { Ok(nodes) })
    }
}

/// A group's name in the tree.
fn group_label(group: ExifGroup) -> &'static str {
    match group {
        ExifGroup::Image => t!("viewer.exif_group.image"),
        ExifGroup::Photo => t!("viewer.exif_group.photo"),
        ExifGroup::Gps => t!("viewer.exif_group.gps"),
        ExifGroup::Interop => t!("viewer.exif_group.interop"),
        ExifGroup::Thumbnail => t!("viewer.exif_group.thumbnail"),
    }
}

#[cfg(test)]
#[path = "exif_tests.rs"]
mod tests;
