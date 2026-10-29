//! The terminal font and the size of one cell. Taken from iced_term 0.8 (`font.rs`, MIT).

use iced::{Font, Size};
use iced_core::{
    Text,
    alignment::Vertical,
    text::{Alignment, LineHeight, Paragraph, Shaping as TextShaping},
};
use iced_graphics::text::paragraph;

use crate::FontSettings;

#[derive(Debug, Clone)]
pub(crate) struct TermFont {
    pub(crate) size: f32,
    pub(crate) font_type: Font,
    pub(crate) scale_factor: f32,
    /// One cell: the advance of `m` and the line height.
    pub(crate) measure: Size<f32>,
}

impl TermFont {
    pub(crate) fn new(settings: &FontSettings) -> Self {
        Self {
            size: settings.size,
            font_type: settings.font_type,
            scale_factor: settings.scale_factor,
            measure: font_measure(settings.size, settings.scale_factor, settings.font_type),
        }
    }

    /// A cell in whole pixels, at least one by one, as iced_term lays the grid out.
    pub(crate) fn cell(&self) -> Size<f32> {
        Size::new(
            self.measure.width.trunc().max(1.0),
            self.measure.height.trunc().max(1.0),
        )
    }
}

fn font_measure(font_size: f32, scale_factor: f32, font_type: Font) -> Size<f32> {
    let paragraph = paragraph::Paragraph::with_text(Text {
        content: "m",
        font: font_type,
        size: iced_core::Pixels(font_size),
        align_y: Vertical::Center,
        align_x: Alignment::Center,
        shaping: TextShaping::Advanced,
        line_height: LineHeight::Relative(scale_factor),
        bounds: Size::INFINITE,
        wrapping: iced_core::text::Wrapping::Glyph,
    });
    paragraph.min_bounds()
}
