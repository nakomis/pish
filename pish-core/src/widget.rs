use egui::{Ui, Vec2};
use crate::Services;

/// Trait that every Pish widget must implement.
///
/// Widgets are compiled statically into pish-shell. Each widget owns its own
/// state and is responsible for rendering itself into the `ui` it is given.
pub trait PishWidget: Send {
    /// Stable identifier used to persist layout config. Must be unique across all widgets.
    fn id(&self) -> &'static str;

    /// Human-readable name shown on the home screen.
    fn title(&self) -> &str;

    /// Preferred minimum size in logical pixels. The shell uses this to size grid cells.
    fn min_size(&self) -> Vec2;

    /// Called every frame when the widget is visible. Draw into `ui`.
    fn update(&mut self, ui: &mut Ui, services: &Services);
}
