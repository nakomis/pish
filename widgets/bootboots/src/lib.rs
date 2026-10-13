use egui::{Color32, ColorImage, TextureHandle, Vec2};
use pish_core::{PishWidget, Services};

/// BootBoots cat camera widget.
/// Currently shows a static slideshow of Mu → Tau → Chi → Kappa with placeholder stats.
/// Tapping the widget advances to the next embedded photo, wrapping back to the first.
/// TODO: fetch live captures from the BootBoots API.
pub struct BootBootsWidget {
    cats: Vec<CatSlide>,
    current: usize,
    texture: Option<TextureHandle>,
}

struct CatSlide {
    name: &'static str,
    image_bytes: &'static [u8],
}

// Cat images embedded at compile time from the bootboots repo.
// Order by age: Mu, Tau, Chi, Kappa.
impl BootBootsWidget {
    pub fn new() -> Self {
        Self {
            cats: vec![
                CatSlide { name: "Mu",    image_bytes: include_bytes!("../assets/mu.jpeg") },
                CatSlide { name: "Tau",   image_bytes: include_bytes!("../assets/tau.jpg") },
                CatSlide { name: "Chi",   image_bytes: include_bytes!("../assets/chi.jpg") },
                CatSlide { name: "Kappa", image_bytes: include_bytes!("../assets/kappa.jpg") },
            ],
            current: 0,
            texture: None,
        }
    }

    /// Index of the photo currently on display.
    pub fn current_index(&self) -> usize {
        self.current
    }

    /// Name of the cat currently on display.
    pub fn current_name(&self) -> &'static str {
        self.cats[self.current].name
    }

    /// Number of embedded photos in the slideshow.
    pub fn slide_count(&self) -> usize {
        self.cats.len()
    }

    /// Advance to the next photo, wrapping round to the first after the last.
    /// Drops the cached texture so the new photo is loaded on the next frame.
    pub fn advance(&mut self) {
        self.current = next_index(self.current, self.cats.len());
        self.texture = None;
    }
}

impl Default for BootBootsWidget {
    fn default() -> Self {
        Self::new()
    }
}

/// Index following `current` in a slideshow of `len` slides, wrapping at the end.
/// An empty slideshow always yields 0.
fn next_index(current: usize, len: usize) -> usize {
    if len == 0 {
        0
    } else {
        (current + 1) % len
    }
}

impl PishWidget for BootBootsWidget {
    fn id(&self) -> &'static str { "bootboots" }
    fn title(&self) -> &str { "BootBoots" }
    fn min_size(&self) -> Vec2 { Vec2::new(320.0, 280.0) }

    fn update(&mut self, ui: &mut egui::Ui, _services: &Services) {
        // Capture the whole widget area before laying out content, so a tap anywhere
        // on the widget (not just the leftover space beneath the label) advances.
        let tap_rect = ui.max_rect();
        let slide = &self.cats[self.current];

        // Load / reload texture when slide changes
        if self.texture.is_none() {
            if let Ok(img) = image::load_from_memory(slide.image_bytes) {
                let rgba = img.to_rgba8();
                let (w, h) = rgba.dimensions();
                let color_image = ColorImage::from_rgba_unmultiplied(
                    [w as usize, h as usize],
                    rgba.as_raw(),
                );
                self.texture = Some(ui.ctx().load_texture(
                    slide.name,
                    color_image,
                    Default::default(),
                ));
            }
        }

        ui.vertical_centered(|ui| {
            if let Some(tex) = &self.texture {
                let available = ui.available_size();
                let img_h = available.y - 48.0;
                ui.image((tex.id(), Vec2::new(img_h * tex.aspect_ratio(), img_h)));
            }

            ui.label(
                egui::RichText::new(format!("{} — 5,234 captures this month", slide.name))
                    .color(Color32::from_rgb(0x88, 0x99, 0xaa))
                    .size(14.0),
            );
        });

        // Advance slide on tap. `interact` respects layering and disabled state, so taps
        // on overlays (e.g. the back button) or in tile view do not cycle the photo.
        let tap_id = ui.id().with("bootboots_tap");
        let tap = ui.interact(tap_rect, tap_id, egui::Sense::click());
        if tap.clicked() {
            self.advance();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_index_advances_and_wraps() {
        assert_eq!(next_index(0, 4), 1);
        assert_eq!(next_index(2, 4), 3);
        assert_eq!(next_index(3, 4), 0);
    }

    #[test]
    fn next_index_single_slide_stays_put() {
        assert_eq!(next_index(0, 1), 0);
    }

    #[test]
    fn next_index_empty_slideshow_is_zero() {
        assert_eq!(next_index(0, 0), 0);
    }

    #[test]
    fn starts_on_first_cat() {
        let w = BootBootsWidget::new();
        assert_eq!(w.current_index(), 0);
        assert_eq!(w.current_name(), "Mu");
        assert_eq!(w.slide_count(), 4);
    }

    #[test]
    fn advance_cycles_through_all_cats_and_wraps() {
        let mut w = BootBootsWidget::new();
        let seen: Vec<_> = (0..5)
            .map(|_| {
                let name = w.current_name();
                w.advance();
                name
            })
            .collect();
        assert_eq!(seen, ["Mu", "Tau", "Chi", "Kappa", "Mu"]);
        assert_eq!(w.current_index(), 1);
    }

    #[test]
    fn embedded_images_decode() {
        let w = BootBootsWidget::new();
        for slide in &w.cats {
            assert!(
                image::load_from_memory(slide.image_bytes).is_ok(),
                "{} image failed to decode",
                slide.name
            );
        }
    }
}
