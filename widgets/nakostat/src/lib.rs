use egui::{Align2, Color32, FontId, Vec2};
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
        let rect = ui.available_rect_before_wrap();
        let cx = rect.center().x;
        let cy = rect.center().y;
        let painter = ui.painter();
        painter.text(
            egui::pos2(cx, cy),
            Align2::CENTER_CENTER,
            format!("{:.1}°", self.setpoint),
            FontId::proportional(48.0),
            Color32::WHITE,
        );
        // TODO: replace with rotary_dial::RotaryDial
    }
}
