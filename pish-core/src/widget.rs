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
    /// Note: `update()` is intentionally not unit-tested — constructing `egui::Ui`
    /// requires a full rendering context. Verified by running the app locally.
    fn update(&mut self, ui: &mut Ui, services: &Services);

    /// Called by the shell when the widget transitions from tile to full-screen.
    /// Override to reset any interaction state that should not carry over from the opening tap.
    fn on_focus(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockWidget;

    impl PishWidget for MockWidget {
        fn id(&self) -> &'static str { "mock" }
        fn title(&self) -> &str { "Mock Widget" }
        fn min_size(&self) -> Vec2 { Vec2::new(100.0, 80.0) }
        fn update(&mut self, _ui: &mut Ui, _services: &Services) {}
    }

    #[test]
    fn mock_widget_id() {
        assert_eq!(MockWidget.id(), "mock");
    }

    #[test]
    fn mock_widget_title() {
        assert_eq!(MockWidget.title(), "Mock Widget");
    }

    #[test]
    fn mock_widget_min_size() {
        let size = MockWidget.min_size();
        assert_eq!(size.x, 100.0);
        assert_eq!(size.y, 80.0);
    }

    #[test]
    fn mock_widget_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<MockWidget>();
    }
}
