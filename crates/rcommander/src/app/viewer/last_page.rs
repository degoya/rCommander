//! Where the last page of a text file starts, kept while the file is open: a page turn, a resize
//! or a page shown again reads the file's end only when the layout changed (RC-080-53).

use std::sync::{Arc, Mutex};

use rc_engine::viewer::{Detected, FileWindow, text};

/// The start of the last page for the encoding and the rows it was worked out for; shared by the
/// page loads of one opened file.
#[derive(Debug, Clone, Default)]
pub struct LastPage(Arc<Mutex<Option<(Detected, usize, u64)>>>);

impl LastPage {
    /// The start of the last `rows` lines of `file` in `detected`, read once per layout.
    pub async fn of(
        &self,
        file: &FileWindow,
        detected: Detected,
        rows: usize,
    ) -> Result<u64, &'static str> {
        if let Some(top) = self.held(detected, rows) {
            return Ok(top);
        }
        let top = text::last_page(file, detected, rows)
            .await
            .map_err(|e| e.code())?;
        if let Ok(mut held) = self.0.lock() {
            *held = Some((detected, rows, top));
        }
        Ok(top)
    }

    fn held(&self, detected: Detected, rows: usize) -> Option<u64> {
        let held = self.0.lock().ok()?;
        (*held)
            .filter(|(was, at, _)| *was == detected && *at == rows)
            .map(|(.., top)| top)
    }
}
