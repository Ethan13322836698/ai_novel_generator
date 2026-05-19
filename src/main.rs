#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod api;
mod app;
mod config;
mod novel;
mod realms;
mod templates;

fn load_icon() -> egui::IconData {
    let bytes = include_bytes!("../assets/icon_1024.png");
    let img = image::load_from_memory(bytes)
        .expect("invalid icon png")
        .into_rgba8();
    let (w, h) = img.dimensions();
    egui::IconData { rgba: img.into_raw(), width: w, height: h }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("AI小说创作工具")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([1000.0, 680.0])
            .with_icon(load_icon()),
        ..Default::default()
    };

    eframe::run_native(
        "AI小说创作工具",
        options,
        Box::new(|cc| {
            app::setup_fonts(&cc.egui_ctx);
            Ok(Box::new(app::NovelApp::new()))
        }),
    )
}
