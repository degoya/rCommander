//! The window's size and position across restarts: followed in memory while the window moves,
//! stored in the setting `window.geometry` when it closes, read before it opens.
//!
//! Only a window that is neither maximized nor minimized is measured, so a maximized window
//! that is restored comes back to the size it had before; the maximized state itself is kept
//! as a flag. Windows parks a minimized window far off screen (-32000), which is never stored.

use iced::{
    Point, Size,
    window::{self, Position},
};
use rc_db::Db;
use serde::{Deserialize, Serialize};

/// The settings key.
pub const GEOMETRY_KEY: &str = "window.geometry";

/// The size of a first start.
const DEFAULT_SIZE: Size = Size::new(1280.0, 800.0);

/// The smallest window that still shows two panels and their columns.
const MIN_SIZE: Size = Size::new(640.0, 400.0);

/// Coordinates, and sizes, beyond this are no screen's.
const FAR: f32 = 16_000.0;

/// What is stored: the inner size, the position of the window's corner when known, and
/// whether it was maximized.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Geometry {
    pub width: f32,
    pub height: f32,
    #[serde(default)]
    pub position: Option<(f32, f32)>,
    #[serde(default)]
    pub maximized: bool,
}

impl Default for Geometry {
    fn default() -> Self {
        Self {
            width: DEFAULT_SIZE.width,
            height: DEFAULT_SIZE.height,
            position: None,
            maximized: false,
        }
    }
}

impl Geometry {
    /// A stored value made safe to open: at least the smallest size and no larger than a screen
    /// can be (RC-080-44), a position only where a screen can be. `None` for a size that is no
    /// number.
    #[must_use]
    pub fn sanitized(self) -> Option<Self> {
        if !self.width.is_finite() || !self.height.is_finite() {
            return None;
        }
        Some(Self {
            width: self.width.clamp(MIN_SIZE.width, FAR),
            height: self.height.clamp(MIN_SIZE.height, FAR),
            position: self.position.filter(|&(x, y)| on_screen(x, y)),
            maximized: self.maximized,
        })
    }

    /// The window's settings for this geometry; centred when there is no position.
    pub fn window_settings(self) -> window::Settings {
        window::Settings {
            size: Size::new(self.width, self.height),
            position: self.position.map_or(Position::Centered, |(x, y)| {
                Position::Specific(Point::new(x, y))
            }),
            maximized: self.maximized,
            min_size: Some(MIN_SIZE),
            ..window::Settings::default()
        }
    }
}

fn on_screen(x: f32, y: f32) -> bool {
    x.is_finite() && y.is_finite() && x.abs() < FAR && y.abs() < FAR
}

/// A report of the window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WindowChange {
    Moved(Point),
    Resized(Size),
}

/// The geometry as the window last reported it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowState {
    geometry: Geometry,
}

impl WindowState {
    /// Starts from what was stored, or from the first-start size.
    pub fn new(stored: Option<Geometry>) -> Self {
        Self {
            geometry: stored.and_then(Geometry::sanitized).unwrap_or_default(),
        }
    }

    /// The window moved or changed size; `maximized` is its state at that moment.
    pub fn observe(&mut self, change: WindowChange, maximized: bool) {
        self.geometry.maximized = maximized;
        if maximized {
            return;
        }
        match change {
            WindowChange::Moved(at) if on_screen(at.x, at.y) => {
                self.geometry.position = Some((at.x, at.y));
            }
            // A minimized window reports a size of nothing.
            WindowChange::Resized(size) if size.width >= 1.0 && size.height >= 1.0 => {
                self.geometry.width = size.width;
                self.geometry.height = size.height;
            }
            WindowChange::Moved(_) | WindowChange::Resized(_) => {}
        }
    }

    pub fn geometry(&self) -> Geometry {
        self.geometry
    }
}

/// The stored geometry; `None` when there is none or it does not read.
pub async fn load(db: &Db) -> Option<Geometry> {
    db.get::<Geometry>(GEOMETRY_KEY).await.ok().flatten()
}

/// Stores `geometry`.
///
/// # Errors
///
/// The database write fails.
pub async fn save(db: &Db, geometry: Geometry) -> rc_db::Result<()> {
    db.set(GEOMETRY_KEY, &geometry).await
}

#[cfg(test)]
#[path = "window_state_tests.rs"]
mod tests;
