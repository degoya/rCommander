//! A portable installation, its data folder apart from it as rCommander keeps it, and the next
//! version's archive, for the install tests.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use super::{DEFAULT_CONFIRM_TIMEOUT_SECS, Journal, PRE_UPDATE_DIR, Plan, steps, update_dir};
use crate::{install_kind::InstallKind, sign::hex_sha256};

pub(crate) const OLD: &str = "1.0.0";
pub(crate) const NEW: &str = "2.0.0";

pub(crate) struct Fixture {
    pub _root: tempfile::TempDir,
    pub install: PathBuf,
    pub data: PathBuf,
    pub journal: Journal,
}

impl Fixture {
    /// A Windows-style installation updated from the package's zip, version folder and all.
    pub(crate) fn zip() -> Self {
        Self::new("rcommander.exe", "rcommander-2.0.0-windows-x64.zip")
    }

    /// A Linux-style installation updated from a `.tar.gz`.
    pub(crate) fn tar() -> Self {
        Self::new("rcommander", "rcommander-2.0.0-linux-x64.tar.gz")
    }

    fn new(executable: &str, artifact_name: &str) -> Self {
        let root = tempfile::tempdir().expect("tempdir");
        let install = root.path().join("install");
        let data = root.path().join("appdata");
        write(&install.join(executable), "old program");
        write(&install.join("VERSION.txt"), OLD);
        write(&install.join("README.md"), "old readme");
        write(&install.join("notes.txt"), "the person's own file");
        write(&data.join("rcommander.db"), "live database");
        write(&data.join("rcommander.db-wal"), "live journal");
        let copy = update_dir(&data)
            .join(PRE_UPDATE_DIR)
            .join("rcommander-1.0.0.db");
        write(&copy, "database before the update");
        let artifact = root.path().join(artifact_name);
        let folder = "rcommander-2.0.0-windows-x64";
        let files: Vec<(String, &str)> = vec![
            (format!("{folder}/{executable}"), "new program"),
            (format!("{folder}/VERSION.txt"), NEW),
            (format!("{folder}/README.md"), "new readme"),
            (format!("{folder}/LICENSE"), "licence"),
            // Never moved, whatever an archive carries.
            (
                format!("{folder}/data/rcommander.db"),
                "a database from the archive",
            ),
        ];
        if artifact_name.ends_with(".zip") {
            zip_archive(&artifact, &files);
        } else {
            tar_archive(&artifact, &files);
        }
        let bytes = fs::read(&artifact).expect("artifact");
        let plan = Plan {
            kind: InstallKind::Portable,
            from_version: OLD.to_owned(),
            target_version: NEW.to_owned(),
            artifact,
            sha256: hex_sha256(&bytes),
            size: bytes.len() as u64,
            install_dir: install.clone(),
            executable: executable.to_owned(),
            data_dir: data.clone(),
            database: data.join("rcommander.db"),
            database_copy_blake3: Some(steps::blake3_of(&copy).expect("digest")),
            database_copy: Some(copy),
            args: Vec::new(),
            start_new: true,
            confirm_timeout_secs: DEFAULT_CONFIRM_TIMEOUT_SECS,
        };
        let mut journal = Journal::begin(plan);
        journal.write().expect("journal");
        Self {
            _root: root,
            install,
            data,
            journal,
        }
    }

    pub(crate) fn read(&self, relative: &str) -> Option<String> {
        fs::read_to_string(self.install.join(relative)).ok()
    }

    pub(crate) fn stored(&self) -> Journal {
        Journal::read(&self.data)
            .expect("journal")
            .expect("written")
    }

    /// Everything the old version had, and nothing the new one brought.
    pub(crate) fn assert_old(&self) {
        let executable = self.journal.plan.executable.clone();
        assert_eq!(self.read(&executable).as_deref(), Some("old program"));
        assert_eq!(self.read("VERSION.txt").as_deref(), Some(OLD));
        assert_eq!(self.read("README.md").as_deref(), Some("old readme"));
        assert!(self.read("LICENSE").is_none());
        assert!(!self.journal.previous_dir().exists());
        assert!(!self.journal.staged_dir().exists());
        self.assert_untouched();
    }

    /// Everything the new version brought, the old one aside.
    pub(crate) fn assert_new(&self) {
        let executable = self.journal.plan.executable.clone();
        assert_eq!(self.read(&executable).as_deref(), Some("new program"));
        assert_eq!(self.read("VERSION.txt").as_deref(), Some(NEW));
        assert_eq!(self.read("LICENSE").as_deref(), Some("licence"));
        assert_eq!(
            fs::read_to_string(self.journal.previous_dir().join(&executable))
                .ok()
                .as_deref(),
            Some("old program")
        );
        assert!(
            !self.install.join("data").exists(),
            "the archive's data stays out"
        );
        self.assert_untouched();
    }

    /// What is neither the old nor the new version's stays where it was.
    pub(crate) fn assert_untouched(&self) {
        assert_eq!(
            self.read("notes.txt").as_deref(),
            Some("the person's own file")
        );
        assert!(self.data.join("rcommander.db").is_file());
    }
}

pub(crate) fn write(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("folder");
    }
    fs::write(path, text).expect("write");
}

/// `previous/locked/rcommander-mcp` in a folder that refuses removals; `None` (and the test is
/// moot) where permissions do not hold, as for root.
#[cfg(unix)]
pub(crate) fn locked_folder(previous: &Path) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt as _;
    let locked = previous.join("locked");
    write(&locked.join("rcommander-mcp"), "a server a client started");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).expect("lock");
    if fs::write(locked.join("probe"), "").is_ok() {
        eprintln!("permissions do not hold for this user; nothing to prove");
        unlock(&locked);
        return None;
    }
    Some(locked)
}

#[cfg(unix)]
pub(crate) fn unlock(folder: &Path) {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(folder, fs::Permissions::from_mode(0o755)).expect("unlock");
}

pub(crate) fn tar_archive(path: &Path, files: &[(String, &str)]) {
    let file = fs::File::create(path).expect("archive");
    let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::fast());
    let mut builder = tar::Builder::new(encoder);
    for (name, text) in files {
        let mut header = tar::Header::new_gnu();
        header.set_size(text.len() as u64);
        header.set_mode(0o755);
        builder
            .append_data(&mut header, name, text.as_bytes())
            .expect("entry");
    }
    builder
        .into_inner()
        .expect("tar")
        .finish()
        .expect("gzip")
        .flush()
        .expect("flush");
}

pub(crate) fn zip_archive(path: &Path, files: &[(String, &str)]) {
    let file = fs::File::create(path).expect("archive");
    let mut writer = zip::ZipWriter::new(file);
    for (name, text) in files {
        writer
            .start_file(name.as_str(), zip::write::SimpleFileOptions::default())
            .expect("entry");
        writer.write_all(text.as_bytes()).expect("write");
    }
    writer.finish().expect("zip");
}
