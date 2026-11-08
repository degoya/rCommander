//! The columns a list draws (RC-100-90): the full view's, whose widths are the settings
//! `columns.*_width`, or a column set's (`ListMode::Custom`) and the comments view's, whose
//! widths are the set's own. Either way each column has a width at the default list font size
//! (the name none: it takes the rest), the least it gives way to, its alignment, its title and
//! what its grip drags. As in Total Commander's custom columns the name always stands first; a
//! set's `name` column only gives it a title.

use rc_engine::{
    Align, ColumnSetSpec, Field,
    columns::{MAX_WIDTH, MIN_WIDTH},
};

use super::columns::{Column, ColumnWidths};
use crate::app::resize::Target;

/// One column as a list draws it.
#[derive(Debug, Clone, PartialEq)]
pub(in crate::app) struct Shown {
    pub column: Column,
    /// Its width at the default list font size; `None` for the name, which takes the rest.
    pub width: Option<f32>,
    /// The least width it gives way to in a narrow panel.
    pub least: f32,
    /// Its values stand at its right edge.
    pub right: bool,
    /// Its own title; the column's name when `None`.
    pub title: Option<String>,
    /// What the grip left of its title drags; `None` for the name.
    pub grip: Option<Target>,
}

/// The full view's `columns` with the person's `widths`.
pub(in crate::app) fn full(columns: &[Column], widths: &ColumnWidths) -> Vec<Shown> {
    columns
        .iter()
        .map(|&column| Shown {
            column,
            width: column.width(widths, 1.0),
            least: column.bounds().0,
            right: column.right(),
            title: None,
            grip: (column != Column::Name).then_some(Target::Column(column)),
        })
        .collect()
}

/// The columns of `spec`, the name first; the widths a grip drags are the set `set`'s (none for
/// the comments view, whose widths are fixed).
pub(in crate::app) fn custom(spec: &ColumnSetSpec, set: Option<i64>) -> Vec<Shown> {
    let name_title = spec
        .columns
        .iter()
        .find(|column| column.field == Field::Name)
        .and_then(|column| column.title.clone());
    let mut shown = vec![Shown {
        column: Column::Name,
        width: None,
        least: 0.0,
        right: false,
        title: name_title,
        grip: None,
    }];
    for (index, column) in spec.columns.iter().enumerate() {
        if column.field == Field::Name {
            continue;
        }
        shown.push(Shown {
            column: column_of(column.field),
            width: Some(f32::from(column.width.clamp(MIN_WIDTH, MAX_WIDTH))),
            least: f32::from(MIN_WIDTH),
            right: column.align == Align::Right,
            title: column.title.clone(),
            grip: set.map(|set| Target::SetColumn { set, index }),
        });
    }
    shown
}

/// The list's column of a set's `field`.
pub(in crate::app) fn column_of(field: Field) -> Column {
    match field {
        Field::Name => Column::Name,
        Field::Ext => Column::Ext,
        Field::Size => Column::Size,
        Field::Packed => Column::Packed,
        Field::Modified => Column::Date,
        Field::Created => Column::Created,
        Field::Accessed => Column::Accessed,
        Field::Attributes => Column::Attributes,
        Field::Rights => Column::Rights,
        Field::Owner => Column::User,
        Field::Group => Column::Group,
        Field::Comment => Column::Comment,
        Field::Git => Column::Git,
    }
}

#[cfg(test)]
#[path = "shown_tests.rs"]
mod tests;
