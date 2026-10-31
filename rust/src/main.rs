mod ui;

fn main() -> eframe::Result {
    let icon = image::load_from_memory(include_bytes!("../../resources/chromakit.ico"))
        .expect("Failed to load ChromaKit icon");
    let icon = eframe::egui::IconData {
        rgba: icon.to_rgba8().into_raw(),
        width: icon.width(),
        height: icon.height(),
    };
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([680.0, 520.0])
            .with_min_inner_size([500.0, 480.0])
            .with_icon(icon),
        ..Default::default()
    };
    eframe::run_native(
        "ChromaKit",
        options,
        Box::new(|creation| Ok(Box::new(ui::ChromaKitApp::new(creation)))),
    )
}
