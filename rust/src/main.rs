mod ui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([680.0, 520.0])
            .with_min_inner_size([500.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(
        "ChromaKit",
        options,
        Box::new(|creation| Ok(Box::new(ui::ChromaKitApp::new(creation)))),
    )
}
