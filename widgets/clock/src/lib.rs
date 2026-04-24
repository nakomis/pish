use egui::{Color32, FontId, RichText, Vec2};
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
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() / 4.0);
            ui.label(
                RichText::new(&services.clock.time)
                    .font(FontId::proportional(56.0))
                    .color(Color32::WHITE),
            );
            ui.label(
                RichText::new(&services.clock.date)
                    .font(FontId::proportional(20.0))
                    .color(Color32::from_rgb(0x88, 0x99, 0xaa)),
            );
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_widget_id() {
        assert_eq!(ClockWidget.id(), "clock");
    }

    #[test]
    fn clock_widget_title() {
        assert_eq!(ClockWidget.title(), "Clock");
    }

    #[test]
    fn clock_widget_min_size() {
        let s = ClockWidget.min_size();
        assert!(s.x >= 200.0);
        assert!(s.y >= 120.0);
    }
}
