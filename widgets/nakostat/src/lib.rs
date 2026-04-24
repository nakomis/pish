use egui::{Color32, FontId, RichText, Vec2};
use pish_core::{PishWidget, Services};

/// Nakostat thermostat widget.
/// TODO: wire rotary-dial crate, MQTT setpoint, live sensor data.
pub struct NakostatWidget {
    setpoint: f32,
}

impl NakostatWidget {
    pub fn new() -> Self {
        Self { setpoint: 20.0 }
    }
}

impl PishWidget for NakostatWidget {
    fn id(&self) -> &'static str { "nakostat" }
    fn title(&self) -> &str { "Heating" }
    fn min_size(&self) -> Vec2 { Vec2::new(300.0, 300.0) }

    fn update(&mut self, ui: &mut egui::Ui, _services: &Services) {
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() / 4.0);
            ui.label(
                RichText::new(format!("{:.1}°", self.setpoint))
                    .font(FontId::proportional(48.0))
                    .color(Color32::WHITE),
            );
            ui.label(
                RichText::new("Heating")
                    .font(FontId::proportional(14.0))
                    .color(Color32::from_rgb(0x88, 0x99, 0xaa)),
            );
        });
        // TODO: replace with rotary_dial::RotaryDial
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
        let w = NakostatWidget::new();
        assert_eq!(w.setpoint, 20.0);
    }
}
