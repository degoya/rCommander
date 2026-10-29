//! A dialog without a window, for the focus ring's tests (RC-080-16): laid out by the software
//! renderer, walked by the very operations Tab runs in the window (`field_walk.rs`), pressed
//! with key events as the window delivers them. Below the probe, what every ring test shares:
//! the walk round a dialog, the keys on its stops, and the apps the dialogs open in.

use std::{any::Any, fs};

use iced::{
    Element, Font, Pixels, Point, Rectangle, Size,
    advanced::{
        Layout, Shell, clipboard, layout, mouse,
        renderer::Headless,
        widget::{
            Id, Operation, Tree,
            operation::{self, Focusable, Outcome},
            tree,
        },
    },
    event::Event,
    keyboard::{
        self, Key, Modifiers,
        key::{Named, NativeCode, Physical},
    },
    widget::{button, text},
};
use rc_db::Db;
use rc_engine::{Command, CursorTarget, OfferedKey, PanelId};
use tempfile::TempDir;

use super::{Cards, Focused, Survey};
use crate::app::harness::open_engine;
use crate::{
    app::{Message, harness::Harness},
    theme::{
        focus::{self, Kind},
        mnemonic,
    },
};

/// The window the dialog is laid out in.
const SIZE: Size = Size::new(1280.0, 800.0);

/// One stop of the ring as the probe saw it, in the order Tab reaches them.
#[derive(Debug, Clone, PartialEq)]
pub struct Seen {
    /// The stop's widget id, if it has one.
    pub id: Option<String>,
    /// A stop of `theme::focus` and its kind; `None` for a text field.
    pub kind: Option<Kind>,
    pub focused: bool,
    /// The texts from the stop up to the next one, its label first.
    pub text: String,
    /// The stop's access key (`theme::mnemonic`).
    pub letter: Option<char>,
}

/// A dialog laid out with its widget tree, as the window holds it.
pub struct Screen<'a> {
    element: Element<'a, Message>,
    tree: Tree,
    node: layout::Node,
    renderer: iced::Renderer,
}

impl<'a> Screen<'a> {
    /// `element` in the scope that presses access keys, as each window's content is.
    pub async fn new(element: Element<'a, Message>) -> Self {
        Self::in_scope(mnemonic::Scope::new(element)).await
    }

    /// `scope` as the window's content.
    pub async fn in_scope(scope: mnemonic::Scope<'a, Message>) -> Self {
        let mut element: Element<'a, Message> = scope.into();
        let renderer =
            <iced::Renderer as Headless>::new(Font::DEFAULT, Pixels(13.0), Some("tiny-skia"))
                .await
                .expect("the software renderer");
        let mut tree = Tree::new(element.as_widget());
        let limits = layout::Limits::new(Size::ZERO, SIZE);
        let node = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits);
        Self {
            element,
            tree,
            node,
            renderer,
        }
    }

    fn run(&mut self, operation: &mut dyn Operation) {
        self.element.as_widget_mut().operate(
            &mut self.tree,
            Layout::new(&self.node),
            &self.renderer,
            operation,
        );
    }

    /// The stops of the top card, in reading order.
    pub fn stops(&mut self) -> Vec<Seen> {
        let mut probe = Probe {
            cards: Cards::default(),
            stops: Vec::new(),
        };
        self.run(&mut probe);
        probe.stops.pop().unwrap_or_default()
    }

    /// Tab (`back`: Shift+Tab), as `Effect::WalkFields` runs it.
    pub fn tab(&mut self, back: bool) {
        let mut survey = Survey {
            back,
            cards: Cards::default(),
            fields: Vec::new(),
            focused: None,
        };
        self.run(&mut survey);
        if let Outcome::Chain(mut next) = survey.finish() {
            self.run(next.as_mut());
        }
    }

    /// The id of the stop that has the keyboard, as the connection manager asks for it.
    pub fn focused_id(&mut self) -> Option<Id> {
        let mut focused = Focused {
            cards: Cards::default(),
            found: None,
        };
        {
            let mut blind = operation::black_box::<Option<Id>, ()>(&mut focused);
            self.run(&mut blind);
        }
        match focused.finish() {
            Outcome::Some(id) => id,
            _ => None,
        }
    }

    /// The stop or field with the id `id` gets the keyboard, as `iced::widget::operation::focus`
    /// gives it to a dialog's safe answer when the dialog opens.
    pub fn focus(&mut self, id: &'static str) {
        let mut focus = operation::focusable::focus::<()>(Id::new(id));
        self.run(&mut focus);
    }

    /// Where the keyboard is: the place of the focused stop among `stops()`.
    pub fn at(&mut self) -> Option<usize> {
        self.stops().iter().position(|seen| seen.focused)
    }

    /// `key` pressed: the messages the widgets sent, and whether one of them took the key.
    pub fn press(&mut self, key: Named) -> (Vec<Message>, bool) {
        self.deliver(&Event::Keyboard(keyboard::Event::KeyPressed {
            key: Key::Named(key),
            modified_key: Key::Named(key),
            physical_key: Physical::Unidentified(NativeCode::Unidentified),
            location: keyboard::Location::Standard,
            modifiers: Modifiers::default(),
            text: None,
            repeat: false,
        }))
    }

    /// The letter `letter` typed, with Alt held or alone, as the window delivers it: the
    /// messages the widgets sent, and whether one of them took the key.
    pub fn type_letter(&mut self, letter: char, alt: bool) -> (Vec<Message>, bool) {
        let typed = Key::Character(letter.to_string().into());
        let modifiers = if alt {
            Modifiers::ALT
        } else {
            Modifiers::default()
        };
        self.deliver(&Event::Keyboard(keyboard::Event::KeyPressed {
            key: typed.clone(),
            modified_key: typed,
            physical_key: Physical::Unidentified(NativeCode::Unidentified),
            location: keyboard::Location::Standard,
            modifiers,
            text: Some(letter.to_string().into()),
            repeat: false,
        }))
    }

    /// A click of the left button on the middle of the text `label`, as the window delivers
    /// it: the messages the widgets sent.
    pub fn click(&mut self, label: &str) -> Vec<Message> {
        let mut find = FindText { label, at: None };
        self.run(&mut find);
        let at = find.at.unwrap_or_else(|| panic!("no text {label}"));
        let mut messages = Vec::new();
        for event in [
            mouse::Event::ButtonPressed(mouse::Button::Left),
            mouse::Event::ButtonReleased(mouse::Button::Left),
        ] {
            let cursor = mouse::Cursor::Available(at);
            messages.extend(self.deliver_at(&Event::Mouse(event), cursor).0);
        }
        messages
    }

    fn deliver(&mut self, event: &Event) -> (Vec<Message>, bool) {
        self.deliver_at(event, mouse::Cursor::Unavailable)
    }

    fn deliver_at(&mut self, event: &Event, cursor: mouse::Cursor) -> (Vec<Message>, bool) {
        let mut messages = Vec::new();
        let captured = {
            let mut shell = Shell::new(&mut messages);
            let mut clipboard = clipboard::Null;
            self.element.as_widget_mut().update(
                &mut self.tree,
                event,
                Layout::new(&self.node),
                cursor,
                &self.renderer,
                &mut clipboard,
                &mut shell,
                &Rectangle::with_size(SIZE),
            );
            shell.is_event_captured()
        };
        (messages, captured)
    }

    /// The buttons that are no stop of the ring: the X of each card's head, and any left out.
    pub fn loose_buttons(&self) -> usize {
        let sample: Element<'_, Message> = button(text("x")).into();
        let button = Tree::new(sample.as_widget()).tag;
        loose(&self.tree, false, button, tree::Tag::of::<focus::State>())
    }
}

fn loose(tree: &Tree, inside: bool, button: tree::Tag, stop: tree::Tag) -> usize {
    let inside = inside || tree.tag == stop;
    let own = usize::from(!inside && tree.tag == button);
    own + tree
        .children
        .iter()
        .map(|child| loose(child, inside, button, stop))
        .sum::<usize>()
}

/// The middle of the first text that reads `label`.
struct FindText<'l> {
    label: &'l str,
    at: Option<Point>,
}

impl Operation for FindText<'_> {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }

    fn text(&mut self, _id: Option<&Id>, bounds: Rectangle, text: &str) {
        if self.at.is_none() && text == self.label {
            self.at = Some(bounds.center());
        }
    }
}

/// The stops of each marked card, as Tab counts them, with their kinds and texts.
struct Probe {
    cards: Cards,
    /// The stops of card `n` at `n - 1`.
    stops: Vec<Vec<Seen>>,
}

impl Probe {
    fn last(&mut self) -> Option<&mut Seen> {
        let card = self.cards.inside?;
        self.stops.get_mut(card - 1)?.last_mut()
    }
}

impl Operation for Probe {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        let (outer, card) = self.cards.enter();
        if card {
            self.stops.push(Vec::new());
        }
        operate(self);
        self.cards.inside = outer;
    }

    fn container(&mut self, id: Option<&Id>, _bounds: Rectangle) {
        self.cards.container(id);
    }

    fn focusable(&mut self, id: Option<&Id>, _bounds: Rectangle, state: &mut dyn Focusable) {
        let Some(card) = self.cards.inside else {
            return;
        };
        if let Some(stops) = self.stops.get_mut(card - 1) {
            stops.push(Seen {
                id: id.map(|id| format!("{id:?}")),
                kind: None,
                focused: state.is_focused(),
                text: String::new(),
                letter: None,
            });
        }
    }

    fn custom(&mut self, _id: Option<&Id>, _bounds: Rectangle, state: &mut dyn Any) {
        let stop = state
            .downcast_mut::<focus::State>()
            .map(|stop| (stop.kind(), stop.letter()));
        if let Some((kind, letter)) = stop
            && let Some(seen) = self.last()
        {
            // A field's access key (`Stop::field`): the field stays a text field.
            if kind != Kind::Field {
                seen.kind = Some(kind);
            }
            seen.letter = letter;
        }
    }

    fn text(&mut self, _id: Option<&Id>, _bounds: Rectangle, text: &str) {
        if let Some(seen) = self.last() {
            if !seen.text.is_empty() {
                seen.text.push(' ');
            }
            seen.text.push_str(text);
        }
    }
}

/// Tab from where the keyboard is reaches every stop once, each the one after the last, and
/// comes round; Shift+Tab goes back one; no two stops share an access key. The stops, in their
/// order.
pub fn walk_round(screen: &mut Screen<'_>, dialog: &str) -> Vec<Seen> {
    let stops = screen.stops();
    let count = stops.len();
    assert!(count > 0, "{dialog}: no stop");
    let mut letters: Vec<char> = stops.iter().filter_map(|seen| seen.letter).collect();
    letters.sort_unstable();
    let before = letters.len();
    letters.dedup();
    assert_eq!(
        letters.len(),
        before,
        "{dialog}: an access key twice, {stops:?}"
    );
    let mut visited = Vec::new();
    for _ in 0..count {
        screen.tab(false);
        visited.push(screen.at().expect("a stop has the keyboard"));
    }
    let mut each = visited.clone();
    each.sort_unstable();
    each.dedup();
    assert_eq!(each.len(), count, "{dialog}: every stop once, {visited:?}");
    for pair in visited.windows(2) {
        assert_eq!(pair[1], (pair[0] + 1) % count, "{dialog}: reading order");
    }
    let here = *visited.last().expect("a stop");
    screen.tab(true);
    assert_eq!(
        screen.at(),
        Some((here + count - 1) % count),
        "{dialog}: Shift+Tab"
    );
    screen.tab(false);
    stops
}

/// `key` on the stop at `at`, reached by Tab: the messages it sends; it takes the key and sends
/// exactly one.
pub fn press_at(screen: &mut Screen<'_>, at: usize, key: Named) -> Vec<Message> {
    let count = screen.stops().len();
    for _ in 0..=count {
        if screen.at() == Some(at) {
            break;
        }
        screen.tab(false);
    }
    assert_eq!(screen.at(), Some(at), "Tab reaches stop {at}");
    let (messages, taken) = screen.press(key);
    assert!(taken, "stop {at} takes {key:?}");
    assert_eq!(messages.len(), 1, "stop {at}: {messages:?}");
    messages
}

/// `key` on the first stop - no text field - whose texts contain `label`: the one message it
/// sends.
pub fn press_on(screen: &mut Screen<'_>, label: &str, key: Named) -> Vec<Message> {
    let stops = screen.stops();
    let at = stops
        .iter()
        .position(|seen| seen.kind.is_some() && seen.text.contains(label))
        .unwrap_or_else(|| panic!("no stop {label} in {stops:?}"));
    press_at(screen, at, key)
}

/// Enter on every button and Space on every switch of the ring sends that control's one
/// message, as a native dialog's keys press them.
pub fn press_every_stop(screen: &mut Screen<'_>, stops: &[Seen], dialog: &str) {
    for (at, seen) in stops.iter().enumerate() {
        let key = match seen.kind {
            Some(Kind::Button) => Named::Enter,
            Some(Kind::Toggle) => Named::Space,
            Some(Kind::Choice | Kind::Field) | None => continue,
        };
        let messages = press_at(screen, at, key);
        assert!(!messages.is_empty(), "{dialog}: {seen:?}");
    }
}

pub fn kinds(stops: &[Seen]) -> Vec<Option<Kind>> {
    stops.iter().map(|seen| seen.kind).collect()
}

/// The place of the first stop of `kind`.
pub fn first_of(stops: &[Seen], kind: Kind) -> usize {
    stops
        .iter()
        .position(|seen| seen.kind == Some(kind))
        .unwrap_or_else(|| panic!("no {kind:?} in {stops:?}"))
}

/// An engine on a temporary folder with a database, and the app over it.
pub async fn harness() -> (TempDir, Harness) {
    Harness::temp_with_db().await
}

/// The left panel with `abc.txt` under its cursor, the right one on `out`, a database for the
/// engine and the app: what a dialog on the files under the cursor wants.
pub async fn beside_a_file() -> (TempDir, Harness) {
    let root = tempfile::tempdir().expect("temp dir");
    fs::write(root.path().join("abc.txt"), b"abc").expect("write");
    fs::create_dir(root.path().join("out")).expect("mkdir");
    let engine = open_engine(root.path(), &root.path().join("out")).await;
    let db = Db::open_in_memory().await.expect("db");
    assert!(engine.attach_db(db.clone()));
    let mut harness = Harness::new(engine);
    harness.app.db = Some(db);
    cursor_on(&mut harness, PanelId::Left, "abc.txt").await;
    (root, harness)
}

/// `left/` and `right/` as the panels: `diff.txt` on both sides, differing in its second line
/// and under both cursors; `only.txt` only on the left, `extra.txt` only on the right.
pub async fn two_trees() -> (TempDir, Harness) {
    let root = tempfile::tempdir().expect("temp dir");
    for (side, content) in [("left", "one\ntwo\n"), ("right", "one\nTWO\n")] {
        let dir = root.path().join(side);
        fs::create_dir(&dir).expect("mkdir");
        fs::write(dir.join("diff.txt"), content).expect("write");
    }
    fs::write(root.path().join("left").join("only.txt"), "x").expect("write");
    fs::write(root.path().join("right").join("extra.txt"), "y").expect("write");
    let mut harness = Harness::open(&root.path().join("left"), &root.path().join("right")).await;
    for panel in PanelId::BOTH {
        cursor_on(&mut harness, panel, "diff.txt").await;
    }
    (root, harness)
}

async fn cursor_on(harness: &mut Harness, panel: PanelId, name: &str) {
    harness
        .app
        .engine()
        .dispatch(Command::SetCursor {
            panel: Some(panel),
            target: CursorTarget::Name(name.into()),
        })
        .await
        .expect("cursor");
    harness.settle().await;
}

/// The server of [`host_key_question`].
pub const HOST: &str = "files.example.org";

const ED25519: &str = include_str!("../../../rc-remote/tests/fixtures/known_hosts/ed25519.pub");

/// A connection to [`HOST`] meets a key never seen: the engine asks, and the app shows the
/// question.
pub async fn host_key_question(harness: &mut Harness) {
    let mut fields = ED25519.split_whitespace();
    let key = OfferedKey::from_openssh(fields.next().expect("type"), fields.next().expect("key"))
        .expect("a valid key");
    // Refused until someone answers; the question is what counts here.
    let _ = harness.app.engine().check_host_key(HOST, 2222, key).await;
    harness.settle().await;
    assert!(harness.app.host_keys.shown().is_some(), "the question");
}
