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
use crate::puzzle::{PuzzleAction, PuzzlePresentKey, PuzzleSession, hash_str_opt};
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PresentSignature {
    Title,
    Credits(usize),
    Setup {
        selected_row: usize,
        video_mode: u16,
        music_on: u16,
        sfx_on: u16,
        timer_on: u16,
        grid_lines_on: u16,
        text_msg_on: u16,
        has_error: bool,
    },
    Browser {
        selected_puzzle: usize,
        selected_grid_dim: usize,
        curved_edges: bool,
        uses_ega: bool,
        can_resume: bool,
        caption_hash: Option<u64>,
        has_error: bool,
    },
    Reference {
        puzzle_num: usize,
        uses_ega: bool,
    },
    Playing {
        puzzle: PuzzlePresentKey,
        keyboard_pointer: Option<(i32, i32)>,
        caption_hash: Option<u64>,
        has_error: bool,
    },
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

    fn present_signature(
        &self,
        mx: i32,
        my: i32,
        show_reference: bool,
        picture_guide: bool,
    ) -> Option<PresentSignature> {
        Some(match self.screen {
            AppScreen::Title => PresentSignature::Title,
            AppScreen::Credits { page } => PresentSignature::Credits(page),
            AppScreen::Setup { .. } => PresentSignature::Setup {
                selected_row: self.setup_menu.selected_row,
                video_mode: self.setup_menu.prefs.video_mode,
                music_on: self.setup_menu.prefs.music_on,
                sfx_on: self.setup_menu.prefs.sfx_on,
                timer_on: self.setup_menu.prefs.timer_on,
                grid_lines_on: self.setup_menu.prefs.grid_lines_on,
                text_msg_on: self.setup_menu.prefs.text_msg_on,
                has_error: self.setup_menu.error.is_some(),
            },
            AppScreen::Browser => {
                let caption_hash = hash_str_opt(
                    (self.prefs.text_msg_on != 0)
                        .then(|| self.audio.common_caption())
                        .flatten(),
                );
                PresentSignature::Browser {
                    selected_puzzle: self.selected_puzzle,
                    selected_grid_dim: self.selected_grid_dim,
                    curved_edges: self.curved_edges,
                    uses_ega: self.preview_asset.uses_ega(),
                    can_resume: self.can_resume_selected(),
                    caption_hash,
                    has_error: self.error.is_some(),
                }
            }
            AppScreen::Playing => {
                let session = self.session.as_ref()?;
                if show_reference {
                    PresentSignature::Reference {
                        puzzle_num: session.puzzle_num,
                        uses_ega: session.asset.uses_ega(),
                    }
                } else {
                    let caption_hash = hash_str_opt(
                        (self.prefs.text_msg_on != 0)
                            .then(|| self.audio.common_caption())
                            .flatten(),
                    );
                    PresentSignature::Playing {
                        puzzle: session.present_key(mx, my, picture_guide),
                        keyboard_pointer: session
                            .keyboard_pointer
                            .map(|(x, y)| (x.round() as i32, y.round() as i32)),
                        caption_hash,
                        has_error: self.error.is_some(),
                    }
                }
            }
        })
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

struct DesktopHudCtx<'a> {
    win_w: f32,
    win_h: f32,
    reference: &'a Texture2D,
    cached_ref_key: &'a mut Option<(usize, bool)>,
    rgba_buf: &'a mut [u8],
    reference_button: Rect,
    reference_hovered: bool,
    reference_available: bool,
    show_reference: bool,
}

fn render_desktop_hud(session: &PuzzleSession, ctx: DesktopHudCtx<'_>) {
    let DesktopHudCtx {
        win_w,
        win_h,
        reference,
        cached_ref_key,
        rgba_buf,
        reference_button,
        reference_hovered,
        reference_available,
        show_reference,
    } = ctx;
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
    let mut last_present_sig: Option<PresentSignature> = None;
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
        let sig = app.present_signature(mx, my, show_reference, picture_guide);
        if sig.is_none() || sig != last_present_sig {
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
            last_present_sig = sig;
        }

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
                    DesktopHudCtx {
                        win_w,
                        win_h,
                        reference: &reference,
                        cached_ref_key: &mut cached_ref_key,
                        rgba_buf: &mut rgba_buf,
                        reference_button,
                        reference_hovered,
                        reference_available,
                        show_reference,
                    },
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
#[path = "main_tests.rs"]
mod tests;
