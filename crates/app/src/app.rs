//! The window: a home screen (capture, permissions, updates) and the editor
//! (toolbar, canvas, frame style). The canvas shows the real render, the
//! same pixels ⌘C copies.

use crate::icons::{self, Svg};
use crate::statusbar::{MenuEvent, StatusBar};
use crate::{macos, shaping, theme};
use eframe::egui::{
    self, Align, Color32, CornerRadius, FontId, Frame, Id, Key, Layout, Margin, Modifiers, Pos2, Rect, RichText,
    Sense, Stroke, TextureHandle, Ui, text::LayoutJob, vec2,
};
use image::RgbaImage;
use rsnap_core::doc::{Annotation, BACKDROPS, Backdrop, Bounds, COLORS, Doc, Pt, Shape};
use rsnap_core::render::{self, Layout as ShotLayout};
use rsnap_core::settings::{self, Settings};
use rsnap_core::text::Fonts;
use rsnap_core::update::{self, Release};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tool {
    Arrow,
    Rect,
    Highlight,
    Redact,
    Step,
    Text,
    Crop,
}

const TOOLS: [(Tool, Svg, &str, Key); 7] = [
    (Tool::Arrow, Svg::Arrow, "Arrow  A", Key::A),
    (Tool::Rect, Svg::Rect, "Box  R", Key::R),
    (Tool::Highlight, Svg::Highlight, "Highlight  H", Key::H),
    (Tool::Redact, Svg::Redact, "Redact  B", Key::B),
    (Tool::Step, Svg::Step, "Numbered step  N", Key::N),
    (Tool::Text, Svg::Text, "Text  T", Key::T),
    (Tool::Crop, Svg::Crop, "Crop  C", Key::C),
];

/// A screenshot being edited.
struct Shot {
    src: Arc<RgbaImage>,
    doc: Doc,
    undo: Vec<Doc>,
    redo: Vec<Doc>,
    texture: Option<TextureHandle>,
    /// The doc the texture shows, to know when to re-render.
    shown: Option<Doc>,
    layout: ShotLayout,
}

enum Done {
    Captured(Option<RgbaImage>),
    UpdateChecked(Result<Option<Release>, String>),
    UpdateInstalled(Result<PathBuf, String>),
}

enum UpdateState {
    Idle,
    Checking,
    Available(Release),
    Installing,
    Failed(String),
}

pub struct App {
    settings: Settings,
    fonts: Option<Arc<Fonts>>,
    shot: Option<Shot>,
    tool: Tool,
    /// A drag in progress, in source pixels.
    drag: Option<(Pt, Pt)>,
    /// A text annotation being typed, at a source position.
    typing: Option<(Pt, String)>,
    hotkey_rx: Receiver<()>,
    hotkey_ok: bool,
    status: Option<(StatusBar, Receiver<MenuEvent>)>,
    done_tx: Sender<Done>,
    done_rx: Receiver<Done>,
    update: UpdateState,
    toast: Option<(String, f64)>,
    screen_ok: bool,
    screen_checked: f64,
    capturing: bool,
    quitting: bool,
    /// Debug builds: RSNAP_DEMO_READY, a file to create once a scripted
    /// scene has drawn (scripts/screenshots.sh).
    demo_ready: Option<PathBuf>,
    frames: u32,
}

impl App {
    pub fn new(cc: &eframe::CreationContext) -> App {
        theme::init(&cc.egui_ctx);
        let (hotkey_tx, hotkey_rx) = channel();
        let hotkey_ok = macos::register_hotkey(hotkey_tx, cc.egui_ctx.clone());
        let (done_tx, done_rx) = channel();
        let demo = |k: &str| std::env::var(k).ok().filter(|_| cfg!(debug_assertions));
        let settings = settings::load();
        let mut app = App {
            fonts: Fonts::system().map(Arc::new),
            shot: None,
            tool: Tool::Arrow,
            drag: None,
            typing: None,
            hotkey_rx,
            hotkey_ok,
            status: if demo("RSNAP_DEMO_READY").is_some() { None } else { StatusBar::new(&cc.egui_ctx) },
            done_tx,
            done_rx,
            update: UpdateState::Idle,
            toast: None,
            screen_ok: macos::screen_access(),
            screen_checked: 0.0,
            capturing: false,
            quitting: false,
            demo_ready: demo("RSNAP_DEMO_READY").map(Into::into),
            frames: 0,
            settings,
        };
        // RSNAP_DEMO_IMAGE opens a shot; RSNAP_DEMO_ANNOTATE adds a sample set.
        if let Some(path) = demo("RSNAP_DEMO_IMAGE")
            && let Ok(img) = image::open(&path)
        {
            app.open_shot(img.to_rgba8());
            if demo("RSNAP_DEMO_ANNOTATE").is_some()
                && let Some(shot) = &mut app.shot
            {
                shot.doc.annotations = sample_annotations(shot.src.width() as f32, shot.src.height() as f32);
            }
        }
        app
    }

    fn notify(&mut self, text: impl Into<String>, now: f64) {
        self.toast = Some((text.into(), now + 3.5));
    }

    fn save_settings(&mut self) {
        let _ = settings::save(&self.settings);
    }

    fn open_shot(&mut self, img: RgbaImage) {
        let doc = Doc { style: self.settings.style, ..Doc::default() };
        let layout = render::layout(img.width(), img.height(), &doc);
        self.shot = Some(Shot { src: Arc::new(img), doc, undo: Vec::new(), redo: Vec::new(), texture: None, shown: None, layout });
        self.drag = None;
        self.typing = None;
    }

    /// Change the doc with undo.
    fn edit(&mut self, f: impl FnOnce(&mut Doc)) {
        if let Some(shot) = &mut self.shot {
            shot.undo.push(shot.doc.clone());
            shot.redo.clear();
            f(&mut shot.doc);
        }
    }

    fn undo(&mut self) {
        if let Some(shot) = &mut self.shot
            && let Some(prev) = shot.undo.pop()
        {
            shot.redo.push(std::mem::replace(&mut shot.doc, prev));
        }
    }

    fn redo(&mut self) {
        if let Some(shot) = &mut self.shot
            && let Some(next) = shot.redo.pop()
        {
            shot.undo.push(std::mem::replace(&mut shot.doc, next));
        }
    }

    /// Style changes also become the default for the next shot.
    fn set_style(&mut self, f: impl FnOnce(&mut rsnap_core::doc::Style)) {
        let mut style = self.shot.as_ref().map(|s| s.doc.style).unwrap_or(self.settings.style);
        f(&mut style);
        self.edit(|d| d.style = style);
        self.settings.style = style;
        self.save_settings();
    }

    fn start_capture(&mut self, ctx: &egui::Context) {
        if self.capturing {
            return;
        }
        if !macos::screen_access() {
            macos::ask_for_screen_access();
            self.screen_ok = false;
            macos::activate();
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            return;
        }
        self.capturing = true;
        self.typing = None;
        macos::hide();
        let (tx, ctx) = (self.done_tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            // Let the window get out of the way first.
            std::thread::sleep(std::time::Duration::from_millis(250));
            let _ = tx.send(Done::Captured(macos::capture_interactive()));
            ctx.request_repaint();
        });
    }

    fn handle_events(&mut self, ctx: &egui::Context, now: f64) {
        while self.hotkey_rx.try_recv().is_ok() {
            self.start_capture(ctx);
        }
        let menu: Vec<MenuEvent> = self.status.as_ref().map(|(_, rx)| rx.try_iter().collect()).unwrap_or_default();
        for event in menu {
            match event {
                MenuEvent::Capture => self.start_capture(ctx),
                MenuEvent::Open => {
                    macos::activate();
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                MenuEvent::Quit => {
                    self.quitting = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
        while let Ok(done) = self.done_rx.try_recv() {
            match done {
                Done::Captured(img) => {
                    self.capturing = false;
                    if let Some(img) = img {
                        let ppp = ctx.pixels_per_point();
                        let (w, h) = (img.width() as f32 / ppp, img.height() as f32 / ppp);
                        self.open_shot(img);
                        let size = vec2((w * 1.18 + 300.0).clamp(820.0, 1500.0), (h * 1.18 + 150.0).clamp(560.0, 960.0));
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
                        macos::activate();
                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    }
                }
                Done::UpdateChecked(Ok(Some(r))) => self.update = UpdateState::Available(r),
                Done::UpdateChecked(Ok(None)) => {
                    self.update = UpdateState::Idle;
                    self.notify(format!("rsnap {} is up to date", update::CURRENT), now);
                }
                Done::UpdateChecked(Err(e)) | Done::UpdateInstalled(Err(e)) => {
                    self.notify(e.clone(), now);
                    self.update = UpdateState::Failed(e);
                }
                Done::UpdateInstalled(Ok(app)) => match update::relaunch(&app) {
                    Ok(()) => {
                        self.quitting = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    Err(e) => self.update = UpdateState::Failed(e),
                },
            }
        }
    }

    fn check_update(&mut self, ctx: &egui::Context) {
        self.update = UpdateState::Checking;
        let (tx, ctx) = (self.done_tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            let _ = tx.send(Done::UpdateChecked(update::check()));
            ctx.request_repaint();
        });
    }

    fn install_update(&mut self, ctx: &egui::Context) {
        let UpdateState::Available(release) = &self.update else { return };
        let release = release.clone();
        let Some(app) = update::current_bundle() else {
            macos::open_url(&release.page_url);
            return;
        };
        self.update = UpdateState::Installing;
        let (tx, ctx) = (self.done_tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            let result = update::download(&release, &std::env::temp_dir()).and_then(|zip| update::install(&zip, &app).map(|_| app));
            let _ = tx.send(Done::UpdateInstalled(result));
            ctx.request_repaint();
        });
    }

    /// Render the current doc and put the PNG on the clipboard.
    fn copy(&mut self, now: f64) {
        self.commit_text();
        let Some(shot) = &self.shot else { return };
        let pm = render::render(&shot.src, &shot.doc, self.fonts.as_deref());
        match render::to_png(&pm) {
            Ok(png) => {
                macos::copy_png(&png);
                if self.settings.sound {
                    macos::play("Pop");
                }
                self.notify("Copied. Ready to paste", now);
                macos::hide();
            }
            Err(e) => self.notify(format!("Couldn't copy: {e}"), now),
        }
    }

    fn save_file(&mut self, now: f64) {
        self.commit_text();
        let Some(shot) = &self.shot else { return };
        let name = format!("rsnap {}.png", stamp());
        let dir = std::env::var("HOME").map(|h| PathBuf::from(h).join("Desktop")).unwrap_or_default();
        let Some(path) = rfd::FileDialog::new().set_file_name(&name).set_directory(dir).add_filter("PNG", &["png"]).save_file() else { return };
        let pm = render::render(&shot.src, &shot.doc, self.fonts.as_deref());
        let result = render::to_png(&pm).and_then(|png| std::fs::write(&path, png).map_err(|e| e.to_string()));
        match result {
            Ok(()) => self.notify(format!("Saved {}", path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()), now),
            Err(e) => self.notify(format!("Couldn't save: {e}"), now),
        }
    }

    fn commit_text(&mut self) {
        if let Some((at, text)) = self.typing.take()
            && !text.trim().is_empty()
        {
            let color = COLORS[self.settings.color.min(COLORS.len() - 1)];
            self.edit(|d| d.annotations.push(Annotation { shape: Shape::Text { at, text }, color }));
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context, now: f64) {
        if self.typing.is_some() {
            return;
        }
        let typing_elsewhere = ctx.memory(|m| m.focused().is_some());
        let pressed = |ctx: &egui::Context, m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_key(m, k));
        if pressed(ctx, Modifiers::COMMAND | Modifiers::SHIFT, Key::Z) {
            self.redo();
        } else if pressed(ctx, Modifiers::COMMAND, Key::Z) {
            self.undo();
        }
        if pressed(ctx, Modifiers::COMMAND, Key::C) || (!typing_elsewhere && pressed(ctx, Modifiers::NONE, Key::Enter)) {
            self.copy(now);
        }
        if pressed(ctx, Modifiers::COMMAND, Key::S) {
            self.save_file(now);
        }
        if pressed(ctx, Modifiers::COMMAND, Key::N) {
            self.start_capture(ctx);
        }
        if typing_elsewhere {
            return;
        }
        for (tool, _, _, key) in TOOLS {
            if pressed(ctx, Modifiers::NONE, key) {
                self.tool = tool;
            }
        }
        for (i, key) in [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6].into_iter().enumerate() {
            if pressed(ctx, Modifiers::NONE, key) {
                self.settings.color = i;
                self.save_settings();
            }
        }
        if pressed(ctx, Modifiers::NONE, Key::Escape) {
            self.drag = None;
            macos::hide();
        }
    }

    // ---- Panels ----

    fn toolbar(&mut self, ui: &mut Ui, now: f64) {
        let drag = ui.interact(ui.max_rect(), Id::new("titlebar"), Sense::click_and_drag());
        if drag.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for (tool, svg, tip, _) in TOOLS {
                if tool_button(ui, svg, tip, self.tool == tool).clicked() {
                    self.commit_text();
                    self.tool = tool;
                }
            }
            ui.add_space(14.0);
            for (i, c) in COLORS.iter().enumerate() {
                let color = Color32::from_rgb(c[0], c[1], c[2]);
                if color_dot(ui, color, self.settings.color == i).on_hover_text(format!("Color  {}", i + 1)).clicked() {
                    self.settings.color = i;
                    self.save_settings();
                }
            }
            ui.add_space(14.0);
            let can_undo = self.shot.as_ref().is_some_and(|s| !s.undo.is_empty());
            let can_redo = self.shot.as_ref().is_some_and(|s| !s.redo.is_empty());
            if icon_button(ui, Svg::Undo, "Undo  ⌘Z", can_undo).clicked() {
                self.undo();
            }
            if icon_button(ui, Svg::Redo, "Redo  ⇧⌘Z", can_redo).clicked() {
                self.redo();
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let copy = egui::Button::image_and_text(icon_image(ui, Svg::Copy, theme::BG()), RichText::new("Copy").color(theme::BG()).strong())
                    .fill(theme::ACCENT())
                    .corner_radius(7);
                if ui.add(copy).on_hover_text("Copy to the clipboard  ⌘C or ↩").clicked() {
                    self.copy(now);
                }
                if icon_button(ui, Svg::Save, "Save as PNG…  ⌘S", true).clicked() {
                    self.save_file(now);
                }
                if icon_button(ui, Svg::Camera, "New capture  ⌘N or ⌥⇧S", true).clicked() {
                    self.start_capture(ui.ctx());
                }
            });
        });
    }

    fn style_panel(&mut self, ui: &mut Ui) {
        let Some(style) = self.shot.as_ref().map(|s| s.doc.style) else { return };
        ui.add_space(6.0);
        section(ui, "Backdrop");
        egui::Grid::new("backdrops").spacing(vec2(8.0, 8.0)).show(ui, |ui| {
            for (i, (name, b)) in BACKDROPS.iter().enumerate() {
                if backdrop_swatch(ui, *b, style.backdrop == *b).on_hover_text(*name).clicked() {
                    let b = *b;
                    self.set_style(|s| s.backdrop = b);
                }
                if i % 3 == 2 {
                    ui.end_row();
                }
            }
        });
        ui.add_space(12.0);
        let framed = style.backdrop != Backdrop::None;
        section(ui, "Padding");
        let mut padding = style.padding;
        if ui.add_enabled(framed, egui::Slider::new(&mut padding, 0.0..=0.3).show_value(false)).changed() {
            self.set_style(|s| s.padding = padding);
        }
        section(ui, "Corners");
        let mut radius = style.radius;
        if ui.add(egui::Slider::new(&mut radius, 0.0..=0.08).show_value(false)).changed() {
            self.set_style(|s| s.radius = radius);
        }
        let mut shadow = style.shadow;
        if ui.add_enabled(framed, egui::Checkbox::new(&mut shadow, "Shadow")).changed() {
            self.set_style(|s| s.shadow = shadow);
        }
        ui.add_space(16.0);
        ui.separator();
        if let Some(shot) = &self.shot {
            let lay = shot.layout;
            ui.label(RichText::new(format!("{} × {} px", lay.width, lay.height)).small().color(theme::FAINT()));
            let cropped = shot.doc.crop.is_some();
            let marks = !shot.doc.annotations.is_empty();
            if cropped && ui.small_button("Undo crop").clicked() {
                self.edit(|d| d.crop = None);
            }
            if marks && ui.small_button("Clear marks").clicked() {
                self.edit(|d| d.annotations.clear());
            }
        }
    }

    fn canvas(&mut self, ui: &mut Ui, now: f64) {
        let ctx = ui.ctx().clone();
        let fonts = self.fonts.clone();
        let Some(shot) = &mut self.shot else { return };
        // Re-render when the doc changed.
        if shot.shown.as_ref() != Some(&shot.doc) {
            let pm = render::render(&shot.src, &shot.doc, fonts.as_deref());
            let image = egui::ColorImage::from_rgba_premultiplied([pm.width() as usize, pm.height() as usize], pm.data());
            match &mut shot.texture {
                Some(t) => t.set(image, egui::TextureOptions::LINEAR),
                None => shot.texture = Some(ctx.load_texture("shot", image, egui::TextureOptions::LINEAR)),
            }
            shot.layout = render::layout(shot.src.width(), shot.src.height(), &shot.doc);
            shot.shown = Some(shot.doc.clone());
        }
        let lay = shot.layout;
        let avail = ui.available_rect_before_wrap().shrink(28.0);
        let ppp = ctx.pixels_per_point();
        let scale = (avail.width() / lay.width as f32).min(avail.height() / lay.height as f32).min(1.0 / ppp);
        let size = vec2(lay.width as f32, lay.height as f32) * scale;
        let rect = Rect::from_center_size(avail.center(), size);
        let painter = ui.painter_at(ui.max_rect());
        if let Some(t) = &shot.texture {
            painter.image(t.id(), rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        }
        let to_src = |p: Pos2| Pt::new((p.x - rect.min.x) / scale - lay.offset_x, (p.y - rect.min.y) / scale - lay.offset_y);
        let to_screen = |p: Pt| Pos2::new(rect.min.x + (p.x + lay.offset_x) * scale, rect.min.y + (p.y + lay.offset_y) * scale);

        let response = ui.interact(rect, Id::new("canvas"), Sense::click_and_drag());
        let hover = response.hover_pos();
        if hover.is_some() {
            ctx.set_cursor_icon(match self.tool {
                Tool::Text => egui::CursorIcon::Text,
                _ => egui::CursorIcon::Crosshair,
            });
        }
        let color = COLORS[self.settings.color.min(COLORS.len() - 1)];
        let color32 = Color32::from_rgba_unmultiplied(color[0], color[1], color[2], 255);
        let mut commit: Option<Annotation> = None;
        let mut new_crop: Option<Bounds> = None;
        let mut start_text: Option<Pt> = None;

        match self.tool {
            Tool::Step | Tool::Text => {
                if response.clicked()
                    && let Some(p) = response.interact_pointer_pos()
                {
                    if self.tool == Tool::Step {
                        commit = Some(Annotation { shape: Shape::Step { at: to_src(p), n: shot.doc.next_step() }, color });
                    } else {
                        start_text = Some(to_src(p));
                    }
                }
            }
            _ => {
                if response.drag_started()
                    && let Some(p) = response.interact_pointer_pos()
                {
                    self.drag = Some((to_src(p), to_src(p)));
                }
                if let (Some((_, end)), Some(p)) = (&mut self.drag, response.interact_pointer_pos())
                    && response.dragged()
                {
                    *end = to_src(p);
                }
                if response.drag_stopped()
                    && let Some((a, b)) = self.drag.take()
                {
                    let big = (a.x - b.x).abs() * scale > 4.0 || (a.y - b.y).abs() * scale > 4.0;
                    if big {
                        match self.tool {
                            Tool::Arrow => commit = Some(Annotation { shape: Shape::Arrow { from: a, to: b }, color }),
                            Tool::Rect => commit = Some(Annotation { shape: Shape::Rect { a, b }, color }),
                            Tool::Highlight => commit = Some(Annotation { shape: Shape::Highlight { a, b }, color }),
                            Tool::Redact => commit = Some(Annotation { shape: Shape::Redact { a, b }, color }),
                            Tool::Crop => new_crop = Some(Bounds::from_corners(a, b)),
                            _ => {}
                        }
                    }
                }
            }
        }

        // What a drag will make, drawn live over the render.
        if let Some((a, b)) = self.drag {
            let (sa, sb) = (to_screen(a), to_screen(b));
            let width = (lay.stroke * scale).max(2.0);
            match self.tool {
                Tool::Arrow => {
                    painter.line_segment([sa, sb], Stroke::new(width, color32));
                    painter.circle_filled(sb, width * 1.6, color32);
                }
                Tool::Rect => {
                    painter.rect_stroke(Rect::from_two_pos(sa, sb), CornerRadius::same(4), Stroke::new(width, color32), egui::StrokeKind::Middle);
                }
                Tool::Highlight => {
                    painter.rect_filled(Rect::from_two_pos(sa, sb), CornerRadius::ZERO, color32.gamma_multiply(0.4));
                }
                Tool::Redact => {
                    let r = Rect::from_two_pos(sa, sb);
                    painter.rect_filled(r, CornerRadius::same(2), Color32::from_gray(128).gamma_multiply(0.75));
                    painter.text(r.center(), egui::Align2::CENTER_CENTER, "redact", FontId::proportional(12.0), Color32::WHITE);
                }
                Tool::Crop => {
                    let r = Rect::from_two_pos(sa, sb);
                    let dim = Color32::from_black_alpha(130);
                    for part in [
                        Rect::from_min_max(rect.min, Pos2::new(rect.max.x, r.min.y)),
                        Rect::from_min_max(Pos2::new(rect.min.x, r.max.y), rect.max),
                        Rect::from_min_max(Pos2::new(rect.min.x, r.min.y), Pos2::new(r.min.x, r.max.y)),
                        Rect::from_min_max(Pos2::new(r.max.x, r.min.y), Pos2::new(rect.max.x, r.max.y)),
                    ] {
                        painter.rect_filled(part, CornerRadius::ZERO, dim);
                    }
                    painter.rect_stroke(r, CornerRadius::ZERO, Stroke::new(1.5_f32, Color32::WHITE), egui::StrokeKind::Middle);
                }
                _ => {}
            }
            ctx.request_repaint();
        }

        // Typing a text annotation, right where it will be drawn.
        let mut finish_text = false;
        let mut cancel_text = false;
        if let Some((at, text)) = &mut self.typing {
            let pos = to_screen(*at);
            let px = render::label_size(text, lay.stroke) * scale;
            egui::Area::new(Id::new("text-annotation")).fixed_pos(pos).order(egui::Order::Foreground).show(&ctx, |ui| {
                Frame::new()
                    .fill(color32)
                    .corner_radius(CornerRadius::same((px * 0.35) as u8))
                    .inner_margin(Margin::symmetric((px * 0.45) as i8, (px * 0.22) as i8))
                    .show(ui, |ui| {
                        let enter = ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
                        let esc = ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
                        let ink = if (color[0] as u32 + color[1] as u32) > 420 { Color32::from_rgb(28, 28, 30) } else { Color32::WHITE };
                        let font = FontId::proportional(px.max(9.0));
                        let out = shaped_edit(ui, text, font, ink, |e| e.frame(false).desired_width(160.0f32.max(px * 8.0)).hint_text("Type, then ↩"));
                        out.response.request_focus();
                        finish_text = enter;
                        cancel_text = esc;
                    });
            });
        }
        if cancel_text {
            self.typing = None;
        } else if finish_text || start_text.is_some() {
            self.commit_text();
        }
        if let Some(p) = start_text {
            self.typing = Some((p, String::new()));
        }
        if let Some(a) = commit {
            self.edit(|d| d.annotations.push(a));
        }
        if let Some(c) = new_crop {
            self.edit(|d| d.crop = Some(c));
            self.tool = Tool::Arrow;
            self.notify("Cropped. ⌘Z to undo", now);
        }
    }

    fn home(&mut self, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() * 0.22).max(24.0));
            ui.label(RichText::new("rsnap").size(34.0).strong());
            ui.add_space(6.0);
            ui.label(RichText::new("Press ⌥⇧S anywhere to capture. Frame it, mark it up, and ↩ copies it.").color(theme::MUTED()));
            ui.add_space(22.0);
            let capture = egui::Button::image_and_text(icon_image(ui, Svg::Camera, theme::BG()), RichText::new("Capture").color(theme::BG()).strong().size(15.0))
                .fill(theme::ACCENT())
                .corner_radius(8)
                .min_size(vec2(160.0, 40.0));
            if ui.add(capture).clicked() {
                self.start_capture(ui.ctx());
            }
            ui.add_space(26.0);
            if !self.screen_ok {
                banner(ui, "Allow Screen Recording", "rsnap needs it to see what you capture. Nothing leaves your Mac.", "Open System Settings", macos::ask_for_screen_access);
            }
            if !self.hotkey_ok {
                ui.label(RichText::new("Another app already uses ⌥⇧S, so use the button or the menu bar icon.").small().color(theme::FAINT()));
            }
            ui.add_space(10.0);
            let mut sound = self.settings.sound;
            if ui.checkbox(&mut sound, "Sound when copied").changed() {
                self.settings.sound = sound;
                self.save_settings();
            }
        });
    }

    fn status_bar(&mut self, ui: &mut Ui, now: f64) {
        if self.toast.as_ref().is_some_and(|(_, until)| now > *until) {
            self.toast = None;
        }
        ui.horizontal_centered(|ui| {
            if let Some((text, _)) = &self.toast {
                ui.label(RichText::new(text).small().color(theme::MUTED()));
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
            } else if self.shot.is_some() {
                ui.label(RichText::new("A arrow · R box · H highlight · B redact · N step · T text · C crop · 1-6 colors · ↩ copy").small().color(theme::FAINT()));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| match &self.update {
                UpdateState::Available(r) => {
                    let label = format!("Install {}", r.version);
                    if ui.small_button(label).clicked() {
                        self.install_update(ui.ctx());
                    }
                }
                UpdateState::Checking | UpdateState::Installing => {
                    ui.label(RichText::new("Updating…").small().color(theme::FAINT()));
                }
                state => {
                    let tip = match state {
                        UpdateState::Failed(e) => format!("Last check failed: {e}"),
                        _ => "Asks GitHub for a newer release. rsnap never does this on its own.".to_string(),
                    };
                    let b = egui::Button::new(RichText::new(format!("rsnap {}", update::CURRENT)).small().color(theme::FAINT())).frame(false);
                    if ui.add(b).on_hover_text(format!("Check for updates. {tip}")).clicked() {
                        self.check_update(ui.ctx());
                    }
                }
            });
        });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        theme::sync(ctx);
        let now = ctx.input(|i| i.time);
        self.handle_events(ctx, now);
        if !self.screen_ok && now - self.screen_checked > 1.5 {
            self.screen_checked = now;
            self.screen_ok = macos::screen_access();
            ctx.request_repaint_after(std::time::Duration::from_secs(2));
        }
        if ctx.input(|i| i.viewport().close_requested()) && !self.quitting {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            macos::hide();
        }
        if self.shot.is_some() {
            self.shortcuts(ctx, now);
        }

        let editing = self.shot.is_some();
        egui::TopBottomPanel::top("toolbar")
            .exact_height(48.0)
            .show_separator_line(false)
            .frame(Frame::new().fill(theme::BG()).inner_margin(Margin { left: 84, right: 12, top: 4, bottom: 0 }))
            .show(ctx, |ui| {
                if editing {
                    self.toolbar(ui, now);
                } else {
                    let drag = ui.interact(ui.max_rect(), Id::new("titlebar"), Sense::click_and_drag());
                    if drag.drag_started() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    }
                }
            });
        egui::TopBottomPanel::bottom("status")
            .exact_height(28.0)
            .show_separator_line(false)
            .frame(Frame::new().fill(theme::BG()).inner_margin(Margin::symmetric(14, 0)))
            .show(ctx, |ui| self.status_bar(ui, now));
        if editing {
            egui::SidePanel::right("style")
                .exact_width(200.0)
                .resizable(false)
                .show_separator_line(false)
                .frame(Frame::new().fill(theme::BG()).inner_margin(Margin { left: 12, right: 16, top: 8, bottom: 8 }))
                .show(ctx, |ui| self.style_panel(ui));
        }
        egui::CentralPanel::default()
            .frame(Frame::new().fill(theme::CANVAS()).corner_radius(if editing { 10 } else { 0 }))
            .show(ctx, |ui| if editing { self.canvas(ui, now) } else { self.home(ui) });

        self.frames = self.frames.saturating_add(1);
        if let Some(ready) = &self.demo_ready {
            if self.frames == 12 {
                let _ = std::fs::write(ready, "");
            } else if self.frames < 12 {
                ctx.request_repaint();
            }
        }
    }
}

// ---- Widgets ----

fn section(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).small().color(theme::FAINT()));
}

fn icon_image(ui: &Ui, svg: Svg, tint: Color32) -> egui::Image<'static> {
    icons::image(ui.ctx(), svg, 15.0, tint)
}

fn tool_button(ui: &mut Ui, svg: Svg, tip: &str, selected: bool) -> egui::Response {
    let (rect, r) = ui.allocate_exact_size(vec2(32.0, 30.0), Sense::click());
    if selected {
        ui.painter().rect_filled(rect, CornerRadius::same(7), theme::ACCENT().gamma_multiply(0.22));
    } else if r.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(7), theme::RAISE());
    }
    let tint = if selected { theme::ACCENT() } else if r.hovered() { theme::TEXT() } else { theme::MUTED() };
    icons::paint(ui, svg, Rect::from_center_size(rect.center(), vec2(17.0, 17.0)), tint);
    r.on_hover_text(tip)
}

fn icon_button(ui: &mut Ui, svg: Svg, tip: &str, enabled: bool) -> egui::Response {
    let (rect, r) = ui.allocate_exact_size(vec2(30.0, 30.0), if enabled { Sense::click() } else { Sense::hover() });
    if enabled && r.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(7), theme::RAISE());
    }
    let tint = if !enabled { theme::FAINT().gamma_multiply(0.6) } else if r.hovered() { theme::TEXT() } else { theme::MUTED() };
    icons::paint(ui, svg, Rect::from_center_size(rect.center(), vec2(16.0, 16.0)), tint);
    r.on_hover_text(tip)
}

fn color_dot(ui: &mut Ui, color: Color32, selected: bool) -> egui::Response {
    let (rect, r) = ui.allocate_exact_size(vec2(24.0, 30.0), Sense::click());
    let c = rect.center();
    if selected {
        ui.painter().circle_stroke(c, 10.0, Stroke::new(2.0_f32, theme::TEXT()));
    }
    ui.painter().circle_filled(c, if r.hovered() { 7.5 } else { 7.0 }, color);
    // A ring so dark ink stays visible on the dark toolbar (and white on light).
    ui.painter().circle_stroke(c, 7.0, Stroke::new(1.0_f32, theme::FAINT().gamma_multiply(0.8)));
    r
}

/// A backdrop preset as a small rounded square (diagonal gradient).
fn backdrop_swatch(ui: &mut Ui, b: Backdrop, selected: bool) -> egui::Response {
    let (rect, r) = ui.allocate_exact_size(vec2(50.0, 36.0), Sense::click());
    let inner = rect.shrink(3.0);
    match b {
        Backdrop::None => {
            ui.painter().rect_filled(inner, CornerRadius::same(6), theme::SURFACE());
            ui.painter().line_segment([inner.left_bottom(), inner.right_top()], Stroke::new(1.5_f32, theme::FAINT()));
        }
        Backdrop::Solid(c) => {
            ui.painter().rect_filled(inner, CornerRadius::same(6), Color32::from_rgb(c[0], c[1], c[2]));
        }
        Backdrop::Gradient(a, z) => {
            let (ca, cz) = (Color32::from_rgb(a[0], a[1], a[2]), Color32::from_rgb(z[0], z[1], z[2]));
            let mid = Color32::from_rgb(((a[0] as u16 + z[0] as u16) / 2) as u8, ((a[1] as u16 + z[1] as u16) / 2) as u8, ((a[2] as u16 + z[2] as u16) / 2) as u8);
            let mut mesh = egui::Mesh::default();
            mesh.colored_vertex(inner.left_top(), ca);
            mesh.colored_vertex(inner.right_top(), mid);
            mesh.colored_vertex(inner.right_bottom(), cz);
            mesh.colored_vertex(inner.left_bottom(), mid);
            mesh.add_triangle(0, 1, 2);
            mesh.add_triangle(0, 2, 3);
            ui.painter().add(mesh);
        }
    }
    ui.painter().rect_stroke(inner, CornerRadius::same(6), Stroke::new(1.0_f32, theme::BORDER()), egui::StrokeKind::Inside);
    if selected {
        ui.painter().rect_stroke(rect, CornerRadius::same(8), Stroke::new(2.0_f32, theme::ACCENT()), egui::StrokeKind::Inside);
    }
    r.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn banner(ui: &mut Ui, title: &str, text: &str, button: &str, action: fn()) {
    Frame::new()
        .fill(theme::tint_accent())
        .stroke(Stroke::new(1.0_f32, theme::ACCENT().gamma_multiply(0.5)))
        .corner_radius(10)
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.set_max_width(420.0);
            ui.label(RichText::new(title).strong());
            ui.label(RichText::new(text).color(theme::MUTED()));
            ui.add_space(4.0);
            if ui.add(egui::Button::new(RichText::new(button).color(theme::BG())).fill(theme::ACCENT()).corner_radius(6)).clicked() {
                action();
            }
        });
    ui.add_space(10.0);
}

/// A text editor that draws Khmer shaped (see shaping.rs).
fn shaped_edit(
    ui: &mut Ui,
    text: &mut String,
    font: FontId,
    color: Color32,
    configure: impl for<'a> FnOnce(egui::TextEdit<'a>) -> egui::TextEdit<'a>,
) -> egui::text_edit::TextEditOutput {
    let mut runs = Vec::new();
    let mut layouter = |ui: &Ui, buf: &dyn egui::TextBuffer, wrap: f32| {
        let mut job = LayoutJob::simple(buf.as_str().to_owned(), font.clone(), color, wrap);
        runs = shaping::hide_runs(ui, &mut job);
        ui.fonts_mut(|f| f.layout_job(job))
    };
    let out = configure(egui::TextEdit::multiline(text).desired_rows(1)).text_color(color).layouter(&mut layouter).show(ui);
    shaping::paint_runs(ui.painter(), &out.galley, out.galley_pos, &runs, None, 0.0);
    out
}

/// `2026-10-16 at 15.32.08`, for file names.
fn stamp() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let local = secs + macos::utc_offset();
    let days = local.div_euclid(86_400);
    let (y, m, d) = civil(days);
    let s = local.rem_euclid(86_400);
    format!("{y:04}-{m:02}-{d:02} at {:02}.{:02}.{:02}", s / 3600, s / 60 % 60, s % 60)
}

fn civil(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// The sample marks the site and README screenshots show, placed by share
/// of the image so they land on the same spots of any capture.
pub fn sample_annotations(w: f32, h: f32) -> Vec<Annotation> {
    let p = |x: f32, y: f32| Pt::new(x * w, y * h);
    vec![
        Annotation { shape: Shape::Highlight { a: p(0.03, 0.195), b: p(0.97, 0.29) }, color: COLORS[2] },
        Annotation { shape: Shape::Rect { a: p(0.02, 0.335), b: p(0.98, 0.47) }, color: COLORS[4] },
        Annotation { shape: Shape::Arrow { from: p(0.62, 0.66), to: p(0.86, 0.48) }, color: COLORS[0] },
        Annotation { shape: Shape::Step { at: p(0.94, 0.2), n: 1 }, color: COLORS[0] },
        Annotation { shape: Shape::Step { at: p(0.94, 0.35), n: 2 }, color: COLORS[0] },
        Annotation { shape: Shape::Text { at: p(0.2, 0.67), text: "Pinned, with its page".into() }, color: COLORS[0] },
        Annotation { shape: Shape::Redact { a: p(0.03, 0.85), b: p(0.6, 0.93) }, color: COLORS[5] },
    ]
}

#[cfg(test)]
mod tests {
    use super::civil;

    #[test]
    fn dates() {
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(1_792_152_000 / 86_400), (2026, 10, 16));
    }
}
