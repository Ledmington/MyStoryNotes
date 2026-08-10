mod app;
mod fonts;
mod graph;
mod logging;
mod markdown;
mod project;
mod settings;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1400.0, 900.0]),
        ..Default::default()
    };

    eframe::run_native(
        "MyStoryNotes",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}
