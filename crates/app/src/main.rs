mod app;
mod icons;
mod macos;
mod shaping;
mod statusbar;
mod theme;

fn main() -> eframe::Result {
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_title("rsnap")
        .with_inner_size([1100.0, 760.0])
        .with_min_inner_size([760.0, 520.0])
        .with_fullsize_content_view(true)
        .with_titlebar_shown(false)
        .with_title_shown(false);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../../../assets/icon-512.png")) {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native("rsnap", options, Box::new(|cc| Ok(Box::new(app::App::new(cc)))))
}
