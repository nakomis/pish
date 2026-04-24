use pish_core::{PishWidget, Services};

/// The home screen renders the widget grid and handles tile arrangement.
/// TODO: persist layout, implement edit mode (long-press to rearrange).
pub struct HomeScreen {
    widgets: Vec<Box<dyn PishWidget>>,
}

impl HomeScreen {
    pub fn new() -> Self {
        Self {
            widgets: vec![
                Box::new(widget_clock::ClockWidget::new()),
                Box::new(widget_nakostat::NakostatWidget::new()),
                Box::new(widget_bootboots::BootBootsWidget::new()),
            ],
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, services: &Services) {
        // Simple uniform grid for now — each widget gets an equal tile.
        let tile_size = egui::vec2(320.0, 280.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
            for widget in &mut self.widgets {
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(0x1e, 0x1e, 0x2e))
                    .corner_radius(12.0)
                    .show(ui, |ui| {
                        ui.set_min_size(tile_size);
                        ui.set_max_size(tile_size);
                        widget.update(ui, services);
                    });
            }
        });
    }
}
