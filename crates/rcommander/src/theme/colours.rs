//! The colours a person chose for the lists (RC-100-43, after Total Commander's Configuration,
//! Color), laid over the light and the dark tokens, and the contrast check of the Colours tab.
//!
//! The tokens stay `&'static` for every view: a change of the colours builds a look once and
//! keeps it for the rest of the run ([`set_colours`]); a look with no colour chosen is the fixed
//! one of `light.rs` and `dark.rs`, and colours chosen before take the look made then
//! (RC-080-53). A change is rare and small, so the looks of earlier changes are kept rather than
//! freed - one per set of colours chosen in a run.

use std::sync::{Mutex, PoisonError, RwLock};

use iced::Color;
use rc_engine::settings::{COLOUR_TOKENS, colour_value};

use super::{Tokens, dark, light};

/// The colours one look overrides; `None` keeps the look's own.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Colours {
    pub text: Option<Color>,
    pub background: Option<Color>,
    pub stripe: Option<Color>,
    pub cursor: Option<Color>,
    pub cursor_inactive: Option<Color>,
    pub cursor_text: Option<Color>,
    pub marked: Option<Color>,
    pub accent: Option<Color>,
}

impl Colours {
    /// The colours of `values`, one per [`COLOUR_TOKENS`] in its order; a value that does not
    /// read as a colour keeps the look's own.
    #[must_use]
    pub fn from_values(values: &[String]) -> Self {
        let colour = |token: &str| {
            let index = COLOUR_TOKENS.iter().position(|known| *known == token)?;
            parse_hex(values.get(index)?)
        };
        Self {
            text: colour("text"),
            background: colour("background"),
            stripe: colour("stripe"),
            cursor: colour("cursor"),
            cursor_inactive: colour("cursor_inactive"),
            cursor_text: colour("cursor_text"),
            marked: colour("marked"),
            accent: colour("accent"),
        }
    }
}

/// `#rrggbb` (or `#rgb`) as a colour; `None` for anything else and for an empty value.
#[must_use]
pub fn parse_hex(text: &str) -> Option<Color> {
    let value = colour_value(text).filter(|value| !value.is_empty())?;
    let rgb = u32::from_str_radix(value.trim_start_matches('#'), 16).ok()?;
    Some(Color::from_rgb8(
        ((rgb >> 16) & 0xff) as u8,
        ((rgb >> 8) & 0xff) as u8,
        (rgb & 0xff) as u8,
    ))
}

/// The look `dark` or light with the system's `accent` and `colours` laid over it.
#[must_use]
pub fn build(dark: bool, accent: Color, colours: &Colours) -> Tokens {
    let accent = colours.accent.unwrap_or(accent);
    let mut tokens = if dark {
        dark::tokens(accent)
    } else {
        light::tokens(accent)
    };
    if let Some(text) = colours.text {
        tokens.list_text = text;
        tokens.list_text_secondary = Color { a: 0.75, ..text };
        tokens.cursor_text = text;
    }
    let set = |slot: &mut Color, chosen: Option<Color>| {
        if let Some(chosen) = chosen {
            *slot = chosen;
        }
    };
    set(&mut tokens.list_fill, colours.background);
    set(&mut tokens.stripe_fill, colours.stripe);
    set(&mut tokens.cursor_fill, colours.cursor);
    set(&mut tokens.cursor_fill_inactive, colours.cursor_inactive);
    set(&mut tokens.cursor_text, colours.cursor_text);
    set(&mut tokens.marked, colours.marked);
    tokens
}

/// The two looks the chosen colours make - `None` for a look without a chosen colour, which is the
/// fixed one - and the colours they were made from.
struct Chosen {
    colours: [Colours; 2],
    looks: [Option<&'static Tokens>; 2],
}

static CHOSEN: RwLock<Option<Chosen>> = RwLock::new(None);

/// A look made in this run: dark or light, the accent and the colours it was made from.
type Made = (bool, Color, Colours, &'static Tokens);

/// Every look made in this run.
static MADE: Mutex<Vec<Made>> = Mutex::new(Vec::new());

/// The look `dark` or light with `accent` and `colours`: the one made before for them, or a new
/// one kept for the rest of the run.
fn made(dark: bool, accent: Color, colours: Colours) -> &'static Tokens {
    let mut made = MADE.lock().unwrap_or_else(PoisonError::into_inner);
    let before = made
        .iter()
        .find(|(was_dark, was_accent, was, _)| {
            *was_dark == dark && *was_accent == accent && *was == colours
        })
        .map(|(.., tokens)| *tokens);
    if let Some(tokens) = before {
        return tokens;
    }
    let tokens: &'static Tokens = Box::leak(Box::new(build(dark, accent, &colours)));
    made.push((dark, accent, colours, tokens));
    tokens
}

/// The light and the dark colours from now on; the default of both brings back the fixed looks.
pub fn set_colours(colours: [Colours; 2], accent: [Color; 2]) {
    let Ok(mut chosen) = CHOSEN.write() else {
        return;
    };
    if chosen
        .as_ref()
        .is_some_and(|known| known.colours == colours)
    {
        return;
    }
    if colours == [Colours::default(); 2] {
        *chosen = None;
        return;
    }
    let looks = [false, true].map(|dark| {
        let look = colours[usize::from(dark)];
        (look != Colours::default()).then(|| made(dark, accent[usize::from(dark)], look))
    });
    *chosen = Some(Chosen { colours, looks });
}

/// The look the chosen colours make, while any is chosen.
pub(super) fn chosen(dark: bool) -> Option<&'static Tokens> {
    CHOSEN
        .read()
        .ok()?
        .as_ref()
        .and_then(|chosen| chosen.looks[usize::from(dark)])
}

/// `top` drawn over the opaque `base`: a translucent fill as it is seen.
#[must_use]
pub fn over(top: Color, base: Color) -> Color {
    let mix = |high: f32, low: f32| high * top.a + low * (1.0 - top.a);
    Color::from_rgb(mix(top.r, base.r), mix(top.g, base.g), mix(top.b, base.b))
}

/// The contrast ratio of two opaque colours, WCAG 2's definition: 1 to 21.
#[must_use]
pub fn contrast(first: Color, second: Color) -> f32 {
    let (first, second) = (luminance(first), luminance(second));
    let (light, dark) = if first > second {
        (first, second)
    } else {
        (second, first)
    };
    (light + 0.05) / (dark + 0.05)
}

/// The smallest contrast WCAG AA asks of normal text.
pub const MIN_CONTRAST: f32 = 4.5;

fn luminance(colour: Color) -> f32 {
    let channel = |value: f32| {
        if value <= 0.039_28 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(colour.r) + 0.7152 * channel(colour.g) + 0.0722 * channel(colour.b)
}

/// The pairs of a look below [`MIN_CONTRAST`], as i18n keys of what they are: text on the
/// background, on the second background, on the cursor, a marked name on the background.
#[must_use]
pub fn low_contrast(tokens: &Tokens) -> Vec<&'static str> {
    let fill = tokens.list_fill;
    let pairs = [
        ("settings.colours.low_text", tokens.list_text, fill),
        (
            "settings.colours.low_stripe",
            tokens.list_text,
            over(tokens.stripe_fill, fill),
        ),
        (
            "settings.colours.low_cursor",
            tokens.cursor_text,
            over(tokens.cursor_fill, fill),
        ),
        ("settings.colours.low_marked", tokens.marked, fill),
    ];
    pairs
        .into_iter()
        .filter(|(_, text, back)| contrast(*text, *back) < MIN_CONTRAST)
        .map(|(what, ..)| what)
        .collect()
}

#[cfg(test)]
#[path = "colours_tests.rs"]
mod tests;
