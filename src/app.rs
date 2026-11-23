use egui::*;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{mpsc::Receiver, Arc};

use crate::{
    model::{generate_grid_no_guess, Field, Model, FLAG_NONE, FLAG_QUESTION, FLAG_SET},
    scores::{self, Score},
    settings::{Preferences, LEVEL_COUNT},
};

#[derive(Clone, Copy)]
struct HintState {
    x: usize,
    y: usize,
    start_time: f64,
}

/// Génération en cours dans un thread d'arrière-plan.
/// L'abandonner (nouvelle partie, fermeture…) arrête le calcul.
struct PendingGeneration {
    attempts: Arc<AtomicU32>,
    cancel: Arc<AtomicBool>,
    receiver: Receiver<Option<Vec<Field>>>,
    click_x: usize,
    click_y: usize,
}

impl Drop for PendingGeneration {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// Suivi des boutons de souris enfoncés sur la grille.
#[derive(Default)]
struct GridInput {
    /// Un appui a commencé sur la grille (et pas dans un menu ou une fenêtre).
    armed: bool,
    /// Une révélation à deux boutons a eu lieu : ignorer les relâchements suivants.
    consumed: bool,
}

/// Taille de case par défaut (et à l'ouverture de la fenêtre).
const CELL: f32 = 30.0;
const MIN_CELL: f32 = 12.0;
const MAX_CELL: f32 = 64.0;
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
    ctx.load_texture(name, color_image, TextureOptions::LINEAR)
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
    Scores(ScoresState),
    SaveScore { time: i32 },
    About,
}

#[derive(Clone)]
struct SettingsState {
    level: usize,
}

/// Scores lus une seule fois à l'ouverture de la fenêtre.
struct ScoresState {
    tables: Vec<Vec<Score>>,
    confirm_reset: bool,
}

impl ScoresState {
    fn load() -> Self {
        Self {
            tables: (0..LEVEL_COUNT).map(scores::top_ten).collect(),
            confirm_reset: false,
        }
    }
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
    gen_attempts: u32,
    pressing: bool,
    cheat: bool,
    modal: Modal,
    save_name: String,
    tex: Textures,
    resize_pending: bool,
    pending_gen: Option<PendingGeneration>,
    popup_was_open: bool,
    hint: Option<HintState>,
    /// L'aide (ou la triche) a servi : la partie n'entre pas dans les scores.
    assisted: bool,
    input: GridInput,
    /// Place prise par tout ce qui n'est pas la grille (menu, barres, marges).
    chrome: Vec2,
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
            gen_attempts: 0,
            pressing: false,
            cheat,
            modal: Modal::None,
            save_name: String::new(),
            tex,
            resize_pending: true,
            pending_gen: None,
            popup_was_open: false,
            hint: None,
            assisted: false,
            input: GridInput::default(),
            chrome: vec2(16.0, 100.0),
        };
        app.new_game();
        app
    }

    fn new_game(&mut self) {
        let (w, h, s) = self.prefs.effective_dims();
        let size_changed = (w, h) != (self.model.width, self.model.height);
        self.model.reset(w, h, s);
        self.state = GameState::NotStarted;
        self.timer = 0.0;
        self.last_time = None;
        self.gen_attempts = 0;
        self.pending_gen = None; // annule une éventuelle génération en cours
        self.hint = None;
        self.assisted = false;
        self.input = GridInput::default();
        if size_changed {
            self.resize_pending = true;
        }
    }

    fn trigger_hint(&mut self, ctx: &Context) {
        if self.state != GameState::Running {
            return;
        }
        if let Some((x, y)) = self.model.hint() {
            let now = ctx.input(|i| i.time);
            self.hint = Some(HintState { x, y, start_time: now });
            self.assisted = true;
        }
    }

    /// Taille de fenêtre pour afficher la grille avec des cases de taille
    /// normale, sans dépasser l'écran.
    fn window_size_for_grid(&self, ctx: &Context) -> Vec2 {
        let wanted = vec2(
            self.model.width as f32 * CELL,
            self.model.height as f32 * CELL,
        ) + self.chrome;
        match ctx.input(|i| i.viewport().monitor_size) {
            Some(monitor) if monitor.x > 0.0 && monitor.y > 0.0 => wanted.min(monitor * 0.9),
            _ => wanted,
        }
    }

    // ── game actions ──────────────────────────

    fn left_click(&mut self, ctx: &Context, x: usize, y: usize) {
        if !matches!(self.state, GameState::NotStarted | GameState::Running) {
            return;
        }
        if self.pending_gen.is_some() {
            return;
        }
        let f = self.model.field(x, y);
        if f.flag != FLAG_NONE || f.discovered {
            return;
        }

        if self.state == GameState::NotStarted {
            let attempts = Arc::new(AtomicU32::new(0));
            let cancel = Arc::new(AtomicBool::new(false));
            let (tx, rx) = std::sync::mpsc::channel();
            let (attempts_clone, cancel_clone) = (Arc::clone(&attempts), Arc::clone(&cancel));
            let ctx_clone = ctx.clone();
            let (width, height, total_cats) =
                (self.model.width, self.model.height, self.model.total_cats());
            std::thread::spawn(move || {
                let data = generate_grid_no_guess(
                    width, height, total_cats, x, y, &attempts_clone, &cancel_clone,
                );
                tx.send(data).ok();
                ctx_clone.request_repaint();
            });
            self.pending_gen = Some(PendingGeneration {
                attempts, cancel, receiver: rx, click_x: x, click_y: y,
            });
            return;
        }

        self.model.reveal(x, y);
        self.check_outcome();
    }

    fn right_click(&mut self, x: usize, y: usize) {
        if !matches!(self.state, GameState::NotStarted | GameState::Running) {
            return;
        }
        self.model.toggle_mark(x, y);
    }

    fn chord_click(&mut self, x: usize, y: usize) {
        if self.state != GameState::Running {
            return;
        }
        if self.model.chord(x, y) {
            self.check_outcome();
        }
    }

    fn check_outcome(&mut self) {
        if self.model.exploded().is_some() {
            self.state = GameState::Lost;
        } else if self.model.is_won() {
            self.state = GameState::Won;
            self.model.flag_remaining_cats();
            if !self.assisted {
                let time = self.timer as i32;
                self.modal = Modal::SaveScore { time };
            }
        }
    }

    // ── drawing ───────────────────────────────

    fn draw_cell(
        painter: &Painter,
        rect: Rect,
        field: &Field,
        active: bool,
        pressed: bool,
        is_exploded: bool,
        tex: &Textures,
    ) {
        let bg      = Color32::from_rgb(32, 32, 32);
        let sunken  = Color32::from_rgb(22, 22, 22);
        let sep     = Color32::from_rgb(18, 18, 18);

        let flag_violet = Color32::from_rgb(160, 0, 220);
        let cat_orange  = Color32::from_rgb(230, 120, 0);

        let draw_number = || {
            if field.neighbours > 0 && !field.cat {
                painter.text(
                    rect.center(), Align2::CENTER_CENTER,
                    field.neighbours.to_string(),
                    FontId::proportional(rect.height() * 0.57),
                    NUM_COLORS[(field.neighbours as usize).min(8)],
                );
            }
        };

        if field.discovered {
            painter.rect_filled(rect, 0.0, sunken);
            painter.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, sep), StrokeKind::Outside);
            if is_exploded {
                // La case cliquée : chat rouge
                draw_cat(painter, rect, Color32::from_rgb(220, 50, 50), &tex.cat);
            } else if field.cat {
                if active {
                    draw_cat(painter, rect, cat_orange, &tex.cat);
                }
            } else {
                draw_number();
            }
        } else {
            painter.rect_filled(rect, 0.0, if pressed { sunken } else { bg });
            painter.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, sep), StrokeKind::Outside);

            if active {
                match field.flag {
                    FLAG_SET => draw_flag(painter, rect, flag_violet),
                    FLAG_QUESTION => paint_icon(painter, rect, &tex.question),
                    _ => {}
                }
            } else {
                // Partie finie — cases non découvertes : révèle les chats en orange
                match (field.flag, field.cat) {
                    (FLAG_SET, true)  => draw_flag(painter, rect, flag_violet),
                    (FLAG_SET, false) => paint_icon(painter, rect, &tex.flag_red),
                    (_, true)         => draw_cat(painter, rect, cat_orange, &tex.cat),
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
        Stroke::new(1.5_f32, Color32::GRAY),
    );
    // base
    painter.line_segment(
        [pos2(pole_x - s * 0.5, pole_bot), pos2(pole_x + s * 0.5, pole_bot)],
        Stroke::new(1.5_f32, Color32::GRAY),
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
    let stroke = Stroke::new(1.5_f32, black);

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
        // Capture BEFORE any drawing — close_menu() clears this flag mid-frame.
        // If a popup was open at the start of the frame, the user is likely
        // clicking inside a menu; block grid input for this frame.
        self.popup_was_open = ctx.memory(|m| m.any_popup_open());

        // advance timer
        if self.state == GameState::Running {
            let now = ctx.input(|i| i.time);
            if let Some(last) = self.last_time {
                self.timer += now - last;
            }
            self.last_time = Some(now);
            // Réveil juste après le prochain changement de seconde.
            let until_next = 1.0 - self.timer.fract() + 0.01;
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(until_next));
        } else {
            self.last_time = None;
        }

        // Poll background grid generation.
        if let Some(pg) = self.pending_gen.take() {
            match pg.receiver.try_recv() {
                Ok(Some(data)) => {
                    self.gen_attempts = pg.attempts.load(Ordering::Relaxed);
                    self.model.install(data);
                    self.state = GameState::Running;
                    self.model.reveal(pg.click_x, pg.click_y);
                    self.check_outcome();
                }
                Ok(None) | Err(std::sync::mpsc::TryRecvError::Disconnected) => {}
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(30));
                    self.pending_gen = Some(pg);
                }
            }
        }

        self.draw_menubar(ctx);
        self.draw_statusbar(ctx);
        self.draw_game_panel(ctx);
        self.draw_modal(ctx);
        self.draw_generating_popup(ctx);

        if self.resize_pending {
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(self.window_size_for_grid(ctx)));
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
                    ui.add_enabled_ui(self.state == GameState::Running, |ui| {
                        if ui.button("Aide             Ctrl+H").clicked() {
                            self.trigger_hint(ctx);
                            ui.close_menu();
                        }
                    });
                    if ui.button("Meilleurs Scores").clicked() {
                        self.modal = Modal::Scores(ScoresState::load());
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
        // Ctrl+H hint shortcut
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::CTRL, Key::H))) {
            self.trigger_hint(ctx);
        }
    }

    fn draw_statusbar(&mut self, ctx: &Context) {
        TopBottomPanel::bottom("status").show(ctx, |ui| {
            let (_, _, cats) = Preferences::level_params(self.prefs.level);
            let level_name = Preferences::level_name(self.prefs.level);
            let text = match self.state {
                GameState::Lost => "Malheureusement, tu as perdu.".into(),
                GameState::Won if self.assisted => "Tu as gagné ! (avec aide : pas de score)".into(),
                GameState::Won => "Tu as gagné !".into(),
                _ if self.gen_attempts == 0 => format!("{} — {} BombaCat", level_name, cats),
                _ => format!("{} — {} BombaCat  (grille résoluble générée en {} essai{}.)",
                    level_name, cats,
                    self.gen_attempts,
                    if self.gen_attempts > 1 { "s" } else { "" }),
            };
            ui.add(Label::new(text).truncate());
        });
    }

    fn draw_game_panel(&mut self, ctx: &Context) {
        CentralPanel::default().show(ctx, |ui| {
            // Block grid input if a modal OR an egui popup (menu) is/was open.
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
                lcd_label(&mut child, self.model.cats_left());
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
                    ui.painter().rect_stroke(face_rect, 3.0, Stroke::new(1.5_f32, Color32::GRAY), egui::StrokeKind::Outside);
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

            // La taille des cases suit la place disponible dans la fenêtre.
            let avail = ui.available_size();
            self.chrome = ctx.screen_rect().size() - avail + Vec2::splat(1.0);
            let cell = ((avail.x + 0.5) / cols as f32)
                .min((avail.y + 0.5) / rows as f32)
                .floor()
                .clamp(MIN_CELL, MAX_CELL);

            let grid_size = vec2(cols as f32 * cell, rows as f32 * cell);
            let (outer_rect, grid_resp) = ui.allocate_exact_size(
                vec2(avail.x.max(grid_size.x), grid_size.y),
                Sense::click(),
            );
            let left = (outer_rect.center().x - grid_size.x / 2.0).round().max(outer_rect.left());
            let grid_rect = Rect::from_min_size(pos2(left, outer_rect.top()), grid_size);

            let cell_at = |pos: Pos2| -> Option<(usize, usize)> {
                if !grid_rect.contains(pos) {
                    return None;
                }
                let col = ((pos.x - grid_rect.left()) / cell) as usize;
                let row = ((pos.y - grid_rect.top()) / cell) as usize;
                (col < cols && row < rows).then_some((col, row))
            };
            let cell_rect = |col: usize, row: usize| {
                Rect::from_min_size(
                    grid_rect.min + vec2(col as f32 * cell, row as f32 * cell),
                    Vec2::splat(cell),
                )
            };

            // ── input ────────────────────────
            // Les boutons sont suivis à la main plutôt qu'avec `clicked()` :
            // l'action part au relâchement, sur la case sous la souris, même
            // si la souris a bougé ou si l'appui a duré.
            use PointerButton::{Middle, Primary, Secondary};
            let (pos, pressed, released, down) = ctx.input(|i| {
                let p = &i.pointer;
                (
                    p.latest_pos(),
                    [Primary, Secondary, Middle].map(|b| p.button_pressed(b)),
                    [Primary, Secondary, Middle].map(|b| p.button_released(b)),
                    [Primary, Secondary, Middle].map(|b| p.button_down(b)),
                )
            });
            let [l_pressed, r_pressed, m_pressed] = pressed;
            let [l_released, r_released, m_released] = released;
            let [l_down, r_down, m_down] = down;
            let hovered = pos.and_then(cell_at);

            let input_ok = active
                && !modal_open
                && !self.popup_was_open
                && self.pending_gen.is_none();

            if !input_ok {
                self.input = GridInput::default();
            } else {
                if (l_pressed || r_pressed || m_pressed)
                    && !self.input.armed
                    && grid_resp.contains_pointer()
                    && hovered.is_some()
                {
                    self.input = GridInput { armed: true, consumed: false };
                }

                if let (true, Some((x, y))) = (self.input.armed, hovered) {
                    let mut acted = false;

                    // Clic droit : marque posée dès l'appui.
                    if r_pressed && !l_down && !m_down {
                        self.right_click(x, y);
                        acted = true;
                    }
                    // Clic milieu : révéler autour d'un chiffre (ou triche).
                    if m_released {
                        if self.cheat {
                            self.model.reveal_all();
                            self.assisted = true;
                        } else {
                            self.chord_click(x, y);
                        }
                        acted = true;
                    }
                    if l_released && !self.input.consumed {
                        if r_down {
                            // gauche + droit
                            self.chord_click(x, y);
                            self.input.consumed = true;
                        } else if self.model.field(x, y).discovered {
                            // clic gauche sur un chiffre
                            self.chord_click(x, y);
                        } else {
                            self.left_click(ctx, x, y);
                        }
                        acted = true;
                    }
                    if r_released && l_down && !self.input.consumed {
                        // gauche + droit, droit relâché en premier
                        self.chord_click(x, y);
                        self.input.consumed = true;
                        acted = true;
                    }

                    if acted {
                        self.hint = None;
                    }
                }

                if !(l_down || r_down || m_down) {
                    self.input = GridInput::default();
                }
            }

            // pressing for face expression (button held over the grid)
            self.pressing = self.input.armed && l_down;

            // Cases dessinées enfoncées tant que le bouton est tenu.
            let mut pressed_cells: Vec<(usize, usize)> = Vec::new();
            if self.input.armed && !self.input.consumed && self.pending_gen.is_none() {
                if let Some((x, y)) = hovered {
                    let f = self.model.field(x, y);
                    let chord_gesture = (m_down && !self.cheat)
                        || (l_down && (r_down || f.discovered));
                    if chord_gesture {
                        if f.discovered && f.neighbours > 0 {
                            for ny in y.saturating_sub(1)..=(y + 1).min(rows - 1) {
                                for nx in x.saturating_sub(1)..=(x + 1).min(cols - 1) {
                                    let nf = self.model.field(nx, ny);
                                    if !nf.discovered && nf.flag == FLAG_NONE {
                                        pressed_cells.push((nx, ny));
                                    }
                                }
                            }
                        }
                    } else if l_down && f.flag == FLAG_NONE {
                        pressed_cells.push((x, y));
                    }
                }
            }

            // ── paint cells ──────────────────
            // (après le traitement des clics : l'image montre déjà le résultat)
            let active = matches!(self.state, GameState::NotStarted | GameState::Running);
            let exploded = self.model.exploded();
            let painter = ui.painter();
            for row in 0..rows {
                for col in 0..cols {
                    Self::draw_cell(
                        painter,
                        cell_rect(col, row),
                        self.model.field(col, row),
                        active,
                        pressed_cells.contains(&(col, row)),
                        exploded == Some((col, row)),
                        &self.tex,
                    );
                }
            }

            // hint blink overlay
            if let Some(hint) = self.hint {
                let now = ctx.input(|i| i.time);
                let elapsed = now - hint.start_time;
                if active && elapsed < 5.0 && hint.x < cols && hint.y < rows {
                    let blink_on = (elapsed * 2.5) as i32 % 2 == 0;
                    if blink_on {
                        let cr = cell_rect(hint.x, hint.y);
                        painter.rect_filled(cr, 0.0, Color32::from_rgba_unmultiplied(0, 180, 255, 100));
                        painter.rect_stroke(cr, 0.0,
                            Stroke::new(2.5_f32, Color32::from_rgb(0, 220, 255)),
                            StrokeKind::Outside);
                    }
                    ctx.request_repaint_after(std::time::Duration::from_millis(200));
                } else {
                    self.hint = None;
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
            Modal::Scores(_) => self.draw_scores_modal(ctx),
            Modal::SaveScore { .. } => self.draw_savescore_modal(ctx),
            Modal::About => self.draw_about_modal(ctx),
        }
    }

    fn draw_settings_modal(&mut self, ctx: &Context) {
        let Modal::Settings(ref st) = self.modal else { return };
        let mut st = st.clone();

        #[derive(PartialEq)] enum Action { None, Ok, Cancel }
        let mut action = Action::None;

        let resp = egui::Modal::new(Id::new("settings_modal")).backdrop_color(Color32::TRANSPARENT).show(ctx, |ui| {
            ui.heading("Préférences");
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Difficulté:");
                ComboBox::from_id_salt("lvl")
                    .selected_text(Preferences::level_name(st.level))
                    .show_ui(ui, |ui| {
                        for level in 0..LEVEL_COUNT {
                            ui.selectable_value(&mut st.level, level, Preferences::level_name(level));
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
        let Modal::Scores(ref mut st) = self.modal else { return };
        let mut close = false;
        let mut reset = false;
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
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (level, top) in st.tables.iter().enumerate() {
                        ui.collapsing(Preferences::level_name(level), |ui| {
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
                if st.confirm_reset {
                    ui.colored_label(
                        Color32::from_rgb(255, 100, 100),
                        "Effacer définitivement tous les scores ?",
                    );
                    ui.horizontal(|ui| {
                        // « Annuler » en premier : c'est lui qui se retrouve sous
                        // la souris, un double-clic n'efface donc rien.
                        if ui.button("Annuler").clicked() { st.confirm_reset = false; }
                        if ui.button("Oui, tout effacer").clicked() { reset = true; }
                    });
                } else {
                    ui.horizontal(|ui| {
                        if ui.button("Réinitialiser…").clicked() { st.confirm_reset = true; }
                        if ui.button("Fermer").clicked() { close = true; }
                    });
                }
            });

        if reset {
            let _ = scores::reset_all();
            self.modal = Modal::Scores(ScoresState::load());
        } else if close || !open {
            self.modal = Modal::None;
        }
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
                ui.label("Clic gauche — découvrir une case");
                ui.label("Clic droit — drapeau, puis « ? », puis rien");
                ui.label("Clic sur un chiffre (ou milieu, ou gauche + droit) —");
                ui.label("découvrir les voisines quand les drapeaux sont posés");
                ui.label("Ctrl+H — aide (la partie ne compte plus pour les scores)");
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
