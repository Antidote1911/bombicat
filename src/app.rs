use egui::*;
use std::sync::{Arc, mpsc::Receiver};
use std::sync::atomic::{AtomicU32, Ordering};

use crate::{model::{Model, Field, generate_grid_no_guess}, scores, settings::Preferences};

struct PendingGeneration {
    attempts: Arc<AtomicU32>,
    receiver: Receiver<Vec<Field>>,
    click_x:  usize,
    click_y:  usize,
}

const CELL: f32 = 34.0;
const FACE: f32 = 32.0;

const NUM_COLORS: [Color32; 9] = [
    Color32::TRANSPARENT,
    Color32::from_rgb(100, 140, 255),
    Color32::from_rgb(0, 140, 0),
    Color32::from_rgb(255, 230, 80),
    Color32::from_rgb(255, 160, 60),
    Color32::from_rgb(255, 80, 80),
    Color32::from_rgb(0, 140, 140),
    Color32::from_rgb(0, 0, 0),
    Color32::from_rgb(100, 100, 100),
];

// ──────────────────────────────────────────────
// Textures
// ──────────────────────────────────────────────

struct Textures {
    flag_red: TextureHandle,
    question: TextureHandle,
    cat: TextureHandle,
}

fn load_png(ctx: &Context, name: &str, bytes: &[u8]) -> TextureHandle {
    let img = image::load_from_memory(bytes)
        .expect("embedded PNG invalid")
        .to_rgba8();
    let (w, h) = img.dimensions();
    let color_image =
        ColorImage::from_rgba_unmultiplied([w as usize, h as usize], img.as_raw());
    ctx.load_texture(name, color_image, TextureOptions::NEAREST)
}

fn load_svg(ctx: &Context, name: &str, bytes: &[u8]) -> TextureHandle {
    use resvg::usvg::TreeParsing;
    let opt = resvg::usvg::Options::default();
    let utree = resvg::usvg::Tree::from_data(bytes, &opt).expect("SVG invalide");
    let rtree = resvg::Tree::from_usvg(&utree);
    let size = rtree.size;
    let (w, h) = (size.width().round() as u32, size.height().round() as u32);
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h).unwrap();
    rtree.render(resvg::tiny_skia::Transform::default(), &mut pixmap.as_mut());
    let color_image = ColorImage::from_rgba_unmultiplied(
        [w as usize, h as usize],
        pixmap.data(),
    );
    ctx.load_texture(name, color_image, TextureOptions::LINEAR)
}

impl Textures {
    fn load(ctx: &Context) -> Self {
        Self {
            flag_red:  load_png(ctx, "flag_red",  include_bytes!("../assets/images/disarmed_red.png")),
            question:  load_png(ctx, "question",  include_bytes!("../assets/images/question.png")),
            cat:       load_svg(ctx, "cat",        include_bytes!("../assets/images/cat.svg")),
        }
    }
}

// ──────────────────────────────────────────────
// State
// ──────────────────────────────────────────────

#[derive(PartialEq, Clone, Copy)]
pub enum GameState {
    NotStarted,
    Running,
    Won,
    Lost,
}

enum Modal {
    None,
    Settings(SettingsState),
    Scores,
    SaveScore { time: i32 },
    About,
}

#[derive(Clone)]
struct SettingsState {
    level: usize,
}

// ──────────────────────────────────────────────
// App
// ──────────────────────────────────────────────

pub struct BombicatApp {
    model: Model,
    prefs: Preferences,
    state: GameState,
    timer: f64,
    last_time: Option<f64>,
    cat_display: i32,
    gen_attempts: u32,
    pressing: bool,
    cheat: bool,
    modal: Modal,
    save_name: String,
    tex: Textures,
    exploded: Option<(usize, usize)>,
    resize_pending: bool,
    pending_gen: Option<PendingGeneration>,
}

impl BombicatApp {
    pub fn new(cc: &eframe::CreationContext<'_>, cheat: bool) -> Self {
        let prefs = Preferences::load();
        let tex = Textures::load(&cc.egui_ctx);
        let mut app = Self {
            model: Model::new(),
            prefs,
            state: GameState::NotStarted,
            timer: 0.0,
            last_time: None,
            cat_display: 0,
            gen_attempts: 0,
            pressing: false,
            cheat,
            modal: Modal::None,
            save_name: String::new(),
            tex,
            exploded: None,
            resize_pending: true,
            pending_gen: None,
        };
        app.new_game();
        app
    }

    fn new_game(&mut self) {
        let (w, h, s) = self.prefs.effective_dims();
        self.model.reset(w, h, s);
        self.state = GameState::NotStarted;
        self.timer = 0.0;
        self.last_time = None;
        self.cat_display = s as i32;
        self.gen_attempts = 0;
        self.pending_gen = None;
        self.exploded = None;
        self.resize_pending = true;
    }

    fn window_size_for_grid(&self) -> Vec2 {
        vec2(
            self.model.width  as f32 * CELL + 16.0,
            self.model.height as f32 * CELL + 100.0,
        )
    }

    // ── game actions ──────────────────────────

    fn left_click(&mut self, ctx: &Context, x: usize, y: usize) {
        if !matches!(self.state, GameState::NotStarted | GameState::Running) {
            return;
        }
        if self.pending_gen.is_some() { return; }
        let f = self.model.field(x, y);
        if f.flag != 0 || f.discovered { return; }

        if self.state == GameState::NotStarted {
            let attempts = Arc::new(AtomicU32::new(0));
            let (tx, rx) = std::sync::mpsc::channel();
            let attempts_clone = Arc::clone(&attempts);
            let ctx_clone = ctx.clone();
            let (width, height, total_cats) = (
                self.model.width, self.model.height, self.model.total_cats(),
            );
            std::thread::spawn(move || {
                let data = generate_grid_no_guess(
                    width, height, total_cats, x, y, &attempts_clone,
                );
                tx.send(data).ok();
                ctx_clone.request_repaint();
            });
            self.pending_gen = Some(PendingGeneration { attempts, receiver: rx, click_x: x, click_y: y });
            return;
        }

        self.discover_flood(x, y);
        self.check_outcome();
    }

    fn right_click(&mut self, x: usize, y: usize) {
        if !matches!(self.state, GameState::NotStarted | GameState::Running) {
            return;
        }
        if self.model.field(x, y).discovered {
            return;
        }
        let delta = self.model.disarm(x, y);
        self.cat_display += delta;
    }

    fn chord_click(&mut self, x: usize, y: usize) {
        if self.state != GameState::Running {
            return;
        }
        let f = self.model.field(x, y);
        if !f.discovered {
            return;
        }
        if self.model.count_flags_around(x, y) != f.neighbours {
            return;
        }
        // collect undiscovered, unflagged neighbours
        let targets: Vec<(usize, usize)> = (-1i32..=1)
            .flat_map(|dy| {
                (-1i32..=1).filter_map(move |dx| {
                    if dx == 0 && dy == 0 {
                        return None;
                    }
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    Some((nx, ny))
                })
            })
            .filter(|&(nx, ny)| self.model.is_valid(nx, ny))
            .map(|(nx, ny)| (nx as usize, ny as usize))
            .filter(|&(nx, ny)| {
                let f = self.model.field(nx, ny);
                !f.discovered && f.flag == 0
            })
            .collect();

        for (nx, ny) in targets {
            self.discover_flood(nx, ny);
        }
        self.check_outcome();
    }

    fn discover_flood(&mut self, x: usize, y: usize) {
        let f = self.model.field(x, y);
        if f.discovered || f.flag != 0 {
            return;
        }
        self.model.discover(x, y);
        let neighbours = self.model.field(x, y).neighbours;
        let is_cat = self.model.field(x, y).cat;

        if neighbours == 0 && !is_cat {
            let coords: Vec<(usize, usize)> = (-1i32..=1)
                .flat_map(|dy| (-1i32..=1).map(move |dx| (dx, dy)))
                .filter(|&(dx, dy)| !(dx == 0 && dy == 0))
                .map(|(dx, dy)| (x as i32 + dx, y as i32 + dy))
                .filter(|&(nx, ny)| self.model.is_valid(nx, ny))
                .map(|(nx, ny)| (nx as usize, ny as usize))
                .filter(|&(nx, ny)| {
                    let f = self.model.field(nx, ny);
                    !f.discovered && f.flag == 0
                })
                .collect();
            for (nx, ny) in coords {
                self.discover_flood(nx, ny);
            }
        }
    }

    fn reveal_all(&mut self) {
        for y in 0..self.model.height {
            for x in 0..self.model.width {
                self.model.data[y * self.model.width + x].discovered = true;
            }
        }
    }

    fn check_outcome(&mut self) {
        if self.model.any_discovered_cat() {
            self.state = GameState::Lost;
            self.exploded = self.model.find_exploded();
        } else if self.model.check_win() {
            self.state = GameState::Won;
            let time = self.timer as i32;
            self.modal = Modal::SaveScore { time };
        }
    }

    // ── drawing ───────────────────────────────

    fn draw_cell(
        painter: &Painter,
        rect: Rect,
        field: &crate::model::Field,
        active: bool,
        is_exploded: bool,
        tex: &Textures,
    ) {
        let bg      = Color32::from_rgb(32, 32, 32);
        let sunken  = Color32::from_rgb(22, 22, 22);
        let sep     = Color32::from_rgb(18, 18, 18);

        let flag_violet = Color32::from_rgb(160, 0, 220);

        if field.discovered {
            if is_exploded {
                // La case cliquée : enfoncée + chat rouge
                painter.rect_filled(rect, 0.0, sunken);
                painter.rect_stroke(rect, 0.0, Stroke::new(1.0, sep), egui::StrokeKind::Outside);
                draw_cat(painter, rect, Color32::from_rgb(220, 50, 50), &tex.cat);
            } else if active {
                painter.rect_filled(rect, 0.0, sunken);
                painter.rect_stroke(rect, 0.0, Stroke::new(1.0, sep), egui::StrokeKind::Outside);
                if field.cat {
                    draw_cat(painter, rect, Color32::from_rgb(230, 120, 0), &tex.cat);
                } else if field.neighbours > 0 {
                    painter.text(
                        rect.center(), Align2::CENTER_CENTER,
                        field.neighbours.to_string(),
                        FontId::proportional(17.0),
                        NUM_COLORS[field.neighbours as usize],
                    );
                }
            } else {
                // Game over — cases découvertes normales
                painter.rect_filled(rect, 0.0, sunken);
                painter.rect_stroke(rect, 0.0, Stroke::new(1.0, sep), egui::StrokeKind::Outside);
                if field.neighbours > 0 {
                    painter.text(
                        rect.center(), Align2::CENTER_CENTER,
                        field.neighbours.to_string(),
                        FontId::proportional(17.0),
                        NUM_COLORS[field.neighbours as usize],
                    );
                }
            }
        } else {
            painter.rect_filled(rect, 0.0, bg);
            painter.rect_stroke(rect, 0.0, Stroke::new(1.0, sep), egui::StrokeKind::Outside);

            if active {
                match field.flag {
                    1 => draw_flag(painter, rect, flag_violet),
                    2 => paint_icon(painter, rect, &tex.question),
                    _ => {}
                }
            } else {
                // Game over — cases non découvertes : révèle les mines en orange
                match (field.flag, field.cat) {
                    (1, true)  => draw_flag(painter, rect, flag_violet),
                    (1, false) => paint_icon(painter, rect, &tex.flag_red),
                    (_, true)  => draw_cat(painter, rect, Color32::from_rgb(230, 120, 0), &tex.cat),
                    _ => {}
                }
            }
        }
    }
}

fn paint_icon(painter: &Painter, cell_rect: Rect, tex: &TextureHandle) {
    let size = cell_rect.width().min(cell_rect.height()) * 0.55;
    let icon_rect = Rect::from_center_size(cell_rect.center(), Vec2::splat(size));
    let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    painter.image(tex.id(), icon_rect, uv, Color32::WHITE);
}

fn draw_flag(painter: &Painter, cell_rect: Rect, color: Color32) {
    let c = cell_rect.center();
    let s = cell_rect.width().min(cell_rect.height()) * 0.28;
    // pole
    let pole_x = c.x - s * 0.1;
    let pole_top = c.y - s * 0.9;
    let pole_bot = c.y + s * 0.9;
    painter.line_segment(
        [pos2(pole_x, pole_top), pos2(pole_x, pole_bot)],
        Stroke::new(1.5, Color32::GRAY),
    );
    // base
    painter.line_segment(
        [pos2(pole_x - s * 0.5, pole_bot), pos2(pole_x + s * 0.5, pole_bot)],
        Stroke::new(1.5, Color32::GRAY),
    );
    // triangular flag
    let mesh = epaint::Mesh {
        indices: vec![0, 1, 2],
        vertices: vec![
            epaint::Vertex { pos: pos2(pole_x, pole_top),              uv: pos2(0.0, 0.0), color },
            epaint::Vertex { pos: pos2(pole_x, pole_top + s * 0.85),   uv: pos2(0.0, 0.0), color },
            epaint::Vertex { pos: pos2(pole_x + s * 0.9, pole_top + s * 0.4), uv: pos2(0.0, 0.0), color },
        ],
        texture_id: epaint::TextureId::default(),
    };
    painter.add(epaint::Shape::mesh(mesh));
}

fn draw_cat(painter: &Painter, cell_rect: Rect, color: Color32, tex: &TextureHandle) {
    let size = cell_rect.width().min(cell_rect.height()) * 0.72;
    let icon_rect = Rect::from_center_size(cell_rect.center(), Vec2::splat(size));
    let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    painter.image(tex.id(), icon_rect, uv, color);
}

#[derive(Clone, Copy)]
enum FaceKind { Normal, Fear, Happy, Sad }

fn draw_face(painter: &Painter, rect: Rect, kind: FaceKind) {
    let c = rect.center();
    let r = rect.width().min(rect.height()) / 2.0 - 1.0;
    let yellow = Color32::from_rgb(255, 220, 0);
    let black  = Color32::BLACK;
    let stroke = Stroke::new(1.5, black);

    painter.circle(c, r, yellow, stroke);

    let eye_y = c.y - r * 0.28;
    let eye_r = (r * 0.12).max(2.0);
    painter.circle_filled(pos2(c.x - r * 0.32, eye_y), eye_r, black);
    painter.circle_filled(pos2(c.x + r * 0.32, eye_y), eye_r, black);

    let mw = r * 0.52;
    match kind {
        FaceKind::Normal => bezier_mouth(painter, c, r * 0.22, mw, 0.30, stroke),
        FaceKind::Happy  => bezier_mouth(painter, c, r * 0.22, mw, 0.60, stroke),
        FaceKind::Sad    => bezier_frown(painter, c, r * 0.35, mw, 0.45, stroke),
        FaceKind::Fear   => {
            painter.circle(pos2(c.x, c.y + r * 0.28), r * 0.18, black, Stroke::NONE);
        }
    }
}

fn bezier_mouth(painter: &Painter, c: Pos2, y_off: f32, hw: f32, depth: f32, stroke: Stroke) {
    let y  = c.y + y_off;
    let p0 = pos2(c.x - hw, y);
    let p3 = pos2(c.x + hw, y);
    let cy = y + depth * hw;
    painter.add(epaint::CubicBezierShape::from_points_stroke(
        [p0, pos2(c.x - hw * 0.3, cy), pos2(c.x + hw * 0.3, cy), p3],
        false, Color32::TRANSPARENT, stroke,
    ));
}

fn bezier_frown(painter: &Painter, c: Pos2, y_off: f32, hw: f32, depth: f32, stroke: Stroke) {
    let y  = c.y + y_off;
    let p0 = pos2(c.x - hw, y);
    let p3 = pos2(c.x + hw, y);
    let cy = y - depth * hw;
    painter.add(epaint::CubicBezierShape::from_points_stroke(
        [p0, pos2(c.x - hw * 0.3, cy), pos2(c.x + hw * 0.3, cy), p3],
        false, Color32::TRANSPARENT, stroke,
    ));
}

fn lcd_label(ui: &mut Ui, value: i32) {
    let text = format!("{:03}", value.clamp(-99, 999));
    ui.label(
        RichText::new(text)
            .monospace()
            .size(22.0)
            .color(Color32::RED)
            .background_color(Color32::BLACK),
    );
}

// ──────────────────────────────────────────────
// eframe::App
// ──────────────────────────────────────────────

impl eframe::App for BombicatApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        // advance timer
        if self.state == GameState::Running {
            let now = ctx.input(|i| i.time);
            if let Some(last) = self.last_time {
                self.timer += now - last;
            }
            self.last_time = Some(now);
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        } else {
            self.last_time = None;
        }

        // Poll background grid generation.
        if let Some(ref pg) = self.pending_gen {
            match pg.receiver.try_recv() {
                Ok(data) => {
                    let (cx, cy) = (pg.click_x, pg.click_y);
                    self.gen_attempts = pg.attempts.load(Ordering::Relaxed);
                    self.model.data = data;
                    self.pending_gen = None;
                    self.state = GameState::Running;
                    self.discover_flood(cx, cy);
                    self.check_outcome();
                }
                Err(_) => { ctx.request_repaint_after(std::time::Duration::from_millis(30)); }
            }
        }

        self.draw_menubar(ctx);
        self.draw_statusbar(ctx);
        self.draw_game_panel(ctx);
        self.draw_modal(ctx);
        self.draw_generating_popup(ctx);

        if self.resize_pending {
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(self.window_size_for_grid()));
            self.resize_pending = false;
        }
    }
}

impl BombicatApp {
    fn draw_menubar(&mut self, ctx: &Context) {
        TopBottomPanel::top("menu").show(ctx, |ui| {
            menu::bar(ui, |ui| {
                ui.menu_button("Fichier", |ui| {
                    if ui.button("Nouvelle Partie  Ctrl+N").clicked() {
                        self.new_game();
                        ui.close_menu();
                    }
                    if ui.button("Meilleurs Scores").clicked() {
                        self.modal = Modal::Scores;
                        ui.close_menu();
                    }
                    if ui.button("Préférences").clicked() {
                        self.modal = Modal::Settings(SettingsState { level: self.prefs.level });
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Quitter").clicked() {
                        ctx.send_viewport_cmd(ViewportCommand::Close);
                    }
                });
                ui.menu_button("Infos", |ui| {
                    if ui.button("À Propos").clicked() {
                        self.modal = Modal::About;
                        ui.close_menu();
                    }
                });
            });
        });

        // Ctrl+N shortcut
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::CTRL, Key::N))) {
            self.new_game();
        }
    }

    fn draw_statusbar(&mut self, ctx: &Context) {
        TopBottomPanel::bottom("status").show(ctx, |ui| {
            let (_, _, cats) = Preferences::level_params(self.prefs.level);
            let level_name = Preferences::level_name(self.prefs.level);
            let text = match self.state {
                GameState::Lost => "Malheureusement, tu as perdu.".into(),
                GameState::Won  => "Tu as gagné !".into(),
                _ if self.gen_attempts == 0 => format!("{} — {} BombaCat", level_name, cats),
                _ => format!("{} — {} BombaCat  (grille résoluble générée en {} essai{}.)",
                    level_name, cats,
                    self.gen_attempts,
                    if self.gen_attempts > 1 { "s" } else { "" }),
            };
            ui.label(text);
        });
    }

    fn draw_game_panel(&mut self, ctx: &Context) {
        CentralPanel::default().show(ctx, |ui| {
            let modal_open = !matches!(self.modal, Modal::None);

            // ── top bar ──────────────────────
            let bar_height = FACE + 8.0;
            let (bar_rect, _) = ui.allocate_exact_size(
                vec2(ui.available_width(), bar_height),
                Sense::hover(),
            );

            // left: cat counter
            let cat_rect = Rect::from_min_size(
                bar_rect.left_top() + vec2(4.0, (bar_height - 30.0) / 2.0),
                vec2(48.0, 30.0),
            );
            // right: timer
            let timer_rect = Rect::from_min_size(
                bar_rect.right_top() + vec2(-52.0, (bar_height - 30.0) / 2.0),
                vec2(48.0, 30.0),
            );
            // center: face button
            let face_rect = Rect::from_center_size(bar_rect.center(), Vec2::splat(FACE + 4.0));

            // draw cat counter
            {
                let mut child = ui.new_child(UiBuilder::new().max_rect(cat_rect).layout(Layout::left_to_right(Align::Center)));
                lcd_label(&mut child, self.cat_display);
            }
            // draw timer
            {
                let mut child = ui.new_child(UiBuilder::new().max_rect(timer_rect).layout(Layout::left_to_right(Align::Center)));
                lcd_label(&mut child, self.timer as i32);
            }
            // draw face button
            let face_response = ui.allocate_rect(face_rect, if modal_open { Sense::hover() } else { Sense::click() });
            {
                let face_kind = match self.state {
                    GameState::Won  => FaceKind::Happy,
                    GameState::Lost => FaceKind::Sad,
                    _ if self.pressing => FaceKind::Fear,
                    _ => FaceKind::Normal,
                };
                let inner = Rect::from_center_size(face_rect.center(), Vec2::splat(FACE));
                draw_face(ui.painter(), inner, face_kind);
                if face_response.hovered() {
                    ui.painter().rect_stroke(face_rect, 3.0, Stroke::new(1.5, Color32::GRAY), egui::StrokeKind::Outside);
                }
            }
            if face_response.clicked() {
                self.new_game();
            }

            ui.separator();

            // ── grid ─────────────────────────
            let active = matches!(self.state, GameState::NotStarted | GameState::Running);
            let cols = self.model.width;
            let rows = self.model.height;

            let cell = CELL;

            let grid_size = vec2(cols as f32 * cell, rows as f32 * cell);
            let (grid_rect, _) =
                ui.allocate_exact_size(grid_size, Sense::hover());

            // detect pressing for face expression
            let pointer_down = ctx.input(|i| i.pointer.primary_down());
            self.pressing = pointer_down && active;

            // collect input
            let mut left_click: Option<(usize, usize)> = None;
            let mut right_click: Option<(usize, usize)> = None;
            let mut chord_click: Option<(usize, usize)> = None;
            let middle_released = self.cheat
                && ctx.input(|i| i.pointer.button_released(PointerButton::Middle));

            if !modal_open && self.pending_gen.is_none() {
            if let Some(pointer_pos) = ctx.input(|i| i.pointer.hover_pos()) {
                if grid_rect.contains(pointer_pos) {
                    let col = ((pointer_pos.x - grid_rect.left()) / cell) as usize;
                    let row = ((pointer_pos.y - grid_rect.top()) / cell) as usize;
                    if col < cols && row < rows {
                        let primary_released = ctx.input(|i| i.pointer.primary_released());
                        let secondary_released = ctx.input(|i| i.pointer.secondary_released());
                        let primary_down = ctx.input(|i| i.pointer.primary_down());
                        let secondary_down = ctx.input(|i| i.pointer.secondary_down());

                        if secondary_released && !primary_down {
                            right_click = Some((col, row));
                        } else if primary_released && secondary_down {
                            chord_click = Some((col, row));
                        } else if primary_released && !secondary_down {
                            left_click = Some((col, row));
                        }
                    }
                }
            }
            } // end !modal_open

            // paint cells
            let painter = ui.painter();
            for row in 0..rows {
                for col in 0..cols {
                    let cell_rect = Rect::from_min_size(
                        grid_rect.min + vec2(col as f32 * cell, row as f32 * cell),
                        Vec2::splat(cell),
                    );
                    let is_exploded = self.exploded == Some((col, row));
                    Self::draw_cell(painter, cell_rect, self.model.field(col, row), active, is_exploded, &self.tex);
                }
            }

            // process input
            if middle_released {
                self.reveal_all();
            }
            if active {
                if let Some((x, y)) = left_click {
                    self.left_click(ctx, x, y);
                }
                if let Some((x, y)) = right_click {
                    self.right_click(x, y);
                }
                if let Some((x, y)) = chord_click {
                    self.chord_click(x, y);
                }
            }
        });
    }

    fn draw_generating_popup(&self, ctx: &Context) {
        let Some(ref pg) = self.pending_gen else { return };
        let n = pg.attempts.load(Ordering::Relaxed);
        if n <= 10_000 { return; }
        egui::Modal::new(Id::new("generating_modal"))
            .backdrop_color(Color32::TRANSPARENT)
            .show(ctx, |ui| {
                ui.label("Test de résolubilité en cours...");
                ui.label(format!("{} essais", n));
            });
    }

    fn draw_modal(&mut self, ctx: &Context) {
        match &self.modal {
            Modal::None => {}
            Modal::Settings(_) => self.draw_settings_modal(ctx),
            Modal::Scores => self.draw_scores_modal(ctx),
            Modal::SaveScore { .. } => self.draw_savescore_modal(ctx),
            Modal::About => self.draw_about_modal(ctx),
        }
    }

    fn draw_settings_modal(&mut self, ctx: &Context) {
        let Modal::Settings(ref st) = self.modal else { return };
        let mut st = st.clone();

        let levels = ["Débutant","Normal","Difficile","Ultra Difficile","Géant","Chuck Norris"];

        #[derive(PartialEq)] enum Action { None, Ok, Cancel }
        let mut action = Action::None;

        let resp = egui::Modal::new(Id::new("settings_modal")).backdrop_color(Color32::TRANSPARENT).show(ctx, |ui| {
            ui.heading("Préférences");
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Difficulté:");
                ComboBox::from_id_salt("lvl")
                    .selected_text(levels[st.level])
                    .show_ui(ui, |ui| {
                        for (i, name) in levels.iter().enumerate() {
                            ui.selectable_value(&mut st.level, i, *name);
                        }
                    });
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("OK").clicked()      { action = Action::Ok; }
                if ui.button("Annuler").clicked() { action = Action::Cancel; }
            });
        });

        match action {
            Action::Ok => {
                self.prefs.level = st.level;
                self.prefs.save();
                self.new_game();
                self.modal = Modal::None;
            }
            Action::Cancel => { self.modal = Modal::None; }
            Action::None if resp.should_close() => { self.modal = Modal::None; }
            _ => {
                if let Modal::Settings(ref mut stored) = self.modal { *stored = st; }
            }
        }
    }

    fn draw_scores_modal(&mut self, ctx: &Context) {
        let mut close = false;
        let screen = ctx.screen_rect();

        let w = screen.width() * 0.85;
        let mut open = true;
        Window::new("Meilleurs Scores")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .fixed_size(vec2(w, screen.height() * 0.75))
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                let levels = ["Débutant","Normal","Difficile","Ultra Difficile","Géant","Chuck Norris"];
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (level, name) in levels.iter().enumerate() {
                        ui.collapsing(*name, |ui| {
                            let top = scores::top_ten(level);
                            if top.is_empty() {
                                ui.label("Aucun score.");
                            } else {
                                for (i, s) in top.iter().enumerate() {
                                    ui.label(format!("{}. {} — {} s", i + 1, s.name, s.time));
                                }
                            }
                        });
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Réinitialiser").clicked() { let _ = scores::reset_all(); }
                    if ui.button("Fermer").clicked() { close = true; }
                });
            });
        if close || !open { self.modal = Modal::None; }
    }

    fn draw_savescore_modal(&mut self, ctx: &Context) {
        let Modal::SaveScore { time } = self.modal else { return };
        let level = self.prefs.level;
        let mut done = false;

        egui::Modal::new(Id::new("savescore_modal")).backdrop_color(Color32::TRANSPARENT).show(ctx, |ui| {
            ui.heading("Victoire !");
            ui.label(format!("Tu as gagné en {} secondes !", time));
            ui.add_space(4.0);
            ui.label("Entre ton nom :");
            ui.text_edit_singleline(&mut self.save_name);
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.button("Sauvegarder").clicked() {
                    let name = self.save_name.trim().to_string();
                    if !name.is_empty() {
                        let _ = scores::insert(&name, time, level);
                        self.save_name.clear();
                        done = true;
                    }
                }
                if ui.button("Ignorer").clicked() { done = true; }
            });
        });
        if done { self.modal = Modal::None; }
    }

    fn draw_about_modal(&mut self, ctx: &Context) {
        let mut close = false;
        let resp = egui::Modal::new(Id::new("about_modal")).backdrop_color(Color32::TRANSPARENT).show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.heading("Bombicat");
                ui.label(format!("version {}", env!("CARGO_PKG_VERSION")));
                ui.label("Un démineur avec des chats.");
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);
                ui.label("Clic gauche  — découvrir une case");
                ui.label("Clic droit   — poser / retirer un drapeau");
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);
                ui.label("Icônes : Freepik — flaticon.com (CC BY 3.0)");
                ui.add_space(8.0);
                if ui.button("  Fermer  ").clicked() { close = true; }
            });
        });
        if close || resp.should_close() { self.modal = Modal::None; }
    }
}
