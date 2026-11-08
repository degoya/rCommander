//! The running text of WordprocessingML (a Word body) and DrawingML (a slide's text frames): both
//! write paragraphs `p` of runs `r` with their text in `t`, a tab inside a run as `tab`, a line
//! break as `br` (or `cr`) and tables as `tbl` of rows `tr` of cells `tc`. A paragraph is a line;
//! a table row is a line of its cells separated by tabs, the paragraphs of one cell joined by a
//! space. A table inside a cell stays inside that cell. What `mc:Fallback` repeats for older
//! readers is skipped, so a text box is read once; `delText` and `instrText` (deleted text, field
//! codes) are not `t` and stay out.

use crate::out::Lines;
use crate::types::Failure;
use crate::xml::{Ev, local, local_end};

/// The state between two events.
#[derive(Default)]
pub(crate) struct Flow {
    depth: usize,
    /// The depth of an `mc:Fallback` being skipped.
    skip: Option<usize>,
    in_text: bool,
    runs: usize,
    tables: usize,
    line: String,
    cell: String,
    cells: Vec<String>,
}

impl Flow {
    /// Takes one event; each finished line goes to `out`.
    pub(crate) fn event(&mut self, ev: &Ev<'_>, out: &mut dyn Lines) -> Result<(), Failure> {
        match ev {
            Ev::Start(start) => {
                self.depth += 1;
                if self.skip.is_some() {
                    return Ok(());
                }
                match local(start) {
                    b"Fallback" => self.skip = Some(self.depth),
                    b"t" => self.in_text = true,
                    b"r" => self.runs += 1,
                    b"tbl" => self.tables += 1,
                    _ => {}
                }
            }
            Ev::End(end) => {
                let depth = self.depth;
                self.depth = self.depth.saturating_sub(1);
                if let Some(skip) = self.skip {
                    if depth <= skip {
                        self.skip = None;
                    }
                    return Ok(());
                }
                match local_end(end) {
                    b"t" => self.in_text = false,
                    b"r" => self.runs = self.runs.saturating_sub(1),
                    b"p" => self.end_paragraph(out)?,
                    b"tc" => self.end_cell(),
                    b"tr" => self.end_row(out)?,
                    b"tbl" => self.tables = self.tables.saturating_sub(1),
                    _ => {}
                }
            }
            Ev::Empty(start) => {
                if self.skip.is_some() {
                    return Ok(());
                }
                match local(start) {
                    b"tab" if self.runs > 0 => self.push("\t"),
                    b"br" | b"cr" => self.push("\n"),
                    b"p" => self.end_paragraph(out)?,
                    _ => {}
                }
            }
            Ev::Text(text) => {
                if self.skip.is_none() && self.in_text {
                    self.push(text);
                }
            }
            Ev::Other | Ev::Eof => {}
        }
        Ok(())
    }

    /// Writes a paragraph the part left open.
    pub(crate) fn finish(&mut self, out: &mut dyn Lines) -> Result<(), Failure> {
        if !self.line.is_empty() {
            self.line.push('\n');
            out.push(&self.line)?;
            self.line.clear();
        }
        Ok(())
    }

    fn push(&mut self, text: &str) {
        if self.tables > 0 {
            // A cell is one field of its row: its tabs and line breaks become spaces.
            self.cell.extend(text.chars().map(|character| {
                if matches!(character, '\t' | '\n' | '\r') {
                    ' '
                } else {
                    character
                }
            }));
        } else {
            self.line.push_str(text);
        }
    }

    fn space_in_cell(&mut self) {
        if !self.cell.is_empty() && !self.cell.ends_with(' ') {
            self.cell.push(' ');
        }
    }

    fn end_paragraph(&mut self, out: &mut dyn Lines) -> Result<(), Failure> {
        if self.tables > 0 {
            self.space_in_cell();
            return Ok(());
        }
        self.line.push('\n');
        out.push(&self.line)?;
        self.line.clear();
        Ok(())
    }

    fn end_cell(&mut self) {
        match self.tables {
            0 => {}
            1 => {
                self.cells.push(self.cell.trim().to_owned());
                self.cell.clear();
            }
            _ => self.space_in_cell(),
        }
    }

    fn end_row(&mut self, out: &mut dyn Lines) -> Result<(), Failure> {
        if self.tables != 1 {
            return Ok(());
        }
        let mut row = self.cells.join("\t");
        row.push('\n');
        self.cells.clear();
        out.push(&row)
    }
}

#[cfg(test)]
#[path = "flow_tests.rs"]
mod tests;
