fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1024.0, 600.0])
            .with_decorations(false)
            .with_resizable(false)
            .with_title("Pish"),
        ..Default::default()
    };
    eframe::run_native(
        "Pish",
        options,
        Box::new(|_cc| Ok(Box::new(pish_shell::App::new()))),
    )
}
