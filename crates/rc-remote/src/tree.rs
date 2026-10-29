//! Whole trees over single commands, alike for FTP and SFTP (RC-080-48): creating a path's
//! missing directories and removing a directory with everything below it, the latter to
//! [`MAX_TREE_DEPTH`] levels - a server that lists a directory inside every directory cannot
//! keep the removal going for ever.

use std::{future::Future, path::Path, pin::Pin};

use rc_vfs::{Result, VfsError};

/// How many levels below the directory given a removal descends.
pub(crate) const MAX_TREE_DEPTH: usize = 256;

type Boxed<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

/// The single commands of a backend the tree helpers are built from.
pub(crate) trait TreeOps: Sync {
    /// `Some(true)` for a directory (or a link to one), `Some(false)` for anything else, `None`
    /// when `path` is missing.
    fn kind_of(&self, path: &Path) -> impl Future<Output = Result<Option<bool>>> + Send;

    /// Creates the one directory `path`.
    fn make_one_dir(&self, path: &Path) -> impl Future<Output = Result<()>> + Send;

    /// The names in `dir`, each with whether the listing calls it a directory - a link is not.
    fn entries_of(&self, dir: &Path) -> impl Future<Output = Result<Vec<(String, bool)>>> + Send;

    /// Removes the file (or link) `path`.
    fn remove_one(&self, path: &Path) -> impl Future<Output = Result<()>> + Send;

    /// Removes the empty directory `path`.
    fn remove_empty(&self, path: &Path) -> impl Future<Output = Result<()>> + Send;
}

/// Creates the missing directories of `path` from the top down; an existing directory is fine,
/// anything else in the way is not.
pub(crate) async fn mkdir_all<T: TreeOps>(ops: &T, path: &Path) -> Result<()> {
    let mut missing = Vec::new();
    let mut current = path;
    loop {
        match ops.kind_of(current).await? {
            Some(true) => break,
            Some(false) if current == path => {
                return Err(VfsError::AlreadyExists {
                    path: path.to_path_buf(),
                });
            }
            Some(false) => {
                return Err(VfsError::NotADirectory {
                    path: current.to_path_buf(),
                });
            }
            None => {
                missing.push(current);
                match current.parent() {
                    Some(parent) => current = parent,
                    None => break,
                }
            }
        }
    }
    for dir in missing.into_iter().rev() {
        ops.make_one_dir(dir).await?;
    }
    Ok(())
}

/// Removes `dir` and everything below it. Links are removed, never followed: only an entry the
/// listing calls a directory is descended into, at most [`MAX_TREE_DEPTH`] levels deep.
pub(crate) async fn remove_tree<T: TreeOps>(ops: &T, dir: &Path) -> Result<()> {
    remove_below(ops, dir, 0).await
}

fn remove_below<'a, T: TreeOps>(ops: &'a T, dir: &'a Path, depth: usize) -> Boxed<'a, ()> {
    Box::pin(async move {
        if depth > MAX_TREE_DEPTH {
            return Err(VfsError::Unsupported {
                path: dir.to_path_buf(),
                operation: "removing a tree deeper than 256 levels",
            });
        }
        for (name, is_dir) in ops.entries_of(dir).await? {
            let child = dir.join(&name);
            if is_dir {
                remove_below(ops, &child, depth + 1).await?;
            } else {
                ops.remove_one(&child).await?;
            }
        }
        ops.remove_empty(dir).await
    })
}

#[cfg(test)]
#[path = "tree_tests.rs"]
mod tests;
