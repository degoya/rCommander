//! The demo data of `--screenshots`: a folder tree of made-up files, an archive among them, and
//! connections to `example.com` hosts (RFC 2606) without a secret - nothing of the person's own,
//! and nothing a picture could leak.
//!
//! The tree is made in a folder of its own that did not exist or was empty, and is removed
//! with it at the end; a folder with anything in it is never touched.

use std::{
    fs::{self, File, FileTimes},
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use anyhow::{Context, Result, ensure};
use rc_engine::{
    AuthMethod, Command, ConnectionDraft, ConnectionKind, Engine, SaveConnectionRequest, SaveFields,
};

/// The files of the tree, by path below its root, with their text. The folders come from the
/// paths.
pub const FILES: &[(&str, &str)] = &[
    (
        "Documents/Invoices/2026-08 Invoice 1042.pdf",
        "%PDF-1.7 demo\n",
    ),
    (
        "Documents/Invoices/2026-09 Invoice 1057.pdf",
        "%PDF-1.7 demo\n",
    ),
    (
        "Documents/Budget 2026.csv",
        "month,rent,food,travel\nJanuary,950,410,120\nFebruary,950,385,0\nMarch,950,430,640\n",
    ),
    (
        "Documents/Meeting notes.md",
        "# Weekly sync\n\n- Release 0.9 is on track\n- Screenshots for the website\n- Next: macOS build\n",
    ),
    (
        "Documents/Travel checklist.txt",
        "Passport\nCharger\nHeadphones\nTickets\n",
    ),
    ("Photos/IMG_2041.jpg", "demo photo"),
    ("Photos/IMG_2042.jpg", "demo photo"),
    ("Photos/IMG_2043.jpg", "demo photo"),
    ("Photos/IMG_2044.jpg", "demo photo"),
    ("Photos/IMG_2045.jpg", "demo photo"),
    ("Photos/IMG_2046.jpg", "demo photo"),
    (
        "Music/Playlist.m3u",
        "#EXTM3U\n#EXTINF:215,Demo Band - First Light\nfirst-light.flac\n",
    ),
    ("Projects/website/index.html", INDEX_HTML),
    ("Projects/website/style.css", STYLE_CSS),
    ("Projects/website/README.md", README_MD),
    ("Projects/website/config.toml", CONFIG_NEW),
    (
        "Projects/website-backup/index.html",
        "<!doctype html>\n<title>Demo site</title>\n<h1>Welcome</h1>\n",
    ),
    ("Projects/website-backup/style.css", STYLE_CSS),
    (
        "Projects/tools/backup.ps1",
        "# Copies the website to the backup folder.\nCopy-Item -Recurse .\\website .\\website-backup -Force\n",
    ),
    (
        "Projects/tools/deploy.sh",
        "#!/bin/sh\n# Uploads the website.\nrsync -av website/ deploy@www.example.com:/srv/www/\n",
    ),
    ("Projects/config.old.toml", CONFIG_OLD),
    ("Projects/config.toml", CONFIG_NEW),
    ("Downloads/notes.txt", "Things to read later.\n"),
    (
        "readme.txt",
        "This folder holds the demo files of rCommander's screenshots.\n",
    ),
];

/// The archive of the tree: the website, packed.
pub const ARCHIVE: &str = "Downloads/website-0.9.0.tar";

/// The connections of the connection manager and the drive bar: name, kind, host, user.
pub const CONNECTIONS: &[(&str, ConnectionKind, &str, &str)] = &[
    (
        "Web server",
        ConnectionKind::Ssh,
        "www.example.com",
        "deploy",
    ),
    (
        "Build machine",
        ConnectionKind::Ssh,
        "build.example.com",
        "ci",
    ),
    (
        "Backup storage",
        ConnectionKind::Ftps,
        "backup.example.net",
        "backup",
    ),
    (
        "Public mirror",
        ConnectionKind::Ftp,
        "mirror.example.org",
        "",
    ),
];

const INDEX_HTML: &str = "<!doctype html>\n<html lang=\"en\">\n<head>\n  <meta charset=\"utf-8\">\n  <title>Demo site</title>\n  <link rel=\"stylesheet\" href=\"style.css\">\n</head>\n<body>\n  <h1>Welcome</h1>\n  <p>A small demo site.</p>\n</body>\n</html>\n";
const STYLE_CSS: &str = "body {\n  font-family: system-ui, sans-serif;\n  margin: 2rem auto;\n  max-width: 40rem;\n}\n\nh1 {\n  color: #2563eb;\n}\n";
const README_MD: &str = "# Demo site\n\nA small static website, used for rCommander's screenshots.\n\n## Build\n\n1. Edit `index.html` and `style.css`.\n2. Run `tools/deploy.sh` to upload it.\n\n## Layout\n\n| File | What it holds |\n| --- | --- |\n| `index.html` | The page |\n| `style.css` | Its look |\n| `config.toml` | The deploy target |\n";
const CONFIG_OLD: &str = "[site]\ntitle = \"Demo site\"\nlanguage = \"en\"\n\n[deploy]\nhost = \"www.example.com\"\npath = \"/srv/www\"\nport = 22\n";
const CONFIG_NEW: &str = "[site]\ntitle = \"Demo site\"\nlanguage = \"en\"\ndescription = \"A small demo site\"\n\n[deploy]\nhost = \"www.example.com\"\npath = \"/srv/www/site\"\nport = 22\nkeep_releases = 3\n";

/// The time every demo file was last changed, 2026-09-14 09:20 UTC; later files a few hours
/// later each, so the dates differ.
const BASE_TIME: u64 = 1_789_377_600;
const STEP: u64 = 5 * 3600 + 17 * 60;

/// Takes `root` for the tree: makes it, or takes it empty. Anything in it already refuses it.
///
/// # Errors
///
/// `root` is relative, is a file, holds anything, or cannot be made.
pub fn claim(root: &Path) -> Result<PathBuf> {
    ensure!(
        root.is_absolute(),
        "the demo folder {} is not absolute",
        root.display()
    );
    if let Ok(mut entries) = fs::read_dir(root) {
        ensure!(
            entries.next().is_none(),
            "the demo folder {} is not empty; remove it or name another",
            root.display()
        );
    } else {
        ensure!(
            !root.exists(),
            "the demo folder {} is no folder",
            root.display()
        );
        fs::create_dir_all(root)
            .with_context(|| format!("the demo folder {} cannot be made", root.display()))?;
    }
    // As given, not resolved: Windows resolves to a `\\?\` path, which the panels would show.
    Ok(root.to_path_buf())
}

/// Writes [`FILES`] and [`ARCHIVE`] below `root`.
///
/// # Errors
///
/// A file or folder cannot be written.
pub fn write_tree(root: &Path) -> Result<()> {
    for (index, (name, text)) in FILES.iter().enumerate() {
        write_file(root, name, text.as_bytes(), index)?;
    }
    let packed: Vec<(&str, &str)> = FILES
        .iter()
        .filter_map(|(name, text)| {
            name.strip_prefix("Projects/")
                .filter(|inner| inner.starts_with("website/"))
                .map(|inner| (inner, *text))
        })
        .collect();
    write_file(root, ARCHIVE, &tar(&packed, BASE_TIME), FILES.len())
}

fn write_file(root: &Path, name: &str, bytes: &[u8], index: usize) -> Result<()> {
    let path = root.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("{} cannot be made", parent.display()))?;
    }
    let mut file =
        File::create(&path).with_context(|| format!("{} cannot be made", path.display()))?;
    file.write_all(bytes)
        .with_context(|| format!("{} cannot be written", path.display()))?;
    let offset = u64::try_from(index).unwrap_or(0) * STEP;
    let modified = SystemTime::UNIX_EPOCH + Duration::from_secs(BASE_TIME + offset);
    // A date is decoration: a file system without it keeps the file as it is.
    let _ = file.set_times(FileTimes::new().set_modified(modified));
    Ok(())
}

/// An uncompressed POSIX tar of `files` (path, text), each changed at `mtime`.
#[must_use]
pub fn tar(files: &[(&str, &str)], mtime: u64) -> Vec<u8> {
    let mut out = Vec::new();
    for (name, text) in files {
        let mut header = [0u8; 512];
        put(&mut header[0..100], name.as_bytes());
        put(&mut header[100..108], b"0000644\0");
        put(&mut header[108..116], b"0000000\0");
        put(&mut header[116..124], b"0000000\0");
        put(
            &mut header[124..136],
            format!("{:011o}\0", text.len()).as_bytes(),
        );
        put(&mut header[136..148], format!("{mtime:011o}\0").as_bytes());
        header[156] = b'0';
        put(&mut header[257..265], b"ustar\x0000");
        // The checksum counts its own field as eight spaces.
        header[148..156].fill(b' ');
        let sum: u32 = header.iter().map(|byte| u32::from(*byte)).sum();
        put(&mut header[148..156], format!("{sum:06o}\0 ").as_bytes());
        out.extend_from_slice(&header);
        out.extend_from_slice(text.as_bytes());
        out.resize(out.len().next_multiple_of(512), 0);
    }
    // Two empty blocks end the archive.
    out.resize(out.len() + 1024, 0);
    out
}

fn put(field: &mut [u8], bytes: &[u8]) {
    let len = bytes.len().min(field.len());
    field[..len].copy_from_slice(&bytes[..len]);
}

/// Stores [`CONNECTIONS`], each a favourite so the drive bar lists it.
///
/// # Errors
///
/// The engine refuses one.
pub async fn add_connections(engine: &Engine) -> Result<()> {
    for (name, kind, host, user) in CONNECTIONS {
        let mut connection = ConnectionDraft::new(*name, *host);
        connection.kind = *kind;
        // No method that stores a secret: the agent for SSH, anonymous FTP without a user.
        connection.auth_method = match (kind.is_ftp(), user.is_empty()) {
            (false, _) => AuthMethod::Agent,
            (true, true) => AuthMethod::Anonymous,
            (true, false) => AuthMethod::Password,
        };
        if !user.is_empty() {
            connection.username = Some((*user).to_owned());
        }
        let request = SaveConnectionRequest {
            fields: SaveFields::Create {
                folder_id: None,
                connection,
            },
            secrets: Vec::new(),
            login_script: None,
            favourite: Some(true),
            credential: None,
        };
        engine
            .dispatch(Command::SaveConnection(request))
            .await
            .map_err(|error| anyhow::anyhow!("the demo connection '{name}': {}", error.code()))?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "demo_tests.rs"]
mod tests;
