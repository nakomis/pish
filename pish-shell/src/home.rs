use pish_core::{PishWidget, Services};

/// The home screen renders the widget grid and handles tile arrangement.
/// TODO: persist layout, implement edit mode (long-press to rearrange).
pub struct HomeScreen {
    widgets: Vec<Box<dyn PishWidget>>,
    focused: Option<usize>,
    /// Index of the widget currently animating (valid while t > 0).
    last_focused: usize,
    /// Screen rects of each tile as of the last grid render — used as animation source.
    tile_rects: Vec<egui::Rect>,
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
            last_focused: 0,
            tile_rects: Vec::new(),
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, services: &Services) {
        let ctx = ui.ctx().clone();
        let panel_rect = ui.max_rect();
        let tile_size = egui::vec2(320.0, 280.0);

        let anim_id = egui::Id::new("home_focus");
        let t = ease_out(ctx.animate_bool_with_time(anim_id, self.focused.is_some(), 0.3));

        if self.focused.is_none() && t == 0.0 {
            // Pure grid mode: render tiles and record their screen rects.
            self.tile_rects.resize(self.widgets.len(), egui::Rect::NOTHING);
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
                    self.tile_rects[i] = resp.response.rect;
                    if ui.interact(resp.response.rect, tile_id, egui::Sense::click()).clicked() {
                        next_focus = Some(i);
                    }
                }
            });
            if let Some(i) = next_focus {
                self.widgets[i].on_focus();
                self.focused = Some(i);
                self.last_focused = i;
                ctx.request_repaint();
            }
        } else {
            // Focused or mid-transition.
            let i = self.last_focused;
            let source = self.tile_rects.get(i).copied().unwrap_or(panel_rect);
            let current = lerp_rect(source, panel_rect, t);

            if t >= 1.0 {
                // Fully focused: widget fills the panel, back button floats over it.
                self.widgets[i].update(ui, services);
                let back_label = egui::RichText::new("‹ Back").size(42.0);
                let back_pos = panel_rect.min + egui::vec2(8.0, 8.0);
                let back_clicked = egui::Area::new(egui::Id::new("back_button"))
                    .fixed_pos(back_pos)
                    .order(egui::Order::Foreground)
                    .show(&ctx, |ui| {
                        ui.add(egui::Button::new(back_label).min_size(egui::vec2(180.0, 84.0))).clicked()
                    })
                    .inner;
                if back_clicked {
                    self.focused = None;
                }
            } else {
                // Mid-transition: widget expands from tile rect toward full panel.
                ui.allocate_new_ui(egui::UiBuilder::new().max_rect(current), |ui| {
                    ui.set_min_size(current.size());
                    ui.set_max_size(current.size());
                    self.widgets[i].update(ui, services);
                });
                ctx.request_repaint();
            }
        }
    }
}

/// Interpolate between two screen-space rects.
fn lerp_rect(a: egui::Rect, b: egui::Rect, t: f32) -> egui::Rect {
    egui::Rect {
        min: egui::pos2(
            egui::lerp(a.min.x..=b.min.x, t),
            egui::lerp(a.min.y..=b.min.y, t),
        ),
        max: egui::pos2(
            egui::lerp(a.max.x..=b.max.x, t),
            egui::lerp(a.max.y..=b.max.y, t),
        ),
    }
}

/// Cubic ease-out: fast start, decelerates into the final position.
fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
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

    #[test]
    fn ease_out_bounds() {
        assert_eq!(ease_out(0.0), 0.0);
        assert_eq!(ease_out(1.0), 1.0);
        assert!(ease_out(0.5) > 0.5); // accelerates past linear at t=0.5
    }

    #[test]
    fn lerp_rect_endpoints() {
        let a = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(100.0, 80.0));
        let b = egui::Rect::from_min_size(egui::pos2(200.0, 150.0), egui::vec2(400.0, 300.0));
        assert_eq!(lerp_rect(a, b, 0.0), a);
        assert_eq!(lerp_rect(a, b, 1.0), b);
    }
}
