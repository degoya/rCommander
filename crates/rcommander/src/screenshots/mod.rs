//! `rcommander --screenshots <scenes.json> --out <dir> [--demo-root <dir>]` (RC-100-45): the
//! pictures of the website and the wiki, taken of the real window on Windows.
//!
//! The run never touches the person's data: its database lies in a throwaway profile under the
//! temporary folder, its panels show a demo tree (`demo.rs`) in a folder of its own, and its
//! connections are made-up ones without a secret. No MCP server starts, nothing connects, and
//! nothing of the last run is restored. Each scene of the file (`scenes.rs`) switches the look,
//! runs its steps - keys, commands, a dialog opened - waits for the window to draw, and saves
//! `iced::window::screenshot` as `<dir>/<scene>.png` (`out.rs`); then the window closes, the
//! demo tree and the profile go, and `<dir>/screenshots.json` says what was taken or why not.
//! There is no MCP tool for it: a picture of the real window could show a secret.
//!
//! The module is a child of `app::startup` (declared there) so that it starts the window through
//! the same `App::boot` with a setup of its own.

mod demo;
mod out;
mod scenes;
mod shooter;

use std::{
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
    sync::{Arc, Mutex, PoisonError},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow, bail};
use rc_db::Db;
use rc_engine::{Command, Engine, Outcome, VfsPath, settings::DIALOG_WINDOWS_KEY};
use rc_vault::Vault;
use serde_json::Value;

use self::{
    out::{Manifest, OutDir},
    scenes::Plan,
    shooter::Report,
};
use super::Setup;
use crate::{
    app::{
        drive_bar::DriveBarLayout,
        window_state::{Geometry, WindowState},
    },
    mcp::McpStatus,
    theme,
};

/// How long the engine's tasks get to end after the window closed.
const SHUTDOWN: Duration = Duration::from_secs(3);

/// The hidden flags; `rcommander --help` does not list them.
#[derive(Debug, Default, clap::Args)]
pub struct Flags {
    /// The scene file: take its screenshots and exit.
    #[arg(long, hide = true, value_name = "SCENES", requires = "out")]
    pub screenshots: Option<PathBuf>,
    /// The folder the pictures and screenshots.json go to.
    #[arg(long, hide = true, value_name = "DIR", requires = "screenshots")]
    pub out: Option<PathBuf>,
    /// The folder the demo files are made in, whose path the pictures show; missing or empty,
    /// and removed at the end. A folder of the throwaway profile when left out.
    #[arg(long, hide = true, value_name = "DIR", requires = "screenshots")]
    pub demo_root: Option<PathBuf>,
}

/// Takes the screenshots when `flags` ask for them; `None` when they do not.
pub fn run_if_asked(flags: &Flags) -> Option<ExitCode> {
    let scenes = flags.screenshots.as_deref()?;
    let out = flags.out.as_deref()?;
    Some(match run(scenes, out, flags.demo_root.as_deref()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // Shown when started from a shell; the manifest says it in any case.
            eprintln!("rcommander --screenshots: {error:#}");
            ExitCode::FAILURE
        }
    })
}

fn run(scenes: &Path, out: &Path, demo_root: Option<&Path>) -> Result<()> {
    let out = OutDir::prepare(out)?;
    let manifest = out.manifest()?;
    if manifest.exists() {
        fs::remove_file(&manifest)
            .with_context(|| format!("{} cannot be removed", manifest.display()))?;
    }
    let report = Arc::new(Mutex::new(Report::default()));
    let shot = shoot(scenes, &out, demo_root, &report);
    let mut report = std::mem::take(&mut *report.lock().unwrap_or_else(PoisonError::into_inner));
    if let Err(error) = shot {
        report.error.get_or_insert_with(|| format!("{error:#}"));
    }
    let ok = report.error.is_none();
    let written = out::write_manifest(
        &manifest,
        &Manifest {
            version: env!("CARGO_PKG_VERSION"),
            ok,
            error: report.error.clone(),
            shots: report.shots,
        },
    );
    match report.error {
        Some(error) => bail!(error),
        None => written,
    }
}

fn read_plan(path: &Path, demo: &Path) -> Result<Plan> {
    let meta = fs::metadata(path).with_context(|| format!("{} cannot be read", path.display()))?;
    if !meta.is_file() || meta.len() > scenes::MAX_FILE_BYTES {
        bail!(
            "{} is no file of at most {} bytes",
            path.display(),
            scenes::MAX_FILE_BYTES
        );
    }
    let text =
        fs::read_to_string(path).with_context(|| format!("{} cannot be read", path.display()))?;
    scenes::parse(&text, &demo.display().to_string())
        .with_context(|| format!("in {}", path.display()))
}

/// The run between the output folder and the manifest: the profile and the demo folder made,
/// the scenes read, the window shown, both folders removed again whatever happened.
fn shoot(
    scenes: &Path,
    out: &OutDir,
    demo_root: Option<&Path>,
    report: &Arc<Mutex<Report>>,
) -> Result<()> {
    let profile = Scratch::new(make_profile()?);
    let demo = match demo_root {
        Some(root) => Scratch::new(demo::claim(root)?),
        None => {
            let root = profile.0.join("demo");
            fs::create_dir(&root).with_context(|| format!("{} cannot be made", root.display()))?;
            Scratch(root, false)
        }
    };
    let plan = read_plan(scenes, &demo.0)?;
    demo::write_tree(&demo.0)?;
    let runtime = tokio::runtime::Runtime::new().context("no runtime starts")?;
    let shown = runtime
        .block_on(prepare(&plan, &profile.0, &demo.0))
        .and_then(|setup| {
            shooter::show(setup, plan, out.clone(), Arc::clone(report))
                .map_err(|error| anyhow!("the window does not run: {error}"))
        });
    // The engine's tasks end before the files they may hold open are removed.
    runtime.shutdown_timeout(SHUTDOWN);
    rc_engine::unpacked::remove_own();
    drop(demo);
    drop(profile);
    shown
}

/// The database, the engine on the demo tree and the window's start, as `startup::prepare`
/// makes them for a real run.
async fn prepare(plan: &Plan, profile: &Path, demo: &Path) -> Result<Setup> {
    let db = Db::open(profile.join("rcommander.db"))
        .await
        .map_err(|error| anyhow!("the throwaway database does not open: {error}"))?;
    theme::init();
    // The website's and the wiki's pictures are English, whatever the machine's language.
    crate::locale::choose_setting("en");
    let engine = Engine::open(VfsPath::local(demo), VfsPath::local(demo.join("Projects")))
        .await
        .map_err(|error| anyhow!("the demo folders do not open: {}", error.code()))?;
    if let Ok(Outcome::Terminals { shells, .. }) = engine.dispatch(Command::ListTerminals {}).await
    {
        crate::term::set_shells(shells);
    }
    engine.attach_db(db.clone());
    // The large dialogs over the main window, so its picture shows them in place.
    engine
        .dispatch(Command::SetSetting {
            key: DIALOG_WINDOWS_KEY.to_owned(),
            value: Value::Bool(false),
        })
        .await
        .map_err(|error| anyhow!("{DIALOG_WINDOWS_KEY} is not set: {}", error.code()))?;
    let vault = Vault::open(db.clone())
        .await
        .map_err(|error| anyhow!("the vault does not open: {}", error.code()))?;
    engine.attach_vault(vault);
    demo::add_connections(&engine).await?;
    let keymap = engine.keymap().await;
    let geometry = Geometry {
        width: plan.width as f32,
        height: plan.height as f32,
        position: None,
        maximized: false,
    };
    Ok(Setup {
        keymap,
        engine,
        db: Some(db),
        geometry: WindowState::new(Some(geometry)).geometry(),
        drive_layout: DriveBarLayout::default(),
        mcp: McpStatus::Off,
        server: Arc::new(Mutex::new(None)),
        theme: theme::ThemeChoice::Dark,
        notice: None,
        masks: Vec::new(),
        info: None,
        dialogs: Vec::new(),
    })
}

/// A fresh folder for the throwaway profile under the temporary folder.
fn make_profile() -> Result<PathBuf> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let path = std::env::temp_dir().join(format!(
        "rcommander-screenshots-{}-{nanos}",
        std::process::id()
    ));
    // `create_dir`, not `create_dir_all`: a folder already there is someone else's.
    fs::create_dir(&path).with_context(|| format!("{} cannot be made", path.display()))?;
    Ok(path)
}

/// A folder this run made, removed with everything in it when dropped - unless it lies inside
/// another one that is (`false`).
struct Scratch(PathBuf, bool);

impl Scratch {
    fn new(path: PathBuf) -> Self {
        Self(path, true)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if self.1 && fs::remove_dir_all(&self.0).is_err() {
            eprintln!(
                "rcommander --screenshots: {} was left behind",
                self.0.display()
            );
        }
    }
}
