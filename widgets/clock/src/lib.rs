use egui::{Align2, Color32, FontId, Vec2};
use pish_core::{PishWidget, Services};

pub struct ClockWidget;

impl ClockWidget {
    pub fn new() -> Self { Self }
}

impl PishWidget for ClockWidget {
    fn id(&self) -> &'static str { "clock" }
    fn title(&self) -> &str { "Clock" }
    fn min_size(&self) -> Vec2 { Vec2::new(200.0, 120.0) }

    fn update(&mut self, ui: &mut egui::Ui, services: &Services) {
        let rect = ui.available_rect_before_wrap();
        let cx = rect.center().x;
        let cy = rect.center().y;
        let painter = ui.painter();
        painter.text(
            egui::pos2(cx, cy - 16.0),
            Align2::CENTER_CENTER,
            &services.clock.time,
            FontId::proportional(56.0),
            Color32::WHITE,
        );
        painter.text(
            egui::pos2(cx, cy + 28.0),
            Align2::CENTER_CENTER,
            &services.clock.date,
            FontId::proportional(20.0),
            Color32::from_rgb(0x88, 0x99, 0xaa),
        );
    }
}
