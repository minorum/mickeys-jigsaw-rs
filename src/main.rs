pub mod assets;
pub mod audio;
pub mod decompress;
pub mod graphics;
pub mod puzzle;
pub mod setup;

use std::env;
use std::path::{Path, PathBuf};

use macroquad::prelude::*;

use crate::assets::{FontAsset, Preferences, PuzzleAsset, ToolboxAsset, load_ilbm_container};
use crate::audio::{AudioEngine, CommonSound};
use crate::decompress::IlbmImage;
use crate::graphics::{Framebuffer, SCREEN_H, SCREEN_W, render_scene_view_with_scratch};
use crate::puzzle::{PuzzleAction, PuzzleSession};
use crate::setup::SetupMenu;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BrowserAction {
    PrevPuzzle,
    NextPuzzle,
    CyclePieces,
    ToggleEdges,
    OpenSetup,
    StartOrResume,
}

const BROWSER_BUTTONS: [(BrowserAction, i32, i32, [u8; 3]); 6] = [
    (BrowserAction::PrevPuzzle, 6, 38, [0, 0, 170]),
    (BrowserAction::NextPuzzle, 48, 38, [0, 0, 170]),
    (BrowserAction::CyclePieces, 90, 68, [170, 0, 0]),
    (BrowserAction::ToggleEdges, 162, 60, [170, 0, 170]),
    (BrowserAction::OpenSetup, 226, 40, [85, 85, 85]),
    (BrowserAction::StartOrResume, 268, 48, [0, 170, 0]),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AppScreen {
    Title,
    Credits { page: usize },
    Setup { return_to_playing: bool },
    Browser,
    Playing,
}

struct BrowserPreviewCache {
    puzzle_id: usize,
    uses_ega: bool,
    fb: Box<Framebuffer>,
    white: u8,
    black: u8,
    button_bgs: [u8; BROWSER_BUTTONS.len()],
}

struct GameApp {
    game_dir: PathBuf,
    font: FontAsset,
    tbx: ToolboxAsset,
    tbx_ega: ToolboxAsset,
    title_images: Vec<IlbmImage>,
    credit_images: Vec<IlbmImage>,
    prefs: Preferences,
    setup_menu: SetupMenu,
    audio: AudioEngine,
    screen: AppScreen,
    screen_timer: f32,

    // Browser state
    selected_puzzle: usize,
    preview_asset: PuzzleAsset,
    preview_cache: Option<BrowserPreviewCache>,
    selected_grid_dim: usize,
    curved_edges: bool,

    // Active puzzle state
    session: Option<PuzzleSession>,
    session_use_ega: bool,
    error: Option<String>,
}

impl GameApp {
    fn new(game_dir: PathBuf, start_in_setup: bool) -> Result<Self, String> {
        let font = FontAsset::load(&game_dir.join("MJP.NFT"))?;
        let tbx = ToolboxAsset::load(&game_dir.join("MJP.TBX"), 0)?;
        let tbx_ega = ToolboxAsset::load(&game_dir.join("MJP.TBX"), 1)?;
        let title_images = load_ilbm_container(&game_dir.join("MJP.TTL"))?;
        let mut credit_images =
            load_ilbm_container(&game_dir.join("MJP.CRD")).unwrap_or_else(|err| {
                eprintln!("Credits unavailable: {err}");
                Vec::new()
            });
        credit_images.retain(|img| img.n_planes == 8);

        let cfg_path = game_dir.join("mjp_config.pre");
        let prefs = if cfg_path.exists() {
            Preferences::load(&cfg_path).unwrap_or_default()
        } else {
            Preferences::load(&game_dir.join("MJP.PRE")).unwrap_or_default()
        };

        let mut audio = AudioEngine::new(&game_dir, prefs.music_on != 0, prefs.sfx_on != 0);

        let selected_puzzle = 1usize;
        let use_ega = prefs.video_mode >= 2;
        let preview_asset = PuzzleAsset::load(&game_dir, selected_puzzle, use_ega)?;

        let setup_menu = SetupMenu::new(prefs);
        let screen = if start_in_setup {
            AppScreen::Setup {
                return_to_playing: false,
            }
        } else {
            audio.play_iss("TITLE", 0);
            AppScreen::Title
        };

        Ok(Self {
            game_dir,
            font,
            tbx,
            tbx_ega,
            title_images,
            credit_images,
            prefs,
            setup_menu,
            audio,
            screen,
            screen_timer: 0.0,
            selected_puzzle,
            preview_asset,
            preview_cache: None,
            selected_grid_dim: 3, // Default 3x3 = 9 pieces
            curved_edges: true,
            session: None,
            session_use_ega: use_ega,
            error: None,
        })
    }

    fn reload_preview(&mut self) {
        let use_ega = self.prefs.video_mode >= 2;
        match PuzzleAsset::load(&self.game_dir, self.selected_puzzle, use_ega) {
            Ok(asset) => {
                self.preview_asset = asset;
                self.error = None;
            }
            Err(err) => {
                eprintln!("{err}");
                self.selected_puzzle = self.preview_asset.id;
                self.error = Some(err);
            }
        }
    }

    fn sync_prefs_from_setup(&mut self) {
        let previous_ega = self.prefs.video_mode >= 2;
        let next_ega = self.setup_menu.prefs.video_mode >= 2;
        let previous_video = self.prefs.video_mode;
        if previous_ega != next_ega {
            let loaded = PuzzleAsset::load(&self.game_dir, self.selected_puzzle, next_ega)
                .and_then(|preview| {
                    let active = self
                        .session
                        .as_ref()
                        .map(|session| {
                            PuzzleAsset::load(&self.game_dir, session.puzzle_num, next_ega)
                        })
                        .transpose()?;
                    Ok((preview, active))
                });
            match loaded {
                Ok((preview, active)) => {
                    self.preview_asset = preview;
                    if let (Some(session), Some(asset)) = (&mut self.session, active) {
                        session.replace_scene_asset(asset);
                        self.audio.stop_all();
                    }
                    self.session_use_ega = next_ega;
                    self.error = None;
                }
                Err(error) => {
                    self.setup_menu.prefs.video_mode = previous_video;
                    self.error = Some(error);
                }
            }
        }
        self.prefs = self.setup_menu.prefs;
        self.audio
            .set_enabled(self.prefs.music_on != 0, self.prefs.sfx_on != 0);
        if let Some(ref mut sess) = self.session {
            sess.grid_lines = self.prefs.grid_lines_on != 0;
            sess.timer_enabled = self.prefs.timer_on != 0;
            sess.text_enabled = self.prefs.text_msg_on != 0;
        }
    }

    fn can_resume_selected(&self) -> bool {
        self.session.as_ref().is_some_and(|session| {
            !session.solved
                && session.puzzle_num == self.selected_puzzle
                && session.grid_dim == self.selected_grid_dim
                && (session.grid_dim == 1 || session.curved_edges == self.curved_edges)
                && self.session_use_ega == (self.prefs.video_mode >= 2)
        })
    }

    fn start_selected_puzzle(&mut self) {
        if self.can_resume_selected() {
            self.audio.stop_all();
            self.screen = AppScreen::Playing;
            self.error = None;
        } else {
            self.restart_selected_puzzle();
        }
    }

    fn restart_selected_puzzle(&mut self) {
        let use_ega = self.prefs.video_mode >= 2;
        match PuzzleAsset::load(&self.game_dir, self.selected_puzzle, use_ega) {
            Ok(asset) => {
                let mut sess = PuzzleSession::new(
                    self.selected_puzzle,
                    asset,
                    self.selected_grid_dim,
                    self.curved_edges,
                    self.prefs.grid_lines_on != 0,
                    self.prefs.timer_on != 0,
                );
                sess.text_enabled = self.prefs.text_msg_on != 0;
                self.session = Some(sess);
                self.session_use_ega = use_ega;
                self.screen = AppScreen::Playing;
                self.audio.play_common(CommonSound::StartPuzzle);
                self.error = None;
            }
            Err(err) => {
                eprintln!("{err}");
                self.error = Some(err);
            }
        }
    }

    fn step_puzzle(&mut self, delta: i32) {
        self.selected_puzzle = if delta < 0 {
            if self.selected_puzzle > 1 {
                self.selected_puzzle - 1
            } else {
                15
            }
        } else if self.selected_puzzle < 15 {
            self.selected_puzzle + 1
        } else {
            1
        };
        self.reload_preview();
        self.audio.play_common(if delta < 0 {
            CommonSound::PreviousPicture
        } else {
            CommonSound::NextPicture
        });
    }

    fn step_grid_dim(&mut self, delta: i32) {
        self.selected_grid_dim = if delta > 0 {
            if self.selected_grid_dim < 8 {
                self.selected_grid_dim + 1
            } else {
                1
            }
        } else if self.selected_grid_dim > 1 {
            self.selected_grid_dim - 1
        } else {
            8
        };
        self.audio.play_common(CommonSound::ChooseSize);
    }

    fn open_setup(&mut self, return_to_playing: bool) {
        self.setup_menu.prefs = self.prefs;
        self.setup_menu.error = None;
        self.screen = AppScreen::Setup { return_to_playing };
    }

    fn exit_setup(&mut self, return_to_playing: bool) {
        self.sync_prefs_from_setup();
        self.screen = if return_to_playing && self.session.is_some() {
            AppScreen::Playing
        } else {
            AppScreen::Browser
        };
    }

    fn update(&mut self, dt: f32, mx: i32, my: i32, left_clicked: bool, right_clicked: bool) {
        if !matches!(self.screen, AppScreen::Setup { .. }) {
            self.audio.update(dt);
        }
        self.update_input(dt, mx, my, left_clicked, right_clicked);
        self.audio
            .set_paused(matches!(self.screen, AppScreen::Setup { .. }));
    }

    fn update_input(&mut self, dt: f32, mx: i32, my: i32, left_clicked: bool, right_clicked: bool) {
        self.screen_timer += dt;

        // Global shortcut F4 opens Setup from anywhere
        if is_key_pressed(KeyCode::F4) && !matches!(self.screen, AppScreen::Setup { .. }) {
            if let Some(session) = &mut self.session {
                session.cancel_pointer_drag();
            }
            let in_play = self.screen == AppScreen::Playing;
            self.open_setup(in_play);
            return;
        }

        match self.screen {
            AppScreen::Title => {
                if left_clicked
                    || is_key_pressed(KeyCode::Space)
                    || is_key_pressed(KeyCode::Enter)
                    || self.screen_timer > 8.0
                {
                    self.screen = if self.credit_images.is_empty() {
                        AppScreen::Browser
                    } else {
                        AppScreen::Credits { page: 0 }
                    };
                    self.screen_timer = 0.0;
                }
            }
            AppScreen::Credits { page } => {
                if left_clicked
                    || is_key_pressed(KeyCode::Space)
                    || is_key_pressed(KeyCode::Enter)
                    || self.screen_timer > 5.0
                {
                    if page + 1 < self.credit_images.len() {
                        self.screen = AppScreen::Credits { page: page + 1 };
                        self.screen_timer = 0.0;
                    } else {
                        self.screen = AppScreen::Browser;
                        self.screen_timer = 0.0;
                    }
                } else if is_key_pressed(KeyCode::Escape) {
                    self.screen = AppScreen::Browser;
                    self.screen_timer = 0.0;
                }
            }
            AppScreen::Setup { return_to_playing } => {
                if is_key_pressed(KeyCode::Up) {
                    self.setup_menu.move_up();
                } else if is_key_pressed(KeyCode::Down) {
                    self.setup_menu.move_down();
                } else if is_key_pressed(KeyCode::Left) {
                    self.setup_menu.activate_or_cycle(-1, &self.game_dir);
                    self.sync_prefs_from_setup();
                } else if is_key_pressed(KeyCode::Right)
                    || is_key_pressed(KeyCode::Enter)
                    || is_key_pressed(KeyCode::Space)
                {
                    if self.setup_menu.activate_or_cycle(1, &self.game_dir) {
                        self.exit_setup(return_to_playing);
                    } else {
                        self.sync_prefs_from_setup();
                    }
                } else if is_key_pressed(KeyCode::Escape) {
                    self.exit_setup(return_to_playing);
                } else if left_clicked {
                    if self.setup_menu.handle_click(mx, my, &self.game_dir) {
                        self.exit_setup(return_to_playing);
                    } else {
                        self.sync_prefs_from_setup();
                    }
                }
            }
            AppScreen::Browser => {
                if is_key_pressed(KeyCode::Left) {
                    self.step_puzzle(-1);
                } else if is_key_pressed(KeyCode::Right) {
                    self.step_puzzle(1);
                } else if is_key_pressed(KeyCode::Up) {
                    self.step_grid_dim(1);
                } else if is_key_pressed(KeyCode::Down) {
                    self.step_grid_dim(-1);
                } else if is_key_pressed(KeyCode::Tab) || is_key_pressed(KeyCode::E) {
                    if self.selected_grid_dim != 1 {
                        self.curved_edges = !self.curved_edges;
                    }
                } else if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::Space) {
                    self.start_selected_puzzle();
                } else if is_key_pressed(KeyCode::R) {
                    self.restart_selected_puzzle();
                } else if is_key_pressed(KeyCode::S) {
                    self.open_setup(false);
                } else if left_clicked {
                    self.handle_browser_click(mx, my);
                }
            }
            AppScreen::Playing => {
                let mut open_menu = false;

                if is_key_pressed(KeyCode::Escape) {
                    open_menu = true;
                }

                if let Some(ref mut sess) = self.session {
                    sess.update(dt, &mut self.audio);

                    if right_clicked {
                        sess.keyboard_pointer = None;
                    }
                    if !left_clicked && !right_clicked {
                        let dx =
                            i32::from(is_key_down(KeyCode::D)) - i32::from(is_key_down(KeyCode::A));
                        let dy =
                            i32::from(is_key_down(KeyCode::S)) - i32::from(is_key_down(KeyCode::W));
                        if dx != 0 || dy != 0 {
                            sess.move_keyboard_pointer(dx as f32, dy as f32, dt);
                        }
                        if is_key_pressed(KeyCode::Enter)
                            && sess.handle_keyboard_activate(&mut self.audio)
                                == Some(PuzzleAction::OpenMenu)
                        {
                            open_menu = true;
                        }
                    }

                    if (right_clicked || is_key_pressed(KeyCode::Space)) && !sess.solved {
                        sess.toggle_toolbox();
                    }
                    if is_key_pressed(KeyCode::F1) {
                        sess.trigger_hint();
                    }
                    if is_key_pressed(KeyCode::F2) {
                        sess.auto_solve(&mut self.audio);
                    }
                    if is_key_pressed(KeyCode::F3) || is_key_pressed(KeyCode::G) {
                        sess.grid_lines = !sess.grid_lines;
                        self.prefs.grid_lines_on = u16::from(sess.grid_lines);
                    }
                    if is_key_pressed(KeyCode::F5) || is_key_pressed(KeyCode::P) {
                        sess.export_printable_picture(&self.game_dir);
                    }
                    if is_key_pressed(KeyCode::Up) {
                        sess.scroll_tray(-1);
                    }
                    if is_key_pressed(KeyCode::Down) {
                        sess.scroll_tray(1);
                    }
                    let (_, wheel_y) = mouse_wheel();
                    if wheel_y > 0.0 {
                        sess.scroll_tray(-1);
                    } else if wheel_y < 0.0 {
                        sess.scroll_tray(1);
                    }

                    if left_clicked
                        && let Some(PuzzleAction::OpenMenu) =
                            sess.handle_pointer_press(mx, my, &mut self.audio)
                    {
                        open_menu = true;
                    }

                    if is_mouse_button_released(MouseButton::Left) {
                        sess.handle_pointer_release(mx, my, &mut self.audio);
                    }
                }

                if open_menu {
                    if let Some(session) = &mut self.session {
                        session.cancel_pointer_drag();
                    }
                    self.audio.stop_all();
                    self.audio.play_common(CommonSound::OtherPicture);
                    self.screen = AppScreen::Browser;
                }
            }
        }
    }

    fn handle_browser_click(&mut self, mx: i32, my: i32) {
        // Bottom control bar buttons (`y = 170..196`)
        if (170..=196).contains(&my) {
            for &(action, bx, bw, _) in &BROWSER_BUTTONS {
                if (bx..=bx + bw).contains(&mx) {
                    match action {
                        BrowserAction::PrevPuzzle => self.step_puzzle(-1),
                        BrowserAction::NextPuzzle => self.step_puzzle(1),
                        BrowserAction::CyclePieces => self.step_grid_dim(1),
                        BrowserAction::ToggleEdges => {
                            if self.selected_grid_dim != 1 {
                                self.curved_edges = !self.curved_edges;
                            }
                        }
                        BrowserAction::OpenSetup => {
                            self.open_setup(false);
                        }
                        BrowserAction::StartOrResume => self.start_selected_puzzle(),
                    }
                    return;
                }
            }
        }
        // Clicking the puzzle preview itself starts the puzzle!
        if my < 168 {
            self.start_selected_puzzle();
        }
    }

    fn render(&mut self, fb: &mut Framebuffer, mx: i32, my: i32) {
        fb.reset_clip();
        match self.screen {
            AppScreen::Title => {
                if let Some(img) = self.title_images.first() {
                    fb.draw_ilbm(img);
                }
            }
            AppScreen::Credits { page } => {
                if let Some(img) = self
                    .credit_images
                    .get(page)
                    .or_else(|| self.credit_images.first())
                {
                    fb.draw_ilbm(img);
                }
            }
            AppScreen::Setup { .. } => {
                fb.reset_default_palette();
                self.setup_menu.render(fb, &self.font);
            }
            AppScreen::Browser => {
                let key = (self.preview_asset.id, self.preview_asset.uses_ega());
                let needs_rebuild = self
                    .preview_cache
                    .as_ref()
                    .is_none_or(|c| (c.puzzle_id, c.uses_ega) != key);
                if needs_rebuild {
                    let mut cached_fb = self
                        .preview_cache
                        .take()
                        .map(|c| c.fb)
                        .unwrap_or_else(|| Box::new(Framebuffer::new()));
                    let mut scratch = Framebuffer::new();
                    render_scene_view_with_scratch(
                        &mut cached_fb,
                        &mut scratch,
                        &self.preview_asset,
                        self.preview_asset.static_frame_idx(),
                    );
                    let white = cached_fb.color_index([255; 3]);
                    let black = cached_fb.color_index([0; 3]);
                    let mut button_bgs = [0u8; BROWSER_BUTTONS.len()];
                    for (dst, &(_, _, _, rgb)) in button_bgs.iter_mut().zip(&BROWSER_BUTTONS) {
                        *dst = cached_fb.color_index(rgb);
                    }
                    self.preview_cache = Some(BrowserPreviewCache {
                        puzzle_id: key.0,
                        uses_ega: key.1,
                        fb: cached_fb,
                        white,
                        black,
                        button_bgs,
                    });
                }
                let cache = self.preview_cache.as_ref().unwrap();
                fb.palette = cache.fb.palette;
                fb.pixels.copy_from_slice(&cache.fb.pixels);
                let white = cache.white;
                let black = cache.black;

                // Top banner showing puzzle title
                fb.fill_rect(6, 4, SCREEN_W - 12, 15, black);
                fb.draw_rect_outline(6, 4, SCREEN_W - 12, 15, white);
                let can_resume = self.can_resume_selected();
                let title = if can_resume {
                    format!("Puzzle {:02} of 15   R: restart", self.selected_puzzle)
                } else {
                    format!("Puzzle {:02} of 15", self.selected_puzzle)
                };
                fb.draw_text_shadowed(&self.font, &title, 14, 7, black, white);

                // Bottom interactive control bar (`y = 168..198`)
                fb.fill_rect(2, 168, SCREEN_W - 4, 30, black);
                fb.draw_rect_outline(2, 168, SCREEN_W - 4, 30, white);

                let pcs_label = if self.selected_grid_dim == 1 {
                    format!("{} PARTS", self.preview_asset.preset_piece_count())
                } else {
                    format!("{} PCS", self.selected_grid_dim * self.selected_grid_dim)
                };
                let edge_label = if self.selected_grid_dim == 1 {
                    "ORIGINAL"
                } else if self.curved_edges {
                    "CURVED"
                } else {
                    "SQUARE"
                };

                for (&(action, x, w, _), &bg) in BROWSER_BUTTONS.iter().zip(&cache.button_bgs) {
                    let label = match action {
                        BrowserAction::PrevPuzzle => "< PREV",
                        BrowserAction::NextPuzzle => "NEXT >",
                        BrowserAction::CyclePieces => pcs_label.as_str(),
                        BrowserAction::ToggleEdges => edge_label,
                        BrowserAction::OpenSetup => "SETUP",
                        BrowserAction::StartOrResume => {
                            if can_resume {
                                "RESUME"
                            } else {
                                "PLAY!"
                            }
                        }
                    };
                    fb.fill_rect(x, 172, w, 22, bg);
                    fb.draw_rect_outline(x, 172, w, 22, white);
                    let tw = self.font.measure_text(label);
                    let tx = x + (w - tw) / 2;
                    fb.draw_text_shadowed(&self.font, label, tx.max(x + 2), 178, black, white);
                }
            }
            AppScreen::Playing => {
                if let Some(ref mut sess) = self.session {
                    let toolbox = if sess.asset.uses_ega() {
                        &self.tbx_ega
                    } else {
                        &self.tbx
                    };
                    let (px, py) = sess
                        .keyboard_pointer
                        .map(|(x, y)| (x.round() as i32, y.round() as i32))
                        .unwrap_or((mx, my));
                    sess.render(fb, toolbox, &self.font, px, py);
                }
            }
        }
        let has_caption = self.prefs.text_msg_on != 0
            && matches!(self.screen, AppScreen::Browser | AppScreen::Playing)
            && self.audio.common_caption().is_some();
        let has_pointer = self.screen == AppScreen::Playing
            && self
                .session
                .as_ref()
                .is_some_and(|session| session.keyboard_pointer.is_some());
        if has_caption || has_pointer || self.error.is_some() {
            let black = fb.color_index([0; 3]);
            let white = fb.color_index([255; 3]);
            if has_caption && let Some(caption) = self.audio.common_caption() {
                let line_count = caption.split('\r').count() as i32;
                let height = line_count * self.font.height as i32 + 8;
                let y = 164 - height;
                fb.fill_rect(4, y, SCREEN_W - 8, height, white);
                fb.draw_rect_outline(4, y, SCREEN_W - 8, height, black);
                for (row, line) in caption.split('\r').enumerate() {
                    let x = (SCREEN_W - self.font.measure_text(line)) / 2;
                    fb.draw_text(
                        &self.font,
                        line,
                        x,
                        y + 4 + row as i32 * self.font.height as i32,
                        black,
                    );
                }
            }
            if has_pointer
                && let Some((x, y)) = self
                    .session
                    .as_ref()
                    .and_then(|session| session.keyboard_pointer)
            {
                let (x, y) = (x.round() as i32, y.round() as i32);
                fb.draw_rect_outline(x - 3, y - 3, 7, 7, black);
                fb.draw_line(x - 2, y, x + 2, y, white);
                fb.draw_line(x, y - 2, x, y + 2, white);
            }
            if self.error.is_some() {
                fb.fill_rect(0, 20, SCREEN_W, 26, black);
                fb.draw_text(&self.font, "Asset load failed (see terminal)", 5, 23, white);
            }
        }
    }
}

fn find_game_dir() -> PathBuf {
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    if cwd.join("MJP.NFT").exists() {
        return cwd;
    }
    let exe_dir = env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."));
    if exe_dir.join("MJP.NFT").exists() {
        return exe_dir;
    }
    cwd
}

fn window_conf() -> Conf {
    Conf {
        window_title: "Mickey's Jigsaw Puzzles (1991 DOS Recreation)".to_string(),
        window_width: 960,
        window_height: 720,
        window_resizable: true,
        ..Default::default()
    }
}

fn game_viewport(width: f32, height: f32, playing: bool) -> Rect {
    let (padding, top, bottom) = if playing && width >= 480.0 && height >= 360.0 {
        (16.0, 84.0, 44.0)
    } else {
        (0.0, 0.0, 0.0)
    };
    let scale = ((width - padding * 2.0) / SCREEN_W as f32)
        .min((height - top - bottom) / SCREEN_H as f32)
        .max(f32::EPSILON);
    let w = SCREEN_W as f32 * scale;
    let h = SCREEN_H as f32 * scale;
    Rect::new(
        (width - w) * 0.5,
        top + (height - top - bottom - h) * 0.5,
        w,
        h,
    )
}

#[expect(clippy::too_many_arguments)]
fn render_desktop_hud(
    session: &PuzzleSession,
    win_w: f32,
    win_h: f32,
    reference: &Texture2D,
    cached_ref_key: &mut Option<(usize, bool)>,
    rgba_buf: &mut [u8],
    reference_button: Rect,
    reference_hovered: bool,
    reference_available: bool,
    show_reference: bool,
) {
    let ink = Color::from_hex(0xf5edd8);
    let muted = Color::from_hex(0xb2c4ca);
    let accent = Color::from_hex(0xedbe5e);
    draw_rectangle(0.0, 0.0, win_w, 80.0, Color::from_hex(0x182831));
    let ref_key = (session.puzzle_num, session.asset.uses_ega());
    if *cached_ref_key != Some(ref_key) {
        session.full_picture_fb.to_rgba8(rgba_buf);
        reference.update_from_bytes(SCREEN_W as u32, SCREEN_H as u32, rgba_buf);
        *cached_ref_key = Some(ref_key);
    }
    draw_texture_ex(
        reference,
        16.0,
        10.0,
        WHITE,
        DrawTextureParams {
            dest_size: Some(vec2(83.0, 60.0)),
            source: Some(Rect::new(2.0, 2.0, 270.0, 196.0)),
            ..Default::default()
        },
    );
    let compact = win_w < 720.0;
    draw_text(
        if compact {
            "Mickey's Puzzles"
        } else {
            "Mickey's Jigsaw Puzzles"
        },
        112.0,
        31.0,
        if compact { 20.0 } else { 25.0 },
        ink,
    );
    let placed = session.pieces.iter().filter(|p| p.placed).count();
    draw_text(
        &if compact {
            format!(
                "{:02}  /  {} of {}",
                session.puzzle_num,
                placed,
                session.pieces.len()
            )
        } else {
            format!(
                "Picture {:02}   /   {} of {} pieces placed",
                session.puzzle_num,
                placed,
                session.pieces.len()
            )
        },
        112.0,
        57.0,
        18.0,
        muted,
    );
    if session.timer_enabled {
        let time = session.formatted_time();
        let width = measure_text(&time, None, 24, 1.0).width;
        draw_text(&time, win_w - width - 20.0, 29.0, 24.0, ink);
    }
    draw_rectangle(
        reference_button.x,
        reference_button.y,
        reference_button.w,
        reference_button.h,
        if show_reference {
            accent
        } else if reference_hovered && reference_available {
            Color::from_hex(0x496675)
        } else {
            Color::from_hex(0x364c57)
        },
    );
    draw_text(
        if session.solved {
            "Picture complete!"
        } else if show_reference {
            "Release to keep playing"
        } else {
            "Hold: picture  [Tab]"
        },
        reference_button.x + 12.0,
        reference_button.y + 21.0,
        18.0,
        if show_reference {
            Color::from_hex(0x182831)
        } else {
            ink
        },
    );
    draw_rectangle(0.0, win_h - 36.0, win_w, 36.0, Color::from_hex(0x182831));
    let help = if compact {
        "Tab: picture   H: guide   F1: hint   Esc: menu"
    } else if show_reference {
        "Picture reference  /  progress and timer paused"
    } else if session.solved {
        "Click the picture to play   /   Esc: choose another puzzle"
    } else if session.held_piece.is_some() {
        "Place on the board   /   Click the tray to return the piece"
    } else {
        "Pick and place   /   H: picture guide   G: edges   F1: hint   Esc: menu"
    };
    draw_text(help, 16.0, win_h - 13.0, 18.0, muted);
}

#[macroquad::main(window_conf)]
async fn main() {
    let args: Vec<String> = env::args().collect();
    let start_in_setup = args
        .iter()
        .any(|a| a.eq_ignore_ascii_case("/S") || a.eq_ignore_ascii_case("--setup"));

    let game_dir = find_game_dir();
    let mut app = match GameApp::new(game_dir, start_in_setup) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("Failed to initialize Mickey's Jigsaw Puzzles: {}", e);
            return;
        }
    };

    let mut fb = Framebuffer::new();
    let mut rgba_buf = vec![0u8; (SCREEN_W as usize) * (SCREEN_H as usize) * 4];
    let texture = Texture2D::from_rgba8(SCREEN_W as u16, SCREEN_H as u16, &rgba_buf);
    texture.set_filter(FilterMode::Nearest);
    let reference = Texture2D::from_rgba8(SCREEN_W as u16, SCREEN_H as u16, &rgba_buf);
    reference.set_filter(FilterMode::Nearest);
    let mut cached_ref_key: Option<(usize, bool)> = None;
    let mut picture_guide = true;
    let mut mouse_reference = false;

    loop {
        let dt = get_frame_time().clamp(0.0, 0.25);

        let win_w = screen_width();
        let win_h = screen_height();
        let playing = app.screen == AppScreen::Playing;
        if playing && is_key_pressed(KeyCode::H) {
            picture_guide = !picture_guide;
        }
        let viewport = game_viewport(win_w, win_h, playing);
        let scale = viewport.w / SCREEN_W as f32;

        let (raw_mx, raw_my) = mouse_position();
        let mx = ((raw_mx - viewport.x) / scale).floor() as i32;
        let my = ((raw_my - viewport.y) / scale).floor() as i32;

        let inside = (0..SCREEN_W).contains(&mx) && (0..SCREEN_H).contains(&my);
        let left_clicked = inside && is_mouse_button_pressed(MouseButton::Left);
        let right_clicked = inside && is_mouse_button_pressed(MouseButton::Right);

        let reference_button = Rect::new(win_w - 210.0, 42.0, 194.0, 30.0);
        let reference_hovered =
            win_w >= 480.0 && win_h >= 360.0 && reference_button.contains(vec2(raw_mx, raw_my));
        let reference_available = playing && app.session.as_ref().is_some_and(|s| !s.solved);
        mouse_reference = reference_available
            && is_mouse_button_down(MouseButton::Left)
            && (mouse_reference
                || (reference_hovered && is_mouse_button_pressed(MouseButton::Left)));
        let show_reference = reference_available && (is_key_down(KeyCode::Tab) || mouse_reference);
        if show_reference {
            if let Some(session) = &mut app.session {
                session.cancel_pointer_drag();
            }
        } else {
            app.update(dt, mx, my, left_clicked, right_clicked);
        }
        if show_reference {
            app.session.as_ref().unwrap().render_reference(&mut fb);
        } else {
            app.render(&mut fb, mx, my);
        }

        fb.to_rgba8(&mut rgba_buf);
        if picture_guide
            && !show_reference
            && app.screen == AppScreen::Playing
            && let Some(session) = &mut app.session
        {
            let (px, py) = session
                .keyboard_pointer
                .map(|(x, y)| (x.round() as i32, y.round() as i32))
                .unwrap_or((mx, my));
            session.blend_picture_guide(&mut rgba_buf, px, py);
        }
        texture.update_from_bytes(SCREEN_W as u32, SCREEN_H as u32, &rgba_buf);

        let viewport = game_viewport(win_w, win_h, app.screen == AppScreen::Playing);
        clear_background(Color::from_hex(0x263640));
        draw_texture_ex(
            &texture,
            viewport.x,
            viewport.y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(viewport.w, viewport.h)),
                ..Default::default()
            },
        );

        if app.screen == AppScreen::Playing && win_w >= 480.0 && win_h >= 360.0 {
            if let Some(session) = &app.session {
                render_desktop_hud(
                    session,
                    win_w,
                    win_h,
                    &reference,
                    &mut cached_ref_key,
                    &mut rgba_buf,
                    reference_button,
                    reference_hovered,
                    reference_available,
                    show_reference,
                );
            }
        } else if app.screen == AppScreen::Playing
            && let Some(session) = app.session.as_ref().filter(|s| s.timer_enabled)
        {
            draw_text(session.formatted_time(), win_w - 100.0, 18.0, 18.0, WHITE);
        }

        next_frame().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::PrintAsset;
    use crate::audio::{synthesize_iss_song, synthesize_nnv_phrase};

    #[test]
    fn play_viewport_preserves_scene_and_input_coordinates() {
        for (width, height) in [
            (960.0, 720.0),
            (1700.0, 1380.0),
            (480.0, 360.0),
            (320.0, 200.0),
        ] {
            let viewport = game_viewport(width, height, true);
            assert!(viewport.x >= 0.0 && viewport.y >= 0.0);
            assert!(viewport.x + viewport.w <= width + 0.01);
            assert!(viewport.y + viewport.h <= height + 0.01);
            assert!((viewport.w / viewport.h - 1.6).abs() < 0.001);
            let scale = viewport.w / SCREEN_W as f32;
            for (x, y) in [(20.0, 40.0), (115.0, 51.0), (250.0, 149.0)] {
                let point = vec2(
                    viewport.x + (x + 0.5) * scale,
                    viewport.y + (y + 0.5) * scale,
                );
                assert_eq!(((point.x - viewport.x) / scale).floor() as i32, x as i32);
                assert_eq!(((point.y - viewport.y) / scale).floor() as i32, y as i32);
            }
        }
    }

    #[test]
    fn picture_guide_preserves_placed_and_held_artwork() {
        let dir = test_game_dir();
        let font = FontAsset::load(&dir.join("MJP.NFT")).unwrap();
        for ega in [false, true] {
            let asset = PuzzleAsset::load(&dir, 1, ega).unwrap();
            let toolbox = ToolboxAsset::load(&dir.join("MJP.TBX"), usize::from(ega)).unwrap();
            let mut session = PuzzleSession::new(1, asset, 3, true, false, false);
            let original = session.full_picture_fb.pixels;
            session.pieces[0].placed = true;
            session.held_piece = Some(1);
            let mut fb = Framebuffer::new();
            session.render(&mut fb, &toolbox, &font, 160, 100);
            let mut before = vec![0; 320 * 200 * 4];
            fb.to_rgba8(&mut before);
            let mut after = before.clone();
            session.blend_picture_guide(&mut after, 160, 100);
            assert_ne!(before, after, "ega={ega}");
            for (index, held) in [(0, false), (1, true)] {
                let piece = &session.pieces[index];
                let (x, y) = if held {
                    (
                        160 - piece.base_w / 2 + piece.target_x - piece.base_x,
                        100 - piece.base_h / 2 + piece.target_y - piece.base_y,
                    )
                } else {
                    (
                        session.playfield_x() + piece.target_x,
                        session.playfield_y() + piece.target_y,
                    )
                };
                for dy in 0..i32::from(piece.sprite.sub_h) {
                    for dx in 0..i32::from(piece.sprite.width) {
                        let source = (dy + i32::from(piece.sprite.sub_y)) as usize
                            * usize::from(piece.sprite.stride)
                            + (dx + i32::from(piece.sprite.sub_x)) as usize;
                        if !(0..320).contains(&(x + dx))
                            || !(0..200).contains(&(y + dy))
                            || !piece.sprite.is_opaque(source)
                        {
                            continue;
                        }
                        let offset = (((y + dy) * 320 + x + dx) * 4) as usize;
                        assert_eq!(
                            &before[offset..offset + 4],
                            &after[offset..offset + 4],
                            "ega={ega}, piece={index}"
                        );
                    }
                }
            }
            assert_eq!(session.full_picture_fb.pixels, original);
            session.solved = true;
            let mut solved = before.clone();
            session.blend_picture_guide(&mut solved, 160, 100);
            assert_eq!(solved, before);
        }
    }

    #[test]
    fn completed_picture_is_unchanged_by_timer_setting() {
        let dir = test_game_dir();
        let font = FontAsset::load(&dir.join("MJP.NFT")).unwrap();
        for ega in [false, true] {
            let asset = PuzzleAsset::load(&dir, 1, ega).unwrap();
            let toolbox = ToolboxAsset::load(&dir.join("MJP.TBX"), usize::from(ega)).unwrap();
            let mut session = PuzzleSession::new(1, asset, 2, true, false, false);
            session.solved = true;
            session.elapsed_secs = 3661.0;
            let mut without_timer = Framebuffer::new();
            session.render(&mut without_timer, &toolbox, &font, 160, 100);
            session.timer_enabled = true;
            let mut with_timer = Framebuffer::new();
            session.render(&mut with_timer, &toolbox, &font, 160, 100);
            let changed = with_timer
                .pixels
                .iter()
                .zip(&without_timer.pixels)
                .filter(|(a, b)| a != b)
                .count();
            assert_eq!(changed, 0, "ega={ega}: timer altered completed artwork");
        }
    }

    fn test_game_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    fn test_browser() -> GameApp {
        let game_dir = test_game_dir();
        let prefs = Preferences {
            music_on: 0,
            sfx_on: 0,
            ..Preferences::default()
        };
        GameApp {
            font: FontAsset::load(&game_dir.join("MJP.NFT")).unwrap(),
            tbx: ToolboxAsset::load(&game_dir.join("MJP.TBX"), 0).unwrap(),
            tbx_ega: ToolboxAsset::load(&game_dir.join("MJP.TBX"), 1).unwrap(),
            title_images: Vec::new(),
            credit_images: Vec::new(),
            setup_menu: SetupMenu::new(prefs),
            prefs,
            audio: AudioEngine::new(&game_dir, false, false),
            screen: AppScreen::Browser,
            screen_timer: 0.0,
            selected_puzzle: 1,
            preview_asset: PuzzleAsset::load(&game_dir, 1, false).unwrap(),
            preview_cache: None,
            selected_grid_dim: 3,
            curved_edges: true,
            session: None,
            session_use_ega: false,
            error: None,
            game_dir,
        }
    }

    #[test]
    fn preset_progress_survives_resume_and_video_switching() {
        let mut app = test_browser();
        app.selected_grid_dim = 1;
        app.start_selected_puzzle();
        let session = app.session.as_mut().unwrap();
        let first = session.tray_order.remove(0);
        session.pieces[first].placed = true;
        let held = session.tray_order.remove(0);
        session.held_piece = Some(held);
        let order = session.tray_order.clone();
        app.screen = AppScreen::Browser;
        app.curved_edges = !app.curved_edges;
        assert!(app.can_resume_selected());
        app.start_selected_puzzle();
        let ega = PuzzleAsset::load(&app.game_dir, 1, true).unwrap();
        app.session.as_mut().unwrap().replace_scene_asset(ega);
        let session = app.session.as_ref().unwrap();
        assert_eq!(session.grid_dim, 1);
        assert!(session.pieces[first].placed);
        assert_eq!(session.held_piece, Some(held));
        assert_eq!(session.tray_order, order);
        assert!(session.asset.uses_ega());
        assert!((11..=16).contains(&session.pieces.len()));
    }

    #[test]
    fn common_captions_render_and_respect_text_preference() {
        let mut app = test_browser();
        for ega in [false, true] {
            app.preview_asset = PuzzleAsset::load(&app.game_dir, 1, ega).unwrap();
            app.prefs.text_msg_on = 0;
            let mut baseline = Framebuffer::new();
            app.render(&mut baseline, 160, 100);
            app.audio.play_common(CommonSound::ChooseSize);
            let mut hidden = Framebuffer::new();
            app.render(&mut hidden, 160, 100);
            assert_eq!(baseline.pixels, hidden.pixels);
            app.prefs.text_msg_on = 1;
            let mut shown = Framebuffer::new();
            app.render(&mut shown, 160, 100);
            assert_ne!(baseline.pixels, shown.pixels);
            assert_eq!(&baseline.pixels[164 * 320..], &shown.pixels[164 * 320..]);
            app.audio.update(60.0);
            let mut expired = Framebuffer::new();
            app.render(&mut expired, 160, 100);
            assert_eq!(baseline.pixels, expired.pixels);
        }
    }

    #[test]
    fn preset_pickup_captions_have_visible_glyphs() {
        let mut app = test_browser();
        app.prefs.text_msg_on = 1;
        app.selected_grid_dim = 1;
        for ega in [false, true] {
            app.prefs.video_mode = if ega { 2 } else { 0 };
            for num in 1..=15 {
                app.selected_puzzle = num;
                app.restart_selected_puzzle();
                let session = app.session.as_mut().unwrap();
                session.handle_click(20, 40, &mut app.audio);
                let held = session.held_piece.unwrap();
                let piece = &session.pieces[held];
                let (x, y) = (
                    session.playfield_x() + piece.base_x + piece.base_w / 2,
                    session.playfield_y() + piece.base_y + piece.base_h / 2,
                );
                session.handle_click(x, y, &mut app.audio);
                let mut fb = Framebuffer::new();
                app.render(&mut fb, 0, 0);
                let black = fb.color_index([0; 3]);
                let count = (150..160)
                    .flat_map(|y| (5..315).map(move |x| (x, y)))
                    .filter(|&(x, y)| fb.get_pixel(x, y) == black)
                    .count();
                assert!(count > 100, "P{num:02} ega={ega} glyph pixels={count}");
            }
        }
    }

    #[test]
    fn setup_video_change_preserves_active_progress_and_rolls_back_failure() {
        let mut app = test_browser();
        app.start_selected_puzzle();
        let session = app.session.as_mut().unwrap();
        session.handle_click(20, 40, &mut app.audio);
        let placed = session.held_piece.unwrap();
        let piece = &session.pieces[placed];
        let target = (
            session.playfield_x() + piece.base_x + piece.base_w / 2,
            session.playfield_y() + piece.base_y + piece.base_h / 2,
        );
        session.handle_click(target.0, target.1, &mut app.audio);
        session.handle_click(20, 40, &mut app.audio);
        session.elapsed_secs = 123.0;
        session.tray_scroll = 1;
        session.toolbox_open = false;
        let held = session.held_piece;
        session.hint_piece = held.map(|index| (index, 1.0));
        let tray = session.tray_order.clone();
        for mode in [2, 1] {
            app.setup_menu.prefs.video_mode = mode;
            app.sync_prefs_from_setup();
            let session = app.session.as_ref().unwrap();
            assert_eq!(session.asset.sprites[0].bpp, if mode == 2 { 4 } else { 8 });
            assert!(session.pieces[placed].placed);
            assert_eq!(session.held_piece, held);
            assert_eq!(session.tray_order, tray);
            assert_eq!(session.tray_scroll, 1);
            assert!(!session.toolbox_open);
            assert_eq!(session.hint_piece, held.map(|index| (index, 1.0)));
            assert_eq!(session.elapsed_secs, 123.0);
            assert!(app.can_resume_selected());
        }
        let palette = app.session.as_ref().unwrap().asset.palette;
        app.game_dir = PathBuf::from("/dev/null");
        app.setup_menu.prefs.video_mode = 2;
        app.sync_prefs_from_setup();
        assert!(app.error.is_some());
        assert_eq!(app.prefs.video_mode, 1);
        assert_eq!(app.setup_menu.prefs.video_mode, 1);
        let session = app.session.as_ref().unwrap();
        assert_eq!(session.asset.palette, palette);
        assert_eq!(session.held_piece, held);
        assert!(session.pieces[placed].placed);
    }

    #[test]
    fn setup_video_change_keeps_solved_state_and_supports_ega_fallback() {
        let mut app = test_browser();
        app.selected_puzzle = 6;
        app.start_selected_puzzle();
        app.session.as_mut().unwrap().auto_solve(&mut app.audio);
        assert!(app.session.as_ref().unwrap().active_anim.is_some());
        app.setup_menu.prefs.video_mode = 2;
        app.sync_prefs_from_setup();
        let session = app.session.as_ref().unwrap();
        assert!(session.solved);
        assert!(session.pieces.iter().all(|piece| piece.placed));
        assert!(session.active_anim.is_none());
        assert_eq!(session.asset.sprites[0].bpp, 8);
        assert!(app.session_use_ega);
    }

    #[test]
    fn video_switch_is_atomic_when_preview_and_paused_puzzle_differ() {
        let mut app = test_browser();
        app.start_selected_puzzle();
        app.selected_puzzle = 2;
        app.reload_preview();
        let old_preview = app.preview_asset.palette;
        let old_active = app.session.as_ref().unwrap().asset.palette;
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("mickey-video-{unique}"));
        std::fs::create_dir(&dir).unwrap();
        std::fs::copy(app.game_dir.join("P02.PZ4"), dir.join("P02.PZ4")).unwrap();
        app.game_dir = dir.clone();
        app.setup_menu.prefs.video_mode = 2;
        app.sync_prefs_from_setup();
        app.game_dir = test_game_dir();
        std::fs::remove_dir_all(dir).unwrap();
        assert!(app.error.is_some());
        assert_eq!(app.prefs.video_mode, 1);
        assert_eq!(app.preview_asset.palette, old_preview);
        assert_eq!(app.session.as_ref().unwrap().asset.palette, old_active);
        assert!(!app.session_use_ega);
        app.setup_menu.prefs.video_mode = 2;
        app.sync_prefs_from_setup();
        assert!(app.error.is_none());
        assert_eq!(app.preview_asset.id, 2);
        assert_eq!(app.preview_asset.sprites[0].bpp, 4);
        assert_eq!(app.session.as_ref().unwrap().puzzle_num, 1);
        assert_eq!(app.session.as_ref().unwrap().asset.sprites[0].bpp, 4);
        app.selected_puzzle = 1;
        app.reload_preview();
        assert!(app.can_resume_selected());
    }

    #[test]
    fn browser_resume_preserves_board_tray_held_piece_and_time() {
        let mut app = test_browser();
        app.start_selected_puzzle();
        let session = app.session.as_mut().unwrap();
        session.handle_click(20, 40, &mut app.audio);
        let placed = session.held_piece.unwrap();
        let piece = &session.pieces[placed];
        let target = (
            session.playfield_x() + piece.base_x + piece.base_w / 2,
            session.playfield_y() + piece.base_y + piece.base_h / 2,
        );
        session.handle_click(target.0, target.1, &mut app.audio);
        session.handle_click(20, 40, &mut app.audio);
        session.elapsed_secs = 123.0;
        let held = session.held_piece;
        let tray = session.tray_order.clone();
        app.screen = AppScreen::Browser;
        assert!(app.can_resume_selected());
        app.start_selected_puzzle();
        let session = app.session.as_ref().unwrap();
        assert_eq!(app.screen, AppScreen::Playing);
        assert!(session.pieces[placed].placed);
        assert_eq!(session.held_piece, held);
        assert_eq!(session.tray_order, tray);
        assert_eq!(session.elapsed_secs, 123.0);
        app.game_dir = PathBuf::from("/dev/null");
        app.restart_selected_puzzle();
        assert!(app.error.is_some());
        assert!(app.session.as_ref().unwrap().pieces[placed].placed);
        assert_eq!(app.session.as_ref().unwrap().held_piece, held);
        app.game_dir = test_game_dir();
        app.restart_selected_puzzle();
        let session = app.session.as_ref().unwrap();
        assert!(session.pieces.iter().all(|piece| !piece.placed));
        assert!(session.held_piece.is_none());
        assert_eq!(session.tray_order.len(), 9);
        assert_eq!(session.elapsed_secs, 0.0);
    }

    #[test]
    fn browser_only_resumes_matching_unfinished_puzzles() {
        let mut app = test_browser();
        assert!(!app.can_resume_selected());
        app.start_selected_puzzle();
        assert!(app.can_resume_selected());
        app.selected_puzzle = 2;
        assert!(!app.can_resume_selected());
        app.selected_puzzle = 1;
        app.selected_grid_dim = 4;
        assert!(!app.can_resume_selected());
        app.selected_grid_dim = 3;
        app.curved_edges = false;
        assert!(!app.can_resume_selected());
        app.curved_edges = true;
        app.prefs.video_mode = 2;
        assert!(!app.can_resume_selected());
        app.prefs.video_mode = 1;
        app.session.as_mut().unwrap().solved = true;
        assert!(!app.can_resume_selected());
    }

    #[test]
    fn every_puzzle_can_start_in_debug_builds() {
        for num in 1..=15 {
            let asset = PuzzleAsset::load(&test_game_dir(), num, false).unwrap();
            let session = PuzzleSession::new(num, asset, 3, true, true, true);
            assert_eq!(session.pieces.len(), 9);
        }
    }

    #[test]
    fn masked_sprites_preserve_every_palette_index() {
        let pixels: Vec<u8> = (0..=255).chain(std::iter::once(248)).collect();
        let mut opacity = vec![true; 257];
        opacity[256] = false;
        let sprite = assets::Sprite8061 {
            stride: 257,
            height: 1,
            transparent_color: 248,
            bpp: 8,
            sub_x: 0,
            sub_y: 0,
            width: 257,
            sub_h: 1,
            pixels: pixels.clone(),
            opacity: Some(opacity),
        };
        let mut fb = Framebuffer::new();
        fb.clear(9);
        fb.blit_sprite(&sprite, 0, 0, 0);
        fb.blit_thumbnail(&sprite, 0, 1, 257, 1);
        assert_eq!(&fb.pixels[..256], &pixels[..256]);
        assert_eq!(&fb.pixels[320..576], &pixels[..256]);
        assert_eq!(fb.pixels[256], 9);
        assert_eq!(fb.pixels[576], 9);
        fb.blit_sprite(&sprite, 0, 2, (5 << 8) | 0x20);
        assert!(fb.pixels[640..896].iter().all(|&color| color == 5));
        assert_eq!(fb.pixels[896], 9);
    }

    #[test]
    fn cut_pieces_cover_the_picture_exactly_once() {
        for ega in [false, true] {
            for num in 1..=15 {
                let asset = PuzzleAsset::load(&test_game_dir(), num, ega).unwrap();
                for dim in 2..=8 {
                    for curved in [false, true] {
                        let session =
                            PuzzleSession::new(num, asset.clone(), dim, curved, true, true);
                        let mut coverage = vec![0u8; 270 * 196];
                        for piece in &session.pieces {
                            for y in 0..piece.sprite.sub_h as i32 {
                                for x in 0..piece.sprite.width as i32 {
                                    let color = piece.sprite.pixels
                                        [(y * piece.sprite.width as i32 + x) as usize];
                                    if piece
                                        .sprite
                                        .is_opaque((y * piece.sprite.width as i32 + x) as usize)
                                    {
                                        let px = piece.target_x + x;
                                        let py = piece.target_y + y;
                                        assert!((0..270).contains(&px) && (0..196).contains(&py));
                                        coverage[(py * 270 + px) as usize] += 1;
                                        assert_eq!(
                                            color,
                                            session.full_picture_fb.get_pixel(2 + px, 2 + py),
                                            "P{num:02} ega={ega} dim={dim} curved={curved} x={px} y={py}"
                                        );
                                    }
                                }
                            }
                        }
                        assert!(
                            coverage.iter().all(|&n| n == 1),
                            "dim={dim} curved={curved}: missing={} overlapping={}",
                            coverage.iter().filter(|&&n| n == 0).count(),
                            coverage.iter().filter(|&&n| n > 1).count()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn truncated_container_is_rejected() {
        assert!(decompress::split_multi_resource(&[1, 0, 4, 0, 0, 0, 42]).is_err());
    }

    #[test]
    fn truncated_da_is_rejected() {
        let mut raw = vec![0; 20];
        raw[..4].copy_from_slice(b"DA\0\0");
        raw[16] = 4;
        assert!(decompress::decompress_da(&raw).is_err());
        let mut compressed = std::fs::read(test_game_dir().join("P01.PZ8")).unwrap();
        compressed.truncate(compressed.len() / 2);
        assert!(decompress::decompress_da(&compressed).is_err());
    }

    #[test]
    fn truncated_ilbm_is_rejected() {
        let raw = std::fs::read(test_game_dir().join("MJP.TTL")).unwrap();
        let decoded = decompress::decompress_da(&raw).unwrap();
        let chunks = decompress::split_multi_resource(&decoded).unwrap();
        let image = chunks[0];
        assert!(decompress::decode_ilbm(&image[..image.len() - 1]).is_err());
    }

    #[test]
    fn truncated_sprite_is_rejected() {
        let mut rec = vec![0; 26];
        rec[..2].copy_from_slice(&0x8061u16.to_le_bytes());
        for offset in [10, 12, 22, 24] {
            rec[offset] = 1;
        }
        rec[16] = 8;
        assert!(assets::Sprite8061::from_record(&rec).is_err());
    }

    #[test]
    fn test_load_font_and_measure_text() {
        let dir = test_game_dir();
        let font = FontAsset::load(&dir.join("MJP.NFT")).expect("MJP.NFT should load");
        assert_eq!(font.height, 10);
        assert_eq!(font.bitmap.len(), 770);

        let w = font.measure_text("MICKEY'S JIGSAW PUZZLES");
        assert!(w > 80);

        let mut fb = Framebuffer::new();
        fb.draw_text_shadowed(&font, "Hello Mickey!", 10, 10, 0, 15);
    }

    #[test]
    fn test_load_title_and_credits_ilbm() {
        let dir = test_game_dir();
        let ttl = load_ilbm_container(&dir.join("MJP.TTL")).expect("MJP.TTL should load");
        assert!(!ttl.is_empty());
        assert_eq!(ttl[0].width, 320);
        assert_eq!(ttl[0].height, 200);
        assert_eq!(ttl[0].pixels.len(), 64000);

        let error = load_ilbm_container(&dir.join("MJP.CRD")).unwrap_err();
        assert_eq!(error, "Truncated DA bitstream");
    }

    #[test]
    fn test_load_toolbox() {
        let dir = test_game_dir();
        let tbx = ToolboxAsset::load(&dir.join("MJP.TBX"), 0).expect("MJP.TBX should load");
        assert_eq!(tbx.sprites.len(), 65);
        // Border sprites 0x38..=0x3C are used by FUN_1b43_0363
        assert!(tbx.sprites.len() > 0x3C);
        let ega = ToolboxAsset::load(&dir.join("MJP.TBX"), 1).unwrap();
        assert_eq!(ega.sprites.len(), 62);
        assert!(ega.sprites.iter().all(|sprite| sprite.bpp == 4));
        let cga = ToolboxAsset::load(&dir.join("MJP.TBX"), 2).unwrap();
        assert_eq!(cga.sprites.len(), 62);
        assert!(cga.sprites.iter().all(|sprite| sprite.bpp == 2));
        assert_eq!(cga.palette[3], [0, 170, 0]);
        let decoded =
            decompress::decompress_da(&std::fs::read(dir.join("MJP.TBX")).unwrap()).unwrap();
        let chunks = decompress::split_multi_resource(&decoded).unwrap();
        let mut checked = 0;
        for row in include_str!("../validation/toolbox-oracle.tsv")
            .lines()
            .skip(1)
        {
            let fields: Vec<_> = row.split('\t').collect();
            let index: usize = fields[0].parse().unwrap();
            let bytes = chunks[index];
            assert_eq!(bytes.len(), fields[1].parse::<usize>().unwrap());
            let hash = bytes.iter().fold(14695981039346656037u64, |hash, byte| {
                (hash ^ u64::from(*byte)).wrapping_mul(1099511628211)
            });
            assert_eq!(hash, u64::from_str_radix(fields[5], 16).unwrap());
            checked += 1;
        }
        assert_eq!(checked, 3);
        assert!(ToolboxAsset::load(&dir.join("MJP.TBX"), 3).is_err());
    }

    #[test]
    fn vga_palette_preserves_original_rgb_bytes() {
        let dir = test_game_dir();
        for num in 1..=15 {
            let raw = std::fs::read(dir.join(format!("P{num:02}.PZ8"))).unwrap();
            let decoded = decompress::decompress_da(&raw).unwrap();
            let asset = PuzzleAsset::load(&dir, num, false).unwrap();
            let mut pos = 0;
            let mut expected = None;
            while pos + 8 <= decoded.len() {
                let tag = &decoded[pos..pos + 4];
                let size =
                    u32::from_le_bytes(decoded[pos + 4..pos + 8].try_into().unwrap()) as usize;
                if tag == b"PALE" {
                    expected = Some(&decoded[pos + 8..pos + 8 + size]);
                }
                if tag == b"MCG0" {
                    break;
                }
                pos += 8 + size;
            }
            let expected = expected.unwrap();
            for (i, color) in asset.palette.iter().enumerate() {
                assert_eq!(
                    color.as_slice(),
                    &expected[i * 3..i * 3 + 3],
                    "puzzle {num}, color {i}"
                );
            }
        }
    }

    #[test]
    fn ega_palette_has_no_six_byte_header() {
        let dir = test_game_dir();
        for num in 1..=15 {
            let path = dir.join(format!("P{num:02}.PZ4"));
            if !path.exists() {
                continue;
            }
            let raw = std::fs::read(path).unwrap();
            let decoded = decompress::decompress_da(&raw).unwrap();
            assert_eq!(&decoded[..4], b"PALE");
            let parsed = PuzzleAsset::from_decompressed(num, &decoded);
            if num == 6 {
                assert!(parsed.is_err());
                continue;
            }
            let asset = parsed.unwrap();
            for i in 0..16 {
                assert_eq!(
                    asset.palette[i].as_slice(),
                    &decoded[8 + i * 3..11 + i * 3],
                    "puzzle {num}, color {i}"
                );
            }
        }
    }

    #[test]
    fn test_load_all_15_puzzles_vga_and_ega_and_prt() {
        let dir = test_game_dir();
        for num in 1..=15 {
            let pz8 = PuzzleAsset::load(&dir, num, false)
                .unwrap_or_else(|e| panic!("P{:02}.PZ8 failed: {}", num, e));
            assert!(
                !pz8.sprites.is_empty(),
                "P{:02}.PZ8 should have sprites",
                num
            );
            assert!(
                pz8.sequences.len() >= 2,
                "P{:02}.PZ8 should have sequences",
                num
            );

            let pz4 = PuzzleAsset::load(&dir, num, true)
                .unwrap_or_else(|e| panic!("P{:02}.PZ4 failed: {}", num, e));
            assert!(!pz4.sprites.is_empty());
            assert_eq!(pz4.id, num);
            let expected_bpp = if dir.join(format!("P{num:02}.PZ4")).exists() && num != 6 {
                4
            } else {
                8
            };
            assert!(pz4.sprites.iter().all(|sprite| sprite.bpp == expected_bpp));

            let prt = PrintAsset::load(&dir, num)
                .unwrap_or_else(|e| panic!("P{:02}.PRT failed: {}", num, e));
            assert_eq!(prt.bitmap.len(), 8000);
        }
    }

    #[test]
    fn scene_sound_descriptors_preserve_original_references() {
        let dir = test_game_dir();
        let engine = AudioEngine::new(&dir, false, false);
        let first = PuzzleAsset::load(&dir, 1, false).unwrap();
        let mut missing_music = Vec::new();
        assert_eq!(first.sounds[0].sounds.len(), 1);
        assert_eq!(first.sounds[0].sounds[0].stem, "SOUND0");
        assert_eq!(first.sounds[1].sounds[1].stem, "SFX-2");
        for num in 1..=15 {
            let asset = PuzzleAsset::load(&dir, num, false).unwrap();
            for entry in &asset.sounds {
                for desc in &entry.sounds {
                    if desc.stem.is_empty() {
                        continue;
                    }
                    let phrase = desc.phrase_idx as usize;
                    if desc.stem.starts_with("SOUND") {
                        if !engine.iss_songs.contains_key(&(desc.stem.clone(), phrase)) {
                            missing_music.push(format!("P{num:02}:{}:{phrase}", desc.stem));
                        }
                    } else {
                        let bank = engine
                            .nnv_banks
                            .get(&desc.stem)
                            .unwrap_or_else(|| panic!("P{num:02}: missing {}", desc.stem));
                        assert!(
                            !synthesize_nnv_phrase(bank, phrase).is_empty(),
                            "P{num:02}: {}:{phrase}",
                            desc.stem
                        );
                    }
                }
            }
        }
        assert_eq!(missing_music, ["P11:SOUND3:2"]);
    }

    #[test]
    fn test_puzzle_cutting_all_difficulties_curved_and_straight() {
        let dir = test_game_dir();
        let asset = PuzzleAsset::load(&dir, 1, false).expect("P01.PZ8 should load");
        let tbx = ToolboxAsset::load(&dir.join("MJP.TBX"), 0).expect("MJP.TBX should load");
        let font = FontAsset::load(&dir.join("MJP.NFT")).expect("MJP.NFT should load");

        for dim in 2..=8 {
            for &curved in &[false, true] {
                let mut sess = PuzzleSession::new(1, asset.clone(), dim, curved, true, true);
                assert_eq!(sess.pieces.len(), dim * dim);
                assert_eq!(sess.tray_order.len(), dim * dim);

                // Verify every cut piece has non-zero pixels
                for (i, p) in sess.pieces.iter().enumerate() {
                    let non_zero = p.sprite.pixels.iter().filter(|&&px| px != 0).count();
                    assert!(
                        non_zero > 20,
                        "Piece {} in dim={} curved={} has too few opaque pixels ({})",
                        i,
                        dim,
                        curved,
                        non_zero
                    );
                }

                let mut fb = Framebuffer::new();
                sess.render(&mut fb, &tbx, &font, 160, 100);
            }
        }
    }

    #[test]
    fn native_bubblegum_sound_remains_in_p11_scene_sequences() {
        let dir = test_game_dir();
        let audio = AudioEngine::new(&dir, false, false);
        assert_eq!(
            u16::from_le_bytes(audio.nnv_banks["BUBGUM"][..2].try_into().unwrap()),
            1
        );
        for ega in [false, true] {
            let asset = PuzzleAsset::load(&dir, 11, ega).unwrap();
            let index = asset
                .sounds
                .iter()
                .position(|entry| entry.tag == "BUBGUM")
                .unwrap();
            let entry = &asset.sounds[index];
            assert_eq!(entry.sounds[1].stem, "BUBGUM");
            assert_eq!(entry.sounds[1].phrase_idx, 0);
            assert!(entry.bubbles[0].is_none());
            for name in ["F_ALL", "F_TAPE"] {
                let sequence = asset
                    .sequences
                    .iter()
                    .find(|sequence| sequence.name == name)
                    .unwrap();
                assert!(asset.records[sequence.start_frame..sequence.end_frame].iter().any(|record| matches!(record,
                    assets::SceneRecord::Frame { commands, .. } if commands.iter().any(|command|
                        matches!(command, assets::FrameCommand::Sound { snd_idx: Some(sound) } if *sound == index))
                )), "{name} ega={ega}");
            }
            let tape = asset
                .sequences
                .iter()
                .find(|sequence| sequence.name == "F_TAPE")
                .unwrap();
            assert!(asset.records[tape.start_frame..tape.end_frame].iter().any(
                |record| matches!(record,
                    assets::SceneRecord::Frame { commands, .. } if commands.iter().any(|command|
                        matches!(command, assets::FrameCommand::Delay { ticks: 998, .. }))
                )
            ));
        }
    }

    #[test]
    fn test_audio_decoders_nnv_ess_and_iss() {
        let dir = test_game_dir();
        let engine = AudioEngine::new(&dir, false, false);

        for key in ["BUBGUM", "PROG_0", "SFX-2", "P01", "P15"] {
            let bank = engine
                .nnv_banks
                .get(key)
                .unwrap_or_else(|| panic!("Missing NNV bank {}", key));
            let samples = synthesize_nnv_phrase(bank, 0);
            assert!(
                !samples.is_empty(),
                "NNV bank {} phrase 0 should produce samples",
                key
            );
        }

        for stem in ["TITLE", "SOUND0", "SOUND1", "SOUND2", "SOUND3"] {
            let song = engine
                .iss_songs
                .get(&(stem.to_string(), 0))
                .unwrap_or_else(|| panic!("Missing ISS song {}:0", stem));
            let samples = synthesize_iss_song(song);
            assert!(
                !samples.is_empty(),
                "ISS song {}:0 should produce samples",
                stem
            );
        }
    }

    #[test]
    fn iss_music_and_standalone_speech_banks_load() {
        let engine = AudioEngine::new(&test_game_dir(), false, false);
        for stem in ["TITLE", "P01", "P15", "SP-231", "SFX-19"] {
            assert!(engine.nnv_banks.contains_key(stem), "{stem}");
        }
        for stem in ["TITLE", "SOUND0", "SOUND1", "SOUND2", "SOUND3"] {
            let song = engine.iss_songs.get(&(stem.to_string(), 0)).unwrap();
            assert!(!synthesize_iss_song(song).is_empty());
        }
    }

    #[test]
    fn click_to_pick_place_complete_and_skip_animation() {
        let dir = test_game_dir();
        let mut audio = AudioEngine::new(&dir, false, false);
        let font = FontAsset::load(&dir.join("MJP.NFT")).unwrap();
        let mut completed = 0;
        let mut placed = 0;
        for ega in [false, true] {
            for num in 1..=15 {
                let asset = PuzzleAsset::load(&dir, num, ega).unwrap();
                let toolbox =
                    ToolboxAsset::load(&dir.join("MJP.TBX"), usize::from(asset.uses_ega()))
                        .unwrap();
                for dim in 2..=8 {
                    for curved in [false, true] {
                        let mut session =
                            PuzzleSession::new(num, asset.clone(), dim, curved, true, true);
                        for step in 0..dim * dim {
                            session.handle_pointer_press(20, 40, &mut audio);
                            let index = session.held_piece.expect("tray pick");
                            let piece = &session.pieces[index];
                            let x = session.playfield_x() + piece.base_x + piece.base_w / 2;
                            let y = session.playfield_y() + piece.base_y + piece.base_h / 2;
                            if step % 2 == 0 {
                                session.handle_pointer_release(21, 41, &mut audio);
                                assert_eq!(session.held_piece, Some(index));
                                session.handle_pointer_press(x, y, &mut audio);
                            }
                            session.handle_pointer_release(x, y, &mut audio);
                            assert!(
                                session.pieces[index].placed,
                                "P{num:02} ega={ega} grid={dim} curved={curved} piece={index}"
                            );
                            assert!(session.held_piece.is_none());
                            placed += 1;
                        }
                        assert!(session.solved);
                        assert!(
                            audio.common_caption().is_none(),
                            "stale pickup caption after P{num:02}"
                        );
                        assert!(session.tray_order.is_empty());
                        assert!(session.pieces.iter().all(|piece| piece.placed));
                        let mut fb = Framebuffer::new();
                        session.render(&mut fb, &toolbox, &font, 160, 100);
                        assert!(session.active_anim.is_some());
                        session.handle_click(160, 100, &mut audio);
                        assert!(session.active_anim.is_none());
                        session.render(&mut fb, &toolbox, &font, 160, 100);
                        assert_eq!(
                            session.handle_click(2, 2, &mut audio),
                            Some(PuzzleAction::OpenMenu)
                        );
                        completed += 1;
                    }
                }
            }
        }
        assert_eq!(completed, 420);
        assert_eq!(placed, 12180);
        println!(
            "{completed} complete games; {placed} pieces placed through alternating click-carry and drag-release input"
        );
    }

    #[test]
    fn animation_preserves_elapsed_remainder() {
        let mut asset = PuzzleAsset::load(&test_game_dir(), 1, false).unwrap();
        asset.records = vec![
            assets::SceneRecord::Frame {
                bg_frame_idx: 0,
                bg_color: 0,
                commands: Vec::new(),
            };
            3
        ];
        let mut session = PuzzleSession::new(1, asset, 3, true, true, true);
        let mut audio = AudioEngine::new(&test_game_dir(), false, false);
        session.active_anim = Some(puzzle::AnimationPlayer {
            seq_idx: 0,
            start_frame: 0,
            previous_frame: 0,
            cur_frame: 0,
            end_frame: 3,
            tick_accum: 0.0,
            frame_delay_secs: 2.0 * 65536.0 / 1193182.0,
            repetitions_left: 1,
            default_delay_secs: 2.0 * 65536.0 / 1193182.0,
            transition_mode: 0,
            bubble: None,
            wait_for_audio: false,
            looping_sound: None,
        });
        let frame_secs = session.active_anim.as_ref().unwrap().frame_delay_secs;
        session.update(2.5 * frame_secs, &mut audio);
        let anim = session.active_anim.as_ref().unwrap();
        assert_eq!(anim.cur_frame, 2);
        assert!((anim.tick_accum - 0.5 * frame_secs).abs() < 0.00001);
        let anim = session.active_anim.as_mut().unwrap();
        anim.cur_frame = 0;
        anim.tick_accum = 0.0;
        anim.repetitions_left = 2;
        session.update(4.5 * frame_secs, &mut audio);
        let anim = session.active_anim.as_ref().unwrap();
        assert_eq!(anim.cur_frame, 1);
        assert_eq!(anim.repetitions_left, 1);
        assert!((anim.tick_accum - 0.5 * frame_secs).abs() < 0.00001);
    }

    #[test]
    fn polygon_clips_full_signed_asset_coordinates() {
        let mut fb = Framebuffer::new();
        fb.fill_polygon(&[(-32768, -32768), (32767, 32767), (-32768, 32767)], 15);
        assert_eq!(fb.pixels[199 * 320 + 199], 15);
        assert_eq!(fb.pixels[199 * 320 + 200], 0);
    }

    #[test]
    fn setup_reports_save_failure_and_ignores_outside_clicks() {
        let mut setup = SetupMenu::new(Preferences::default());
        let original_text = setup.prefs.text_msg_on;
        setup.selected_row = 5;
        assert!(!setup.activate_or_cycle(1, Path::new("/dev/null")));
        assert_eq!(setup.prefs.text_msg_on, original_text ^ 1);
        setup.selected_row = setup.num_rows() - 1;
        assert!(!setup.activate_or_cycle(1, Path::new("/dev/null")));
        assert!(setup.error.is_some());
        let before = setup.prefs;
        setup.handle_click(-1, 32, Path::new("/dev/null"));
        assert_eq!(setup.prefs, before);
    }

    #[test]
    fn thumbnails_fit_inside_their_tray_slot() {
        let mut fb = Framebuffer::new();
        let sprite = assets::Sprite8061 {
            stride: 135,
            height: 98,
            transparent_color: 248,
            opacity: None,
            bpp: 8,
            sub_x: 0,
            sub_y: 0,
            width: 135,
            sub_h: 98,
            pixels: vec![7; 135 * 98],
        };
        fb.blit_thumbnail(&sprite, 3, 32, 40, 48);
        let mut count = 0;
        for y in 0..200 {
            for x in 0..320 {
                if fb.get_pixel(x, y) == 7 {
                    assert!((3..43).contains(&x) && (32..80).contains(&y));
                    count += 1;
                }
            }
        }
        assert!(count > 1000);
    }

    #[test]
    fn toolbox_graphics_match_actual_scene_depth_including_fallbacks() {
        let mut app = test_browser();
        app.setup_menu.prefs.video_mode = 2;
        app.sync_prefs_from_setup();
        for id in [1, 6, 7, 10] {
            app.selected_puzzle = id;
            app.restart_selected_puzzle();
            app.audio.update(60.0);
            let toolbox = ToolboxAsset::load(
                &app.game_dir.join("MJP.TBX"),
                usize::from(app.session.as_ref().unwrap().asset.sprites[0].bpp == 4),
            )
            .unwrap();
            let mut expected = Framebuffer::new();
            app.session
                .as_mut()
                .unwrap()
                .render(&mut expected, &toolbox, &app.font, 160, 100);
            let mut actual = Framebuffer::new();
            app.render(&mut actual, 160, 100);
            assert!(
                actual.pixels == expected.pixels,
                "P{id:02}, toolbox variant {}",
                usize::from(app.session.as_ref().unwrap().asset.sprites[0].bpp == 4)
            );
        }
    }

    #[test]
    fn browser_and_puzzle_controls_use_palette_colors() {
        let dir = test_game_dir();
        let font = FontAsset::load(&dir.join("MJP.NFT")).unwrap();
        for ega in [false, true] {
            let asset = PuzzleAsset::load(&dir, 1, ega).unwrap();
            let toolbox = ToolboxAsset::load(&dir.join("MJP.TBX"), usize::from(ega)).unwrap();
            let mut session = PuzzleSession::new(1, asset, 3, true, true, true);
            let mut fb = Framebuffer::new();
            session.render(&mut fb, &toolbox, &font, 160, 100);
            assert_eq!(fb.get_pixel(1, 100), fb.color_index([0; 3]), "ega={ega}");
            assert_eq!(fb.get_pixel(3, 19), fb.color_index([85; 3]), "ega={ega}");
            assert_ne!(fb.get_pixel(1, 100), fb.get_pixel(3, 19), "ega={ega}");
            let mut app = test_browser();
            app.setup_menu.prefs.video_mode = if ega { 2 } else { 1 };
            app.sync_prefs_from_setup();
            app.render(&mut fb, 160, 100);
            assert_eq!(fb.get_pixel(7, 5), fb.color_index([0; 3]), "ega={ega}");
            assert_eq!(
                fb.get_pixel(7, 173),
                fb.color_index([0, 0, 170]),
                "ega={ega}"
            );
        }
    }

    #[test]
    fn ui_white_uses_the_loaded_palette() {
        let asset = PuzzleAsset::load(&test_game_dir(), 1, false).unwrap();
        let mut fb = Framebuffer::new();
        fb.palette = asset.palette;
        let white = fb.color_index([255; 3]);
        assert_eq!(fb.palette[white as usize], [255; 3]);
        assert_ne!(white, 15);
    }

    #[test]
    fn printable_export_preserves_bitmap_and_reports_write_failure() {
        let temp = env::temp_dir().join(format!("mickey-print-test-{}", std::process::id()));
        std::fs::create_dir(&temp).unwrap();
        let source = test_game_dir();
        std::fs::copy(source.join("P01.PRT"), temp.join("P01.PRT")).unwrap();
        let asset = PuzzleAsset::load(&source, 1, false).unwrap();
        let mut session = PuzzleSession::new(1, asset, 3, true, true, true);
        session.export_printable_picture(&temp);
        let image = std::fs::read(temp.join("prints/P01.pbm")).unwrap();
        assert_eq!(&image[..11], b"P4\n320 200\n");
        assert_eq!(&image[11..], &PrintAsset::load(&source, 1).unwrap().bitmap);
        std::fs::remove_dir_all(temp.join("prints")).unwrap();
        std::fs::write(temp.join("prints"), b"blocked").unwrap();
        session.export_printable_picture(&temp);
        assert!(
            session
                .status_message
                .as_ref()
                .unwrap()
                .0
                .starts_with("Print error:")
        );
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn browser_navigation_wraps_and_buttons_align() {
        let mut app = GameApp::new(test_game_dir(), true).unwrap();
        app.screen = AppScreen::Browser;
        app.selected_puzzle = 1;
        app.step_puzzle(-1);
        assert_eq!(app.selected_puzzle, 15);
        app.step_puzzle(1);
        assert_eq!(app.selected_puzzle, 1);

        app.selected_grid_dim = 1;
        app.step_grid_dim(-1);
        assert_eq!(app.selected_grid_dim, 8);
        app.step_grid_dim(1);
        assert_eq!(app.selected_grid_dim, 1);

        app.selected_grid_dim = 3;
        let edges_was = app.curved_edges;
        let (_, ex, ew, _) = BROWSER_BUTTONS[3];
        app.handle_browser_click(ex + ew / 2, 180);
        assert_ne!(app.curved_edges, edges_was);

        let (_, sx, sw, _) = BROWSER_BUTTONS[4];
        app.handle_browser_click(sx + sw / 2, 180);
        assert_eq!(
            app.screen,
            AppScreen::Setup {
                return_to_playing: false
            }
        );
    }

    #[test]
    fn open_setup_clears_previous_error() {
        let mut app = test_browser();
        app.setup_menu.error = Some("stale error".to_string());
        app.open_setup(false);
        assert!(app.setup_menu.error.is_none());
        assert_eq!(
            app.screen,
            AppScreen::Setup {
                return_to_playing: false
            }
        );
    }

    #[test]
    #[ignore]
    fn perf_benchmark() {
        use std::hint::black_box;
        use std::time::Instant;

        let dir = test_game_dir();
        let t0 = Instant::now();
        for id in 1..=15 {
            black_box(PuzzleAsset::load(&dir, id, false).unwrap());
        }
        let load_all = t0.elapsed();

        let asset = PuzzleAsset::load(&dir, 1, false).unwrap();
        let t1 = Instant::now();
        let mut session = PuzzleSession::new(1, asset, 8, true, true, true);
        let cut_8x8 = t1.elapsed();

        let tbx = ToolboxAsset::load(&dir.join("MJP.TBX"), 0).unwrap();
        let font = FontAsset::load(&dir.join("MJP.NFT")).unwrap();
        let mut fb = Framebuffer::new();
        let mut rgba = vec![0u8; 320 * 200 * 4];

        // Warm up caches
        session.render(&mut fb, &tbx, &font, 160, 100);
        fb.to_rgba8(&mut rgba);
        session.blend_picture_guide(&mut rgba, 160, 100);

        let iters = 2000;
        let t2 = Instant::now();
        for _ in 0..iters {
            session.render(&mut fb, &tbx, &font, 160, 100);
            black_box(&fb.pixels[0]);
        }
        let render_warm = t2.elapsed();

        let t3 = Instant::now();
        for _ in 0..iters {
            fb.to_rgba8(&mut rgba);
            black_box(rgba[0]);
        }
        let to_rgba = t3.elapsed();

        let t4 = Instant::now();
        for _ in 0..iters {
            session.blend_picture_guide(&mut rgba, 160, 100);
            black_box(rgba[0]);
        }
        let guide = t4.elapsed();

        eprintln!(
            "load_15_vga={:.2}ms cut_8x8={:.2}ms render_warm={:.2}us/frame to_rgba8={:.2}us/frame guide={:.2}us/frame",
            load_all.as_secs_f64() * 1000.0,
            cut_8x8.as_secs_f64() * 1000.0,
            render_warm.as_secs_f64() * 1e6 / iters as f64,
            to_rgba.as_secs_f64() * 1e6 / iters as f64,
            guide.as_secs_f64() * 1e6 / iters as f64,
        );
    }
}
