//! `panel.thumbnail_zoom` (RC-100-89): Ctrl and the mouse wheel over the thumbnails make the tiles
//! a step larger or smaller. Total Commander zooms by a percentage of the size set
//! (`ThumbScaleFactor`); rCommander writes the size itself in steps - one setting instead of two.

use serde_json::Value;

use crate::{
    Engine, EngineError, Outcome, ZoomStep,
    settings::{DEFAULT_THUMBNAIL_SIZE, THUMBNAIL_SIZE_KEY, stored_setting},
};

/// The tile sizes the zoom steps through, in pixels.
pub(crate) const ZOOM_STEPS: [i64; 7] = [64, 96, 128, 192, 256, 384, 512];

impl Engine {
    /// `panel.thumbnail_zoom`: `thumbnails.size` to the next step up or down.
    ///
    /// # Errors
    ///
    /// No database is attached, or it fails.
    pub(crate) async fn thumbnail_zoom(&self, step: ZoomStep) -> Result<Outcome, EngineError> {
        let now = stored_setting(self.db()?, THUMBNAIL_SIZE_KEY)
            .await
            .as_ref()
            .and_then(Value::as_i64)
            .unwrap_or(DEFAULT_THUMBNAIL_SIZE);
        let next = Value::from(zoomed(now, step));
        self.set_setting(THUMBNAIL_SIZE_KEY, &next).await
    }
}

/// The step after `size` towards `step`; a size beyond the last step stays where it is.
#[must_use]
pub(crate) fn zoomed(size: i64, step: ZoomStep) -> i64 {
    let next = match step {
        ZoomStep::In => ZOOM_STEPS.iter().find(|candidate| **candidate > size),
        ZoomStep::Out => ZOOM_STEPS.iter().rev().find(|candidate| **candidate < size),
    };
    next.copied().unwrap_or(size)
}

#[cfg(test)]
#[path = "thumbnails_tests.rs"]
mod tests;
