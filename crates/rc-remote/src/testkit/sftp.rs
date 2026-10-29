//! The test server's `sftp` subsystem: a directory on disk served as `/`, answering the way
//! OpenSSH's `sftp-server` does - a path that is not a directory is "no such file", "exists",
//! "is a directory" and "not empty" are a bare "failure", and `RENAME` does not replace.
//!
//! [`SftpServe::hostile_names`] are listed in every directory before its real entries, as a
//! hostile server would; [`SftpServe::block`] is how many names one `READDIR` answers.

use std::{
    collections::{HashMap, VecDeque},
    fs::{self, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
    time::{Duration, SystemTime},
};

use russh::{Channel, server::Msg};
use russh_sftp::protocol::{
    Attrs, Data, File, FileAttributes, Handle, Name, OpenFlags, Status, StatusCode, Version,
};

/// The most bytes one `READ` answers.
const READ_MAX: u32 = 256 * 1024;

/// What the subsystem serves.
#[derive(Debug, Clone)]
pub struct SftpServe {
    /// The directory served as `/`.
    pub root: PathBuf,
    /// The home directory, what `realpath(".")` answers.
    pub home: String,
    /// Names listed in every directory before its real entries.
    pub hostile_names: Vec<String>,
    /// Names per `READDIR` answer.
    pub block: usize,
}

impl SftpServe {
    /// `root` served as `/`, home `/`, 100 names per block, nothing hostile.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            home: "/".to_owned(),
            hostile_names: Vec::new(),
            block: 100,
        }
    }
}

/// Serves SFTP on `channel` until the client closes it.
pub(super) fn serve(channel: Channel<Msg>, options: SftpServe) {
    let handler = Served {
        options,
        handles: HashMap::new(),
        next: 0,
    };
    tokio::spawn(russh_sftp::server::run(channel.into_stream(), handler));
}

enum Opened {
    File(fs::File),
    Dir(VecDeque<File>),
}

struct Served {
    options: SftpServe,
    handles: HashMap<String, Opened>,
    next: u64,
}

/// The status `sftp-server` answers for an error of the file system.
fn status(error: &io::Error) -> StatusCode {
    match error.kind() {
        io::ErrorKind::NotFound | io::ErrorKind::NotADirectory => StatusCode::NoSuchFile,
        io::ErrorKind::PermissionDenied => StatusCode::PermissionDenied,
        io::ErrorKind::InvalidInput | io::ErrorKind::InvalidFilename => StatusCode::BadMessage,
        io::ErrorKind::Unsupported => StatusCode::OpUnsupported,
        _ => StatusCode::Failure,
    }
}

fn ok(id: u32) -> Status {
    Status {
        id,
        status_code: StatusCode::Ok,
        error_message: "Ok".to_owned(),
        language_tag: "en-US".to_owned(),
    }
}

fn done(id: u32, result: io::Result<()>) -> Result<Status, StatusCode> {
    result.map(|()| ok(id)).map_err(|error| status(&error))
}

fn attrs(metadata: io::Result<fs::Metadata>) -> Result<FileAttributes, StatusCode> {
    metadata
        .map(|metadata| FileAttributes::from(&metadata))
        .map_err(|error| status(&error))
}

impl Served {
    /// `wire` made absolute against the home directory and folded, as the server names it.
    fn absolute(&self, wire: &str) -> String {
        let joined = if wire.starts_with('/') {
            wire.to_owned()
        } else {
            format!("{}/{wire}", self.options.home)
        };
        let mut parts: Vec<&str> = Vec::new();
        for part in joined.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                name => parts.push(name),
            }
        }
        format!("/{}", parts.join("/"))
    }

    /// Where `wire` lies on disk; never above the root.
    fn local(&self, wire: &str) -> PathBuf {
        let mut path = self.options.root.clone();
        for component in Path::new(&self.absolute(wire)).components() {
            if let Component::Normal(name) = component {
                path.push(name);
            }
        }
        path
    }

    fn handle(&mut self, opened: Opened) -> String {
        self.next += 1;
        let handle = format!("h{}", self.next);
        self.handles.insert(handle.clone(), opened);
        handle
    }

    fn file(&mut self, handle: &str) -> Result<&mut fs::File, StatusCode> {
        match self.handles.get_mut(handle) {
            Some(Opened::File(file)) => Ok(file),
            _ => Err(StatusCode::Failure),
        }
    }

    fn entry(&self, dir: &Path, name: String) -> File {
        let found = attrs(fs::symlink_metadata(dir.join(&name))).unwrap_or_else(|_| {
            let mut bare = FileAttributes::empty();
            bare.set_regular(true);
            bare
        });
        File::new(name, found)
    }
}

impl russh_sftp::server::Handler for Served {
    type Error = StatusCode;

    fn unimplemented(&self) -> Self::Error {
        StatusCode::OpUnsupported
    }

    async fn init(
        &mut self,
        _version: u32,
        _extensions: HashMap<String, String>,
    ) -> Result<Version, Self::Error> {
        Ok(Version::new())
    }

    async fn realpath(&mut self, id: u32, path: String) -> Result<Name, Self::Error> {
        Ok(Name {
            id,
            files: vec![File::dummy(self.absolute(&path))],
        })
    }

    async fn stat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
        let attrs = attrs(fs::metadata(self.local(&path)))?;
        Ok(Attrs { id, attrs })
    }

    async fn lstat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
        let attrs = attrs(fs::symlink_metadata(self.local(&path)))?;
        Ok(Attrs { id, attrs })
    }

    async fn fstat(&mut self, id: u32, handle: String) -> Result<Attrs, Self::Error> {
        let attrs = attrs(self.file(&handle)?.metadata())?;
        Ok(Attrs { id, attrs })
    }

    async fn open(
        &mut self,
        id: u32,
        filename: String,
        pflags: OpenFlags,
        _attrs: FileAttributes,
    ) -> Result<Handle, Self::Error> {
        let file = OpenOptions::from(pflags)
            .open(self.local(&filename))
            .map_err(|error| status(&error))?;
        let handle = self.handle(Opened::File(file));
        Ok(Handle { id, handle })
    }

    async fn read(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        len: u32,
    ) -> Result<Data, Self::Error> {
        let file = self.file(&handle)?;
        let mut data = vec![0; len.min(READ_MAX) as usize];
        let read = file
            .seek(SeekFrom::Start(offset))
            .and_then(|_| file.read(&mut data))
            .map_err(|error| status(&error))?;
        if read == 0 {
            return Err(StatusCode::Eof);
        }
        data.truncate(read);
        Ok(Data { id, data })
    }

    async fn write(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        data: Vec<u8>,
    ) -> Result<Status, Self::Error> {
        let file = self.file(&handle)?;
        let written = file
            .seek(SeekFrom::Start(offset))
            .and_then(|_| file.write_all(&data));
        done(id, written)
    }

    async fn close(&mut self, id: u32, handle: String) -> Result<Status, Self::Error> {
        self.handles.remove(&handle);
        Ok(ok(id))
    }

    async fn opendir(&mut self, id: u32, path: String) -> Result<Handle, Self::Error> {
        let dir = self.local(&path);
        let mut names: Vec<String> = fs::read_dir(&dir)
            .map_err(|error| status(&error))?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        let mut listing: VecDeque<File> = [".", ".."]
            .into_iter()
            .map(|name| self.entry(&dir, name.to_owned()))
            .collect();
        listing.extend(
            self.options
                .hostile_names
                .iter()
                .map(|name| File::new(name.clone(), FileAttributes::dummy())),
        );
        listing.extend(names.into_iter().map(|name| self.entry(&dir, name)));
        let handle = self.handle(Opened::Dir(listing));
        Ok(Handle { id, handle })
    }

    async fn readdir(&mut self, id: u32, handle: String) -> Result<Name, Self::Error> {
        let block = self.options.block.max(1);
        let Some(Opened::Dir(listing)) = self.handles.get_mut(&handle) else {
            return Err(StatusCode::Failure);
        };
        if listing.is_empty() {
            return Err(StatusCode::Eof);
        }
        let take = block.min(listing.len());
        Ok(Name {
            id,
            files: listing.drain(..take).collect(),
        })
    }

    async fn remove(&mut self, id: u32, filename: String) -> Result<Status, Self::Error> {
        done(id, fs::remove_file(self.local(&filename)))
    }

    async fn mkdir(
        &mut self,
        id: u32,
        path: String,
        _attrs: FileAttributes,
    ) -> Result<Status, Self::Error> {
        done(id, fs::create_dir(self.local(&path)))
    }

    async fn rmdir(&mut self, id: u32, path: String) -> Result<Status, Self::Error> {
        done(id, fs::remove_dir(self.local(&path)))
    }

    async fn rename(
        &mut self,
        id: u32,
        oldpath: String,
        newpath: String,
    ) -> Result<Status, Self::Error> {
        let (from, to) = (self.local(&oldpath), self.local(&newpath));
        if fs::symlink_metadata(&to).is_ok() {
            return Err(StatusCode::Failure);
        }
        done(id, fs::rename(from, to))
    }

    async fn setstat(
        &mut self,
        id: u32,
        path: String,
        attrs: FileAttributes,
    ) -> Result<Status, Self::Error> {
        let local = self.local(&path);
        let changed = set_permissions(&local, attrs.permissions).and_then(|()| {
            let Some(mtime) = attrs.mtime else {
                return Ok(());
            };
            let at = |secs: u32| {
                filetime::FileTime::from_system_time(
                    SystemTime::UNIX_EPOCH + Duration::from_secs(u64::from(secs)),
                )
            };
            // Not through a `File::open` handle: Windows sets times only on a handle opened
            // for writing attributes, and opens no directory without backup semantics.
            filetime::set_file_times(&local, at(attrs.atime.unwrap_or(mtime)), at(mtime))
        });
        done(id, changed)
    }

    async fn readlink(&mut self, id: u32, path: String) -> Result<Name, Self::Error> {
        let target = fs::read_link(self.local(&path)).map_err(|error| status(&error))?;
        Ok(Name {
            id,
            files: vec![File::dummy(target.to_string_lossy())],
        })
    }

    async fn symlink(
        &mut self,
        id: u32,
        linkpath: String,
        targetpath: String,
    ) -> Result<Status, Self::Error> {
        done(id, make_link(&targetpath, &self.local(&linkpath)))
    }
}

#[cfg(unix)]
fn set_permissions(path: &Path, mode: Option<u32>) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    match mode {
        Some(mode) => fs::set_permissions(path, fs::Permissions::from_mode(mode & 0o7777)),
        None => Ok(()),
    }
}

#[cfg(not(unix))]
fn set_permissions(path: &Path, mode: Option<u32>) -> io::Result<()> {
    let Some(mode) = mode else {
        return Ok(());
    };
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_readonly(mode & 0o222 == 0);
    fs::set_permissions(path, permissions)
}

#[cfg(unix)]
fn make_link(target: &str, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(not(unix))]
fn make_link(_target: &str, _link: &Path) -> io::Result<()> {
    Err(io::Error::from(io::ErrorKind::Unsupported))
}
