//! The output folder of `--screenshots`: every file written is `<dir>/<scene>.png` or the run's
//! manifest `<dir>/screenshots.json`, checked to lie directly in the folder - never a path out of
//! it, never through a link planted at the name.

use std::{
    fs::{self, File, OpenOptions},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail, ensure};
use image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder};
use serde::Serialize;

use super::scenes::check_name;

/// What the run leaves beside the pictures, and what `scripts/screenshots.sh` waits for.
pub const MANIFEST: &str = "screenshots.json";

/// The output folder, made and resolved.
#[derive(Debug, Clone)]
pub struct OutDir {
    root: PathBuf,
}

impl OutDir {
    /// Makes `dir` if it is missing and resolves it.
    ///
    /// # Errors
    ///
    /// `dir` is relative, cannot be made, or is no folder.
    pub fn prepare(dir: &Path) -> Result<Self> {
        ensure!(
            dir.is_absolute(),
            "the output folder {} is not absolute",
            dir.display()
        );
        fs::create_dir_all(dir)
            .with_context(|| format!("the output folder {} cannot be made", dir.display()))?;
        let root = fs::canonicalize(dir)
            .with_context(|| format!("the output folder {} does not resolve", dir.display()))?;
        ensure!(root.is_dir(), "{} is no folder", root.display());
        Ok(Self { root })
    }

    #[cfg(test)]
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Where the picture of scene `name` goes.
    ///
    /// # Errors
    ///
    /// The name is no scene name, or the path would leave the folder.
    pub fn shot(&self, name: &str) -> Result<PathBuf> {
        check_name(name)?;
        self.inside(&format!("{name}.png"))
    }

    /// Where the manifest goes.
    ///
    /// # Errors
    ///
    /// The path would leave the folder (it cannot).
    pub fn manifest(&self) -> Result<PathBuf> {
        self.inside(MANIFEST)
    }

    /// `file` directly in the folder, and not a link or a folder already there.
    fn inside(&self, file: &str) -> Result<PathBuf> {
        let path = self.root.join(file);
        ensure!(
            path.parent() == Some(self.root.as_path())
                && path.file_name().is_some_and(|name| name == file),
            "{file} is outside the output folder"
        );
        if let Ok(meta) = fs::symlink_metadata(&path)
            && !meta.file_type().is_file()
        {
            bail!("{} is there already and no plain file", path.display());
        }
        Ok(path)
    }
}

/// Writes `rgba` (`width` x `height`, 8 bits each) as a PNG to `path`, replacing a file there.
///
/// # Errors
///
/// The size and the bytes do not match, or the file cannot be written.
pub fn write_png(path: &Path, rgba: &[u8], width: u32, height: u32) -> Result<()> {
    let expected = u64::from(width) * u64::from(height) * 4;
    ensure!(
        width > 0 && height > 0 && rgba.len() as u64 == expected,
        "a picture of {width}x{height} has {} bytes, not {expected}",
        rgba.len()
    );
    let mut file = BufWriter::new(create(path)?);
    PngEncoder::new(&mut file)
        .write_image(rgba, width, height, ExtendedColorType::Rgba8)
        .with_context(|| format!("{} cannot be encoded", path.display()))?;
    file.flush()
        .with_context(|| format!("{} cannot be written", path.display()))
}

/// One picture taken.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Taken {
    pub name: String,
    pub file: String,
    /// In physical pixels: the logical size times the display's scale.
    pub width: u32,
    pub height: u32,
}

/// The run's outcome, written once at its end.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Manifest {
    pub version: &'static str,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub shots: Vec<Taken>,
}

/// Writes `manifest` to `path`.
///
/// # Errors
///
/// The file cannot be written.
pub fn write_manifest(path: &Path, manifest: &Manifest) -> Result<()> {
    let text = serde_json::to_vec_pretty(manifest).context("the manifest is not written")?;
    let mut file = create(path)?;
    file.write_all(&text)
        .and_then(|()| file.flush())
        .with_context(|| format!("{} cannot be written", path.display()))
}

fn create(path: &Path) -> Result<File> {
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .with_context(|| format!("{} cannot be created", path.display()))
}

#[cfg(test)]
#[path = "out_tests.rs"]
mod tests;
