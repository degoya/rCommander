//! The viewer case of the measurements over FTP (RC-080-46, `scripts/perf.sh --cases`): a large
//! file on rc-remote's in-process FTP server (no network: 127.0.0.1), opened as F3 opens it, its
//! end and middle read (Ctrl+End, the scroll bar) and a search backwards from the end for a text
//! near the start. Without `REST` every read would read the file from byte 0.
//!
//! `perf ftp-viewer <MiB>` prints `ftp_open`, `ftp_window_end`, `ftp_window_middle` and
//! `ftp_search_back` in milliseconds, `ftp_size_mib`, and the resident memory peak.

use std::{
    error::Error,
    io::Write as _,
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};

use rc_engine::viewer::{BLOCK_SIZE, Direction, FileWindow, Needle, SearchOutcome, search};
use rc_remote::ftp::{
    Credentials, Encryption, FtpConfig, FtpVfs,
    test_server::{Options, PASSWORD, TestServer},
};
use rc_vault::SecretString;
use rc_vfs::Vfs;

use super::{memory_kib, timed};

/// What the search looks for, 1 MiB into the file.
const NEEDLE: &[u8] = b"the needle of the measurement";

pub(super) async fn run(mib: u64) -> Result<(), Box<dyn Error>> {
    let server = TestServer::start(Options::default()).await;
    write_file(&server.local("big.log"), mib)?;
    let mut config = FtpConfig::new(
        "127.0.0.1",
        Encryption::None,
        Credentials::User {
            name: "tester".to_owned(),
            password: SecretString::from(PASSWORD),
        },
    );
    config.port = server.port;
    let vfs: Arc<dyn Vfs> = Arc::new(FtpVfs::connect(config).await?);
    let window = timed("ftp_open", FileWindow::open(vfs, "/big.log")).await?;
    let size = window.size();
    let block = BLOCK_SIZE as u64;
    let end = size.saturating_sub(block);
    timed("ftp_window_end", window.read_uncached(end, BLOCK_SIZE)).await?;
    timed(
        "ftp_window_middle",
        window.read_uncached(size / 2, BLOCK_SIZE),
    )
    .await?;
    let needle = Needle::bytes(NEEDLE)?;
    let cancel = AtomicBool::new(false);
    let found = timed(
        "ftp_search_back",
        search(
            &window,
            &needle,
            size,
            Direction::Backward,
            &cancel,
            &mut |_| {},
        ),
    )
    .await?;
    if !matches!(found, SearchOutcome::Found(found) if found.offset == 1 << 20) {
        return Err(format!("the backward search answered {found:?}").into());
    }
    println!("ftp_size_mib\t{mib}");
    if let Some(peak) = memory_kib("VmHWM") {
        println!("rss_peak_kib\t{peak}");
    }
    Ok(())
}

/// `mib` MiB of numbered lines with [`NEEDLE`] at 1 MiB.
fn write_file(path: &Path, mib: u64) -> std::io::Result<()> {
    let mut file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let total = mib << 20;
    let mut written = 0_u64;
    let mut line = 0_u64;
    while written < total {
        let text = if written == 1 << 20 {
            NEEDLE.to_vec()
        } else {
            format!("{line:010} a line of a large log file, about eighty bytes long ......\n")
                .into_bytes()
        };
        // Lines are cut so the needle starts exactly at 1 MiB.
        let room = if written < 1 << 20 {
            usize::try_from((1 << 20) - written).unwrap_or(usize::MAX)
        } else {
            usize::MAX
        };
        let text = &text[..text.len().min(room)];
        file.write_all(text)?;
        written += text.len() as u64;
        line += 1;
    }
    file.flush()
}
