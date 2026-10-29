//! The form follows the tree: the connection or shared credential under the cursor, as stored
//! now, unless the form has edits.

use rc_engine::TreeItem;

use super::{ConnectionManager, form::Form};

impl ConnectionManager {
    /// A form without edits shows what is under the cursor as stored now; one with edits keeps
    /// them and is measured against it.
    pub(super) fn refresh_form(&mut self) {
        let dirty = self.form.as_ref().is_some_and(|form| form.dirty);
        if dirty {
            self.rebase_form();
            self.fit_form();
            return;
        }
        let shown = match self.tree.current() {
            Some(TreeItem::Connection(id)) => self
                .tree
                .connection(id)
                .map(|connection| Form::of(connection, self.is_favourite(id))),
            Some(TreeItem::Credential(id)) => self.tree.credential(id).map(Form::of_credential),
            _ => None,
        };
        let new_form = self
            .form
            .as_ref()
            .is_some_and(|form| form.id().is_none() && form.credential_id().is_none());
        self.form = match shown {
            Some(form) => Some(form),
            // A new connection's or credential's form stays while the cursor is on a folder.
            None if new_form => self.form.take(),
            None => None,
        };
        self.fit_form();
    }

    /// The cursor moved to `item`: the form shows it, its edits gone.
    pub(super) fn show(&mut self, item: Option<TreeItem>) {
        if let Some(form) = &mut self.form {
            form.dirty = false;
        }
        if !matches!(
            item,
            Some(TreeItem::Connection(_) | TreeItem::Credential(_))
        ) {
            self.form = None;
        }
        self.refresh_form();
    }

    /// A form with edits measured against what is stored now.
    fn rebase_form(&mut self) {
        let Some(form) = self.form.as_ref() else {
            return;
        };
        if let Some(id) = form.id()
            && let Some(connection) = self.tree.connection(id).cloned()
        {
            let favourite = self.is_favourite(id);
            if let Some(form) = &mut self.form {
                form.rebase(&connection, favourite);
            }
        } else if let Some(id) = form.credential_id()
            && let Some(credential) = self.tree.credential(id).cloned()
            && let Some(form) = &mut self.form
        {
            form.rebase_credential(&credential);
        }
    }

    /// A connection's form offers the shared credentials while the setting is on and one exists.
    pub(super) fn fit_form(&mut self) {
        let offer = self.tree.offer_credentials && !self.tree.credentials().is_empty();
        if let Some(form) = &mut self.form {
            form.offer_shared = offer;
        }
    }
}
