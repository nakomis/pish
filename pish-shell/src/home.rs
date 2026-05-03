use pish_core::{PishWidget, Services};

/// The home screen renders the widget grid and handles tile arrangement.
/// TODO: persist layout, implement edit mode (long-press to rearrange).
pub struct HomeScreen {
    widgets: Vec<Box<dyn PishWidget>>,
    focused: Option<usize>,
}

impl HomeScreen {
    pub fn new() -> Self {
        Self {
            widgets: vec![
                Box::new(widget_clock::ClockWidget::new()),
                Box::new(widget_nakostat::NakostatWidget::new()),
                Box::new(widget_bootboots::BootBootsWidget::new()),
            ],
            focused: None,
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, services: &Services) {
        let tile_size = egui::vec2(320.0, 280.0);
        let focus = self.focused;

        if let Some(i) = focus {
            if ui.button("‹ Back").clicked() {
                self.focused = None;
            }
            self.widgets[i].update(ui, services);
        } else {
            let mut next_focus: Option<usize> = None;
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
                for (i, widget) in self.widgets.iter_mut().enumerate() {
                    let tile_id = egui::Id::new(widget.id());
                    let resp = egui::Frame::new()
                        .fill(egui::Color32::from_rgb(0x1e, 0x1e, 0x2e))
                        .corner_radius(12.0)
                        .show(ui, |ui| {
                            ui.set_min_size(tile_size);
                            ui.set_max_size(tile_size);
                            widget.update(ui, services);
                        });
                    if ui.interact(resp.response.rect, tile_id, egui::Sense::click()).clicked() {
                        next_focus = Some(i);
                    }
                }
            });
            self.focused = next_focus;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_screen_has_no_focus() {
        let screen = HomeScreen::new();
        assert!(screen.focused.is_none());
    }

    #[test]
    fn focus_can_be_set_and_cleared() {
        let mut screen = HomeScreen::new();
        screen.focused = Some(0);
        assert_eq!(screen.focused, Some(0));
        screen.focused = None;
        assert!(screen.focused.is_none());
    }

    #[test]
    fn focus_index_bounds() {
        let screen = HomeScreen::new();
        assert!(screen.focused.unwrap_or(0) < screen.widgets.len() || screen.focused.is_none());
    }
}
