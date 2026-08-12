#![forbid(unsafe_code)]

use eframe::egui;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1400.0, 900.0]),
        ..Default::default()
    };

    eframe::run_native(
        "MyStoryNotes",
        options,
        Box::new(|cc| Ok(Box::new(my_story_notes::app::App::new(cc)))),
    )
}
