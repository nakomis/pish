use egui::Vec2;
use pish_core::{PishWidget, Services};
use rotary_dial::RotaryDial;

pub struct NakostatWidget {
    dial: RotaryDial,
    /// True from the moment the widget is focused until the opening touch is released.
    /// While set, setpoint changes are suppressed so the tap-to-open doesn't move the dial.
    waiting_for_release: bool,
}

impl NakostatWidget {
    pub fn new() -> Self {
        Self {
            dial: RotaryDial::new(20.0, 20.0),
            waiting_for_release: false,
        }
    }
}

impl PishWidget for NakostatWidget {
    fn id(&self) -> &'static str { "nakostat" }
    fn title(&self) -> &str { "Heating" }
    fn min_size(&self) -> Vec2 { Vec2::new(300.0, 300.0) }

    fn on_focus(&mut self) {
        self.waiting_for_release = true;
    }

    fn update(&mut self, ui: &mut egui::Ui, _services: &Services) {
        let available = ui.available_size();
        let size = available.min_elem().max(100.0);
        let y_pad = ((available.y - size) / 2.0).max(0.0);

        // Suppress setpoint changes until the opening touch has been fully released.
        let saved = self.dial.setpoint;
        ui.add_space(y_pad);
        ui.vertical_centered(|ui| {
            self.dial.show(ui);
        });
        if self.waiting_for_release {
            self.dial.setpoint = saved;
            if ui.input(|i| !i.pointer.any_down()) {
                self.waiting_for_release = false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nakostat_widget_id() {
        assert_eq!(NakostatWidget::new().id(), "nakostat");
    }

    #[test]
    fn nakostat_widget_title() {
        assert_eq!(NakostatWidget::new().title(), "Heating");
    }

    #[test]
    fn nakostat_default_setpoint() {
        assert_eq!(NakostatWidget::new().dial.setpoint, 20.0);
    }

    #[test]
    fn nakostat_default_current_temp() {
        assert_eq!(NakostatWidget::new().dial.current_temperature, 20.0);
    }

    #[test]
    fn nakostat_on_focus_arms_release_guard() {
        let mut w = NakostatWidget::new();
        assert!(!w.waiting_for_release);
        w.on_focus();
        assert!(w.waiting_for_release);
    }
}
