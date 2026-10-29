//! The scene file of `--screenshots` (`scripts/screenshots/scenes.json`): read strictly - an
//! unknown field, an unknown step, a name that could leave the output folder or a limit passed
//! ends the run before a window opens.
//!
//! ```json
//! { "version": 1, "size": { "width": 1280, "height": 800 }, "startup_ms": 2500,
//!   "scenes": [ { "name": "panels-dark", "theme": "dark", "capture": "main", "settle_ms": 800,
//!                 "steps": [ { "key": "Ctrl+F" },
//!                            { "command": { "command": "panel.navigate",
//!                                           "params": { "path": "Documents" } } },
//!                            { "wait_ms": 300 }, { "open": "settings" }, { "close": "lister" },
//!                            { "mcp_question": { "tool": "ops_copy", "class": "write" } } ] } ] }
//! ```
//!
//! `{demo}` in a command's strings and a question's values stands for the demo folder, so a
//! scene names its files wherever the folder lies.
//!
//! A `command` step runs as its key would (`App::run_bound`) and may only read or write - no
//! command that deletes, overwrites, runs a program or decides about trust. A `key` step goes
//! through the keymap; the window refuses one bound to a deleting or trust command
//! (`shooter.rs`).

use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use rc_engine::{Chord, Command, CommandClass, McpArgument};
use serde::Deserialize;
use serde_json::Value;

/// The largest scene file read.
pub const MAX_FILE_BYTES: u64 = 256 * 1024;
/// The most scenes in one file.
pub const MAX_SCENES: usize = 64;
/// The most steps in one scene.
pub const MAX_STEPS: usize = 64;
/// The longest wait of a step or before a shot.
pub const MAX_WAIT_MS: u64 = 30_000;
/// The longest wait for the window after the start.
pub const MAX_STARTUP_MS: u64 = 60_000;
/// The longest scene name; it becomes `<name>.png`.
pub const MAX_NAME: usize = 64;
/// The most arguments of an MCP question, and the longest value shown.
pub const MAX_ARGUMENTS: usize = 16;
pub const MAX_VALUE: usize = 500;

/// The window's smallest and largest size, in logical pixels.
const MIN_SIZE: (u32, u32) = (640, 400);
const MAX_SIZE: (u32, u32) = (3840, 2160);
const DEFAULT_SIZE: (u32, u32) = (1280, 800);
const DEFAULT_STARTUP_MS: u64 = 2500;
const DEFAULT_SETTLE_MS: u64 = 800;

/// Names Windows keeps for devices, with any extension.
const RESERVED: [&str; 4] = ["con", "prn", "aux", "nul"];

/// The scene file, checked.
#[derive(Debug, Clone)]
pub struct Plan {
    /// The main window's size.
    pub width: u32,
    pub height: u32,
    /// How long the window gets to open and list before the first scene.
    pub startup: Duration,
    pub scenes: Vec<Scene>,
}

/// One picture: the steps that set it up, then the shot.
#[derive(Debug, Clone)]
pub struct Scene {
    /// Also the file name, `<name>.png`.
    pub name: String,
    /// The look switched to before the steps; the one before stays when `None`.
    pub theme: Option<Look>,
    pub steps: Vec<Step>,
    /// How long the window gets to draw after the last step.
    pub settle: Duration,
    pub capture: Capture,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Look {
    Dark,
    Light,
}

/// The window a scene photographs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capture {
    #[default]
    Main,
    /// The settings, which always have a window of their own.
    Settings,
    /// A Lister, a window of its own as in Total Commander.
    Lister,
}

/// A dialog a step opens that no command opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Opening {
    Settings,
}

#[derive(Debug, Clone)]
pub enum Step {
    /// A key press, as the keymap spells it.
    Key(Chord),
    /// A command, run as its key runs it.
    Command(Box<Command>),
    Wait(Duration),
    Open(Opening),
    /// The window of this kind closes, as its X closes it.
    Close(Capture),
    /// The question an MCP call would ask, shown without a call behind it.
    Ask(Ask),
}

/// An MCP question to show; the panel it names is the active one as it stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ask {
    pub tool: String,
    pub class: CommandClass,
    pub arguments: Vec<McpArgument>,
    /// Whether "allow for this session" is offered.
    pub session: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFile {
    version: u32,
    #[serde(default)]
    size: Option<RawSize>,
    #[serde(default)]
    startup_ms: Option<u64>,
    scenes: Vec<RawScene>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSize {
    width: u32,
    height: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawScene {
    name: String,
    #[serde(default)]
    theme: Option<Look>,
    #[serde(default)]
    steps: Vec<RawStep>,
    #[serde(default)]
    settle_ms: Option<u64>,
    #[serde(default)]
    capture: Capture,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum RawStep {
    Key(String),
    /// Read as JSON first, so a parameter the command does not know is found.
    Command(Value),
    WaitMs(u64),
    Open(Opening),
    Close(Capture),
    McpQuestion(RawAsk),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAsk {
    tool: String,
    class: CommandClass,
    #[serde(default)]
    arguments: Vec<RawArgument>,
    #[serde(default)]
    session: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawArgument {
    name: String,
    value: String,
}

/// The placeholder for the demo folder.
pub const DEMO: &str = "{demo}";

/// Reads and checks a scene file's text; `demo` replaces [`DEMO`].
///
/// # Errors
///
/// The text is no scene file of version 1, or a scene breaks a rule above.
pub fn parse(text: &str, demo: &str) -> Result<Plan> {
    let raw: RawFile = serde_json::from_str(text).context("the scene file does not parse")?;
    ensure!(
        raw.version == 1,
        "scene file version {} is not 1",
        raw.version
    );
    let (width, height) = raw
        .size
        .map_or(DEFAULT_SIZE, |size| (size.width, size.height));
    ensure!(
        (MIN_SIZE.0..=MAX_SIZE.0).contains(&width) && (MIN_SIZE.1..=MAX_SIZE.1).contains(&height),
        "the window size {width}x{height} is outside {}x{} to {}x{}",
        MIN_SIZE.0,
        MIN_SIZE.1,
        MAX_SIZE.0,
        MAX_SIZE.1
    );
    let startup = raw.startup_ms.unwrap_or(DEFAULT_STARTUP_MS);
    ensure!(
        startup <= MAX_STARTUP_MS,
        "startup_ms {startup} is over {MAX_STARTUP_MS}"
    );
    ensure!(!raw.scenes.is_empty(), "the scene file has no scenes");
    ensure!(
        raw.scenes.len() <= MAX_SCENES,
        "the scene file has more than {MAX_SCENES} scenes"
    );
    let mut scenes: Vec<Scene> = Vec::with_capacity(raw.scenes.len());
    for scene in raw.scenes {
        check_name(&scene.name)?;
        ensure!(
            scenes.iter().all(|known| known.name != scene.name),
            "the scene name '{}' is used twice",
            scene.name
        );
        scenes.push(check_scene(scene, demo)?);
    }
    Ok(Plan {
        width,
        height,
        startup: Duration::from_millis(startup),
        scenes,
    })
}

/// A scene name: 1 to 64 of `a-z`, `0-9` and `-`, starting with a letter or digit, and no
/// device name of Windows - so `<name>.png` is one plain file in the output folder.
///
/// # Errors
///
/// The name breaks that rule.
pub fn check_name(name: &str) -> Result<()> {
    let plain = !name.is_empty()
        && name.len() <= MAX_NAME
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && name.bytes().next().is_some_and(|byte| byte != b'-');
    if !plain {
        bail!(
            "the scene name '{name}' is not 1 to {MAX_NAME} of a-z, 0-9 and -, led by a-z or 0-9"
        );
    }
    if is_device(name) {
        bail!("the scene name '{name}' is a device name of Windows");
    }
    Ok(())
}

fn is_device(name: &str) -> bool {
    if RESERVED.contains(&name) {
        return true;
    }
    // COM1 to COM9 and LPT1 to LPT9.
    let bytes = name.as_bytes();
    bytes.len() == 4
        && (name.starts_with("com") || name.starts_with("lpt"))
        && (b'1'..=b'9').contains(&bytes[3])
}

fn check_scene(scene: RawScene, demo: &str) -> Result<Scene> {
    let name = scene.name;
    ensure!(
        scene.steps.len() <= MAX_STEPS,
        "scene '{name}' has more than {MAX_STEPS} steps"
    );
    let settle = scene.settle_ms.unwrap_or(DEFAULT_SETTLE_MS);
    ensure!(
        settle <= MAX_WAIT_MS,
        "scene '{name}': settle_ms {settle} is over {MAX_WAIT_MS}"
    );
    let steps = scene
        .steps
        .into_iter()
        .enumerate()
        .map(|(index, step)| {
            check_step(step, demo).with_context(|| format!("scene '{name}', step {}", index + 1))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Scene {
        name,
        theme: scene.theme,
        steps,
        settle: Duration::from_millis(settle),
        capture: scene.capture,
    })
}

fn check_step(step: RawStep, demo: &str) -> Result<Step> {
    Ok(match step {
        RawStep::Key(keys) => Step::Key(
            keys.parse()
                .map_err(|error| anyhow::anyhow!("the key '{keys}' does not parse: {error}"))?,
        ),
        RawStep::Command(mut value) => {
            place_demo(&mut value, demo);
            Step::Command(Box::new(check_command(value)?))
        }
        RawStep::WaitMs(wait) => {
            ensure!(wait <= MAX_WAIT_MS, "wait_ms {wait} is over {MAX_WAIT_MS}");
            Step::Wait(Duration::from_millis(wait))
        }
        RawStep::Open(opening) => Step::Open(opening),
        RawStep::Close(Capture::Main) => bail!("the main window is not closed by a step"),
        RawStep::Close(capture) => Step::Close(capture),
        RawStep::McpQuestion(ask) => Step::Ask(check_ask(ask, demo)?),
    })
}

/// A command as `{"command": "<id>", "params": {...}}`: every parameter one the command knows,
/// and only a command that reads or writes.
fn check_command(value: Value) -> Result<Command> {
    let Value::Object(fields) = &value else {
        bail!("a command is an object of command and params");
    };
    if let Some(field) = fields
        .keys()
        .find(|field| !matches!(field.as_str(), "command" | "params"))
    {
        bail!("a command has no field '{field}'");
    }
    let command: Command =
        serde_json::from_value(value.clone()).context("the command does not parse")?;
    // A parameter serde passed over is missing from the command written back.
    if let Some(Value::Object(given)) = fields.get("params") {
        let written = serde_json::to_value(&command).context("the command is not written back")?;
        let known = written.get("params").and_then(Value::as_object);
        if let Some(field) = given
            .keys()
            .find(|field| known.is_none_or(|known| !known.contains_key(*field)))
        {
            bail!("{} has no parameter '{field}'", command.id());
        }
    }
    let class = command.spec().class;
    ensure!(
        matches!(class, CommandClass::Read | CommandClass::Write),
        "{} is of class {}; a scene only reads and writes",
        command.id(),
        class.as_str()
    );
    Ok(command)
}

/// [`DEMO`] in every string of `value` replaced by `demo`.
fn place_demo(value: &mut Value, demo: &str) {
    match value {
        Value::String(text) if text.contains(DEMO) => *text = text.replace(DEMO, demo),
        Value::Array(items) => items.iter_mut().for_each(|item| place_demo(item, demo)),
        Value::Object(fields) => fields
            .values_mut()
            .for_each(|field| place_demo(field, demo)),
        _ => {}
    }
}

fn check_ask(ask: RawAsk, demo: &str) -> Result<Ask> {
    let tool_ok = !ask.tool.is_empty()
        && ask.tool.len() <= MAX_NAME
        && ask
            .tool
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
    ensure!(
        tool_ok,
        "the tool '{}' is not 1 to {MAX_NAME} of a-z, 0-9 and _",
        ask.tool
    );
    ensure!(
        ask.arguments.len() <= MAX_ARGUMENTS,
        "a question shows at most {MAX_ARGUMENTS} arguments"
    );
    let mut arguments = Vec::with_capacity(ask.arguments.len());
    for argument in ask.arguments {
        ensure!(
            !argument.name.is_empty() && argument.name.chars().count() <= MAX_NAME,
            "an argument's name is empty or longer than {MAX_NAME}"
        );
        ensure!(
            argument.value.chars().count() <= MAX_VALUE,
            "the value of '{}' is longer than {MAX_VALUE}",
            argument.name
        );
        arguments.push(McpArgument {
            name: argument.name,
            value: argument.value.replace(DEMO, demo),
        });
    }
    Ok(Ask {
        tool: ask.tool,
        class: ask.class,
        arguments,
        session: ask.session,
    })
}

#[cfg(test)]
#[path = "scenes_tests.rs"]
mod tests;
