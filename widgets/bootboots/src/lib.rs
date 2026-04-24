use egui::{Color32, ColorImage, TextureHandle, Vec2};
use pish_core::{PishWidget, Services};

/// BootBoots cat camera widget.
/// Currently shows a static slideshow of Mu → Tau → Chi → Kappa with placeholder stats.
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
}

impl PishWidget for BootBootsWidget {
    fn id(&self) -> &'static str { "bootboots" }
    fn title(&self) -> &str { "BootBoots" }
    fn min_size(&self) -> Vec2 { Vec2::new(320.0, 280.0) }

    fn update(&mut self, ui: &mut egui::Ui, _services: &Services) {
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

        // Advance slide on tap
        if ui.rect_contains_pointer(ui.available_rect_before_wrap())
            && ui.input(|i| i.pointer.any_click())
        {
            self.current = (self.current + 1) % self.cats.len();
            self.texture = None;
        }
    }
}
