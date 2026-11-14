//! Application state machine, input handling, and software presentation layer.
//!
//! Kept independent of `macroquad` so all screen transitions, input handling,
//! cache invalidation, and framebuffer rendering can be tested headlessly.

use std::path::PathBuf;

use crate::assets::{FontAsset, Preferences, PuzzleAsset, ToolboxAsset, load_ilbm_container};
use crate::audio::{AudioEngine, CommonSound};
use crate::decompress::IlbmImage;
use crate::graphics::{Framebuffer, SCREEN_H, SCREEN_W, render_scene_view_with_scratch};
use crate::puzzle::{PuzzleAction, PuzzlePresentKey, PuzzleSession};
use crate::puzzle_render::PuzzleRenderer;
use crate::setup::SetupMenu;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserAction {
    PrevPuzzle,
    NextPuzzle,
    CyclePieces,
    ToggleEdges,
    OpenSetup,
    StartOrResume,
}

pub const BROWSER_BUTTONS: [(BrowserAction, i32, i32, [u8; 3]); 6] = [
    (BrowserAction::PrevPuzzle, 6, 38, [0, 0, 170]),
    (BrowserAction::NextPuzzle, 48, 38, [0, 0, 170]),
    (BrowserAction::CyclePieces, 90, 68, [170, 0, 0]),
    (BrowserAction::ToggleEdges, 162, 60, [170, 0, 170]),
    (BrowserAction::OpenSetup, 226, 40, [85, 85, 85]),
    (BrowserAction::StartOrResume, 268, 48, [0, 170, 0]),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppScreen {
    Title,
    Credits { page: usize },
    Setup { return_to_playing: bool },
    Browser,
    Playing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentSignature {
    Title,
    Credits(usize),
    Setup {
        app_rev: u64,
        setup_rev: u64,
    },
    Browser {
        app_rev: u64,
        caption_key: Option<u64>,
    },
    Reference {
        session_id: u64,
        asset_rev: u64,
    },
    Playing {
        app_rev: u64,
        puzzle: PuzzlePresentKey,
        caption_key: Option<u64>,
    },
}

/// Platform-agnostic per-frame input snapshot consumed by [`GameApp::handle_input`].
#[derive(Clone, Copy, Debug, Default)]
pub struct AppInput {
    pub dt: f32,
    pub pointer_x: i32,
    pub pointer_y: i32,
    pub left_pressed: bool,
    pub left_released: bool,
    pub right_pressed: bool,
    pub wheel_delta: i32,
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub enter: bool,
    pub space: bool,
    pub escape: bool,
    pub tab: bool,
    pub key_e: bool,
    pub key_r: bool,
    pub key_s: bool,
    pub key_g: bool,
    pub key_p: bool,
    pub key_t: bool,
    pub key_m: bool,
    pub f1: bool,
    pub f2: bool,
    pub f3: bool,
    pub f4: bool,
    pub f5: bool,
    pub f7: bool,
    pub f8: bool,
    pub move_x: i32,
    pub move_y: i32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewportRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

pub fn game_viewport(width: f32, height: f32, playing: bool) -> ViewportRect {
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
    ViewportRect {
        x: (width - w) * 0.5,
        y: top + (height - top - bottom - h) * 0.5,
        w,
        h,
    }
}

pub struct BrowserPreviewCache {
    puzzle_id: usize,
    uses_ega: bool,
    fb: Box<Framebuffer>,
    white: u8,
    black: u8,
    button_bgs: [u8; BROWSER_BUTTONS.len()],
}

pub struct GameApp {
    pub game_dir: PathBuf,
    pub font: FontAsset,
    pub tbx: ToolboxAsset,
    pub tbx_ega: ToolboxAsset,
    pub title_images: Vec<IlbmImage>,
    pub credit_images: Vec<IlbmImage>,
    pub prefs: Preferences,
    pub setup_menu: SetupMenu,
    pub audio: AudioEngine,
    pub screen: AppScreen,
    screen_timer: f32,

    // Browser state
    pub selected_puzzle: usize,
    pub preview_asset: PuzzleAsset,
    pub selected_grid_dim: usize,
    pub curved_edges: bool,

    // Active puzzle state
    pub session: Option<PuzzleSession>,
    pub session_use_ega: bool,
    pub confirm_exit_puzzle: bool,
    pub error: Option<String>,

    app_rev: u64,
}

impl GameApp {
    pub fn new(game_dir: PathBuf, start_in_setup: bool) -> Result<Self, String> {
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
            selected_grid_dim: 3, // Default 3x3 = 9 pieces
            curved_edges: true,
            session: None,
            session_use_ega: use_ega,
            confirm_exit_puzzle: false,
            error: None,
            app_rev: 1,
        })
    }

    #[cfg(test)]
    pub(crate) fn from_parts(
        game_dir: PathBuf,
        font: FontAsset,
        tbx: ToolboxAsset,
        tbx_ega: ToolboxAsset,
        preview_asset: PuzzleAsset,
        prefs: Preferences,
        start_in_setup: bool,
    ) -> Self {
        let use_ega = prefs.video_mode >= 2;
        let audio = AudioEngine::new(&game_dir, false, false);
        let setup_menu = SetupMenu::new(prefs);
        let screen = if start_in_setup {
            AppScreen::Setup {
                return_to_playing: false,
            }
        } else {
            AppScreen::Browser
        };
        Self {
            game_dir,
            font,
            tbx,
            tbx_ega,
            title_images: Vec::new(),
            credit_images: Vec::new(),
            prefs,
            setup_menu,
            audio,
            screen,
            screen_timer: 0.0,
            selected_puzzle: preview_asset.id,
            preview_asset,
            selected_grid_dim: 3,
            curved_edges: true,
            session: None,
            session_use_ega: use_ega,
            confirm_exit_puzzle: false,
            error: None,
            app_rev: 1,
        }
    }

    #[inline]
    fn bump_rev(&mut self) {
        self.app_rev = self.app_rev.wrapping_add(1);
    }

    pub fn set_screen(&mut self, screen: AppScreen) {
        if self.screen != screen {
            self.screen = screen;
            self.screen_timer = 0.0;
            self.bump_rev();
        }
    }

    fn sync_session_preferences(&mut self) {
        if let Some(sess) = &mut self.session {
            sess.set_preferences(
                self.prefs.grid_lines_on != 0,
                self.prefs.timer_on != 0,
                self.prefs.text_msg_on != 0,
            );
        }
    }

    fn toggle_curved_edges(&mut self) {
        if self.selected_grid_dim != 1 {
            self.curved_edges = !self.curved_edges;
            self.bump_rev();
        }
    }

    fn do_return_to_browser(&mut self) {
        if let Some(session) = &mut self.session {
            session.cancel_pointer_drag();
        }
        self.confirm_exit_puzzle = false;
        self.audio.stop_all();
        self.audio.play_common(CommonSound::OtherPicture);
        self.set_screen(AppScreen::Browser);
    }

    fn return_to_browser(&mut self) {
        let unfinished = self
            .session
            .as_ref()
            .is_some_and(|s| !s.solved && s.has_unfinished_progress());
        if unfinished && !self.confirm_exit_puzzle {
            self.confirm_exit_puzzle = true;
            self.audio.play_common(CommonSound::NotDoneYet);
            if let Some(sess) = &mut self.session {
                sess.cancel_pointer_drag();
                sess.status_message = Some((
                    "We're not done yet. Exit? (Enter/Click=Yes, Esc=No)".to_string(),
                    8.0,
                ));
            }
            self.bump_rev();
            return;
        }
        self.do_return_to_browser();
    }

    pub fn reload_preview(&mut self) {
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
        self.bump_rev();
    }

    pub fn sync_prefs_from_setup(&mut self) {
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
                    self.setup_menu.set_video_mode(previous_video);
                    self.error = Some(error);
                }
            }
        }
        self.prefs = self.setup_menu.prefs;
        self.audio
            .set_enabled(self.prefs.music_on != 0, self.prefs.sfx_on != 0);
        self.sync_session_preferences();
        self.bump_rev();
    }

    pub fn can_resume_selected(&self) -> bool {
        self.session.as_ref().is_some_and(|session| {
            !session.solved
                && session.puzzle_num == self.selected_puzzle
                && session.grid_dim == self.selected_grid_dim
                && (session.grid_dim == 1 || session.curved_edges == self.curved_edges)
                && self.session_use_ega == (self.prefs.video_mode >= 2)
        })
    }

    pub fn start_selected_puzzle(&mut self) {
        if self.can_resume_selected() {
            self.audio.stop_all();
            self.error = None;
            self.set_screen(AppScreen::Playing);
        } else {
            self.restart_selected_puzzle();
        }
    }

    pub fn restart_selected_puzzle(&mut self) {
        let use_ega = self.prefs.video_mode >= 2;
        match PuzzleAsset::load(&self.game_dir, self.selected_puzzle, use_ega) {
            Ok(asset) => {
                let sess = PuzzleSession::new(
                    self.selected_puzzle,
                    asset,
                    self.selected_grid_dim,
                    self.curved_edges,
                    self.prefs.grid_lines_on != 0,
                    self.prefs.timer_on != 0,
                );
                self.session = Some(sess);
                self.sync_session_preferences();
                self.session_use_ega = use_ega;
                self.audio.play_common(CommonSound::StartPuzzle);
                self.error = None;
                self.set_screen(AppScreen::Playing);
            }
            Err(err) => {
                eprintln!("{err}");
                self.error = Some(err);
                self.bump_rev();
            }
        }
    }

    pub fn step_puzzle(&mut self, delta: i32) {
        self.selected_puzzle = if delta < 0 {
            if self.selected_puzzle > 1 {
                self.selected_puzzle - 1
            } else {
                15
            }
        } else {
            (self.selected_puzzle % 15) + 1
        };
        self.reload_preview();
        self.audio.play_common(if delta < 0 {
            CommonSound::PreviousPicture
        } else {
            CommonSound::NextPicture
        });
    }

    pub fn step_grid_dim(&mut self, delta: i32) {
        self.selected_grid_dim = if delta > 0 {
            (self.selected_grid_dim % 8) + 1
        } else if self.selected_grid_dim > 1 {
            self.selected_grid_dim - 1
        } else {
            8
        };
        self.audio.play_common(if self.can_resume_selected() {
            CommonSound::ChangePieceSize
        } else {
            CommonSound::ChooseSize
        });
        self.bump_rev();
    }

    pub fn open_setup(&mut self, return_to_playing: bool) {
        self.setup_menu.set_prefs(self.prefs);
        self.setup_menu.clear_error();
        self.set_screen(AppScreen::Setup { return_to_playing });
    }

    pub fn exit_setup(&mut self, return_to_playing: bool) {
        self.sync_prefs_from_setup();
        self.audio.play_common(CommonSound::SetupStart);
        let next = if return_to_playing && self.session.is_some() {
            AppScreen::Playing
        } else {
            AppScreen::Browser
        };
        self.set_screen(next);
    }

    pub fn handle_input(&mut self, input: &AppInput) {
        let dt = if input.dt.is_finite() {
            input.dt.max(0.0)
        } else {
            0.0
        };
        if !matches!(self.screen, AppScreen::Setup { .. }) {
            self.audio.update(dt);
        }
        self.process_input(input, dt);
        self.audio
            .set_paused(matches!(self.screen, AppScreen::Setup { .. }));
    }

    fn process_input(&mut self, input: &AppInput, dt: f32) {
        // Global shortcut F4 opens Setup from anywhere
        if input.f4 && !matches!(self.screen, AppScreen::Setup { .. }) {
            if let Some(session) = &mut self.session {
                session.cancel_pointer_drag();
            }
            let in_play = self.screen == AppScreen::Playing;
            self.open_setup(in_play);
            return;
        }

        match self.screen {
            AppScreen::Title => {
                self.screen_timer += dt;
                if input.escape {
                    self.set_screen(AppScreen::Browser);
                } else if input.left_pressed
                    || input.space
                    || input.enter
                    || self.screen_timer > 8.0
                {
                    let next = if self.credit_images.is_empty() {
                        AppScreen::Browser
                    } else {
                        AppScreen::Credits { page: 0 }
                    };
                    self.set_screen(next);
                }
            }
            AppScreen::Credits { page } => {
                self.screen_timer += dt;
                if input.escape {
                    self.set_screen(AppScreen::Browser);
                } else if input.left_pressed
                    || input.space
                    || input.enter
                    || self.screen_timer > 5.0
                {
                    let next = if page + 1 < self.credit_images.len() {
                        AppScreen::Credits { page: page + 1 }
                    } else {
                        AppScreen::Browser
                    };
                    self.set_screen(next);
                }
            }
            AppScreen::Setup { return_to_playing } => {
                if input.up {
                    self.setup_menu.move_up();
                } else if input.down {
                    self.setup_menu.move_down();
                } else if input.escape {
                    self.exit_setup(return_to_playing);
                } else {
                    let done = if input.left {
                        if self.setup_menu.selected_row + 1 < self.setup_menu.num_rows() {
                            self.setup_menu.activate_or_cycle(&self.game_dir);
                        }
                        false
                    } else if input.right || input.enter || input.space {
                        self.setup_menu.activate_or_cycle(&self.game_dir)
                    } else if input.left_pressed {
                        self.setup_menu.handle_click(
                            input.pointer_x,
                            input.pointer_y,
                            &self.game_dir,
                        )
                    } else {
                        return;
                    };
                    if done {
                        self.exit_setup(return_to_playing);
                    } else {
                        self.sync_prefs_from_setup();
                    }
                }
            }
            AppScreen::Browser => {
                if input.left {
                    self.step_puzzle(-1);
                } else if input.right {
                    self.step_puzzle(1);
                } else if input.up {
                    self.step_grid_dim(1);
                } else if input.down {
                    self.step_grid_dim(-1);
                } else if input.tab || input.key_e {
                    self.toggle_curved_edges();
                } else if input.enter || input.space {
                    self.start_selected_puzzle();
                } else if input.key_r {
                    self.restart_selected_puzzle();
                } else if input.key_s {
                    self.open_setup(false);
                } else if input.left_pressed {
                    self.handle_browser_click(input.pointer_x, input.pointer_y);
                }
            }
            AppScreen::Playing => {
                if self.confirm_exit_puzzle {
                    if input.enter || input.left_pressed {
                        self.confirm_exit_puzzle = false;
                        self.do_return_to_browser();
                    } else if input.escape {
                        self.confirm_exit_puzzle = false;
                        self.audio.play_common(CommonSound::Cancel);
                        if let Some(sess) = &mut self.session {
                            sess.status_message = None;
                        }
                        self.bump_rev();
                    }
                    return;
                }
                if input.escape {
                    self.return_to_browser();
                    return;
                }
                let mut open_menu = false;

                if let Some(ref mut sess) = self.session {
                    let was_animating = sess.active_anim.is_some();
                    sess.update(dt, &mut self.audio);

                    if input.right_pressed {
                        sess.clear_keyboard_pointer();
                    }
                    if !input.left_pressed && !input.right_pressed {
                        if input.move_x != 0 || input.move_y != 0 {
                            sess.move_keyboard_pointer(
                                input.move_x as f32,
                                input.move_y as f32,
                                dt,
                            );
                        }
                        if input.enter
                            && !(was_animating && sess.active_anim.is_none())
                            && sess.handle_keyboard_activate(&mut self.audio)
                                == Some(PuzzleAction::OpenMenu)
                        {
                            open_menu = true;
                        }
                    }

                    if input.right_pressed && !sess.solved {
                        if !sess.handle_right_click(
                            input.pointer_x,
                            input.pointer_y,
                            &mut self.audio,
                        ) {
                            sess.toggle_toolbox();
                        }
                    } else if input.space && !sess.solved {
                        sess.toggle_toolbox();
                    }
                    if input.f1 && sess.trigger_hint() {
                        self.audio.play_common(CommonSound::SeeWhatHappens);
                    }
                    if input.f2 {
                        sess.auto_solve(&mut self.audio);
                    }
                    if input.f3 || input.key_g {
                        self.prefs.grid_lines_on = u16::from(sess.toggle_grid_lines());
                        self.app_rev = self.app_rev.wrapping_add(1);
                    }
                    if input.f5 || input.key_p {
                        sess.export_printable_picture(&self.game_dir);
                    }
                    if input.f7 {
                        self.prefs.music_on = u16::from(self.prefs.music_on == 0);
                        self.audio
                            .set_enabled(self.prefs.music_on != 0, self.prefs.sfx_on != 0);
                        self.audio.play_common(CommonSound::UiClick);
                        sess.status_message = Some((
                            if self.prefs.music_on != 0 {
                                "Music: ON".to_string()
                            } else {
                                "Music: OFF".to_string()
                            },
                            2.0,
                        ));
                        self.app_rev = self.app_rev.wrapping_add(1);
                    }
                    if input.f8 {
                        self.prefs.sfx_on = u16::from(self.prefs.sfx_on == 0);
                        self.audio
                            .set_enabled(self.prefs.music_on != 0, self.prefs.sfx_on != 0);
                        self.audio.play_common(CommonSound::UiClick);
                        sess.status_message = Some((
                            if self.prefs.sfx_on != 0 {
                                "Sound Effects: ON".to_string()
                            } else {
                                "Sound Effects: OFF".to_string()
                            },
                            2.0,
                        ));
                        self.app_rev = self.app_rev.wrapping_add(1);
                    }
                    if input.key_t {
                        self.prefs.timer_on = u16::from(self.prefs.timer_on == 0);
                        sess.set_preferences(
                            self.prefs.grid_lines_on != 0,
                            self.prefs.timer_on != 0,
                            self.prefs.text_msg_on != 0,
                        );
                        self.audio.play_common(CommonSound::UiClick);
                        sess.status_message = Some((
                            if self.prefs.timer_on != 0 {
                                "Timer: ON".to_string()
                            } else {
                                "Timer: OFF".to_string()
                            },
                            2.0,
                        ));
                        self.app_rev = self.app_rev.wrapping_add(1);
                    }
                    if input.key_m {
                        self.prefs.text_msg_on = u16::from(self.prefs.text_msg_on == 0);
                        sess.set_preferences(
                            self.prefs.grid_lines_on != 0,
                            self.prefs.timer_on != 0,
                            self.prefs.text_msg_on != 0,
                        );
                        self.audio.play_common(CommonSound::UiClick);
                        sess.status_message = Some((
                            if self.prefs.text_msg_on != 0 {
                                "Text Messages: ON".to_string()
                            } else {
                                "Text Messages: OFF".to_string()
                            },
                            2.0,
                        ));
                        self.app_rev = self.app_rev.wrapping_add(1);
                    }
                    if input.up || input.wheel_delta > 0 {
                        sess.scroll_tray(-1);
                    }
                    if input.down || input.wheel_delta < 0 {
                        sess.scroll_tray(1);
                    }

                    if input.left_pressed
                        && sess.handle_pointer_press(
                            input.pointer_x,
                            input.pointer_y,
                            &mut self.audio,
                        ) == Some(PuzzleAction::OpenMenu)
                    {
                        open_menu = true;
                    }

                    if input.left_released {
                        sess.handle_pointer_release(
                            input.pointer_x,
                            input.pointer_y,
                            &mut self.audio,
                        );
                    }
                }

                if open_menu {
                    self.return_to_browser();
                }
            }
        }
    }

    pub fn handle_browser_click(&mut self, mx: i32, my: i32) {
        // Bottom control bar buttons (`y = 170..196`)
        if (170..=196).contains(&my) {
            for &(action, bx, bw, _) in &BROWSER_BUTTONS {
                if (bx..=bx + bw).contains(&mx) {
                    match action {
                        BrowserAction::PrevPuzzle => self.step_puzzle(-1),
                        BrowserAction::NextPuzzle => self.step_puzzle(1),
                        BrowserAction::CyclePieces => self.step_grid_dim(1),
                        BrowserAction::ToggleEdges => self.toggle_curved_edges(),
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

    pub fn present_signature(
        &self,
        mx: i32,
        my: i32,
        show_reference: bool,
        picture_guide: bool,
    ) -> Option<PresentSignature> {
        let caption_key = (self.prefs.text_msg_on != 0)
            .then(|| self.audio.caption_key())
            .flatten();
        Some(match self.screen {
            AppScreen::Title => PresentSignature::Title,
            AppScreen::Credits { page } => PresentSignature::Credits(page),
            AppScreen::Setup { .. } => PresentSignature::Setup {
                app_rev: self.app_rev,
                setup_rev: self.setup_menu.revision(),
            },
            AppScreen::Browser => PresentSignature::Browser {
                app_rev: self.app_rev,
                caption_key,
            },
            AppScreen::Playing => {
                let session = self.session.as_ref()?;
                if show_reference {
                    PresentSignature::Reference {
                        session_id: session.session_id(),
                        asset_rev: session.asset_rev(),
                    }
                } else {
                    PresentSignature::Playing {
                        app_rev: self.app_rev,
                        puzzle: session.present_key(mx, my, picture_guide),
                        caption_key,
                    }
                }
            }
        })
    }
}

/// Software presentation pipeline owning framebuffers and render caches for [`GameApp`].
pub struct AppRenderer {
    pub fb: Framebuffer,
    pub rgba_buf: Vec<u8>,
    scratch_fb: Box<Framebuffer>,
    preview_cache: Option<BrowserPreviewCache>,
    pub puzzle_renderer: PuzzleRenderer,
    last_present_sig: Option<PresentSignature>,
}

impl Default for AppRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl AppRenderer {
    pub fn new() -> Self {
        Self {
            fb: Framebuffer::new(),
            rgba_buf: vec![0u8; (SCREEN_W as usize) * (SCREEN_H as usize) * 4],
            scratch_fb: Box::new(Framebuffer::new()),
            preview_cache: None,
            puzzle_renderer: PuzzleRenderer::new(),
            last_present_sig: None,
        }
    }

    /// Renders the current frame into `self.rgba_buf` only if `present_signature` changed.
    /// Returns `true` when `rgba_buf` was updated.
    pub fn render_if_needed(
        &mut self,
        app: &GameApp,
        mx: i32,
        my: i32,
        show_reference: bool,
        picture_guide: bool,
    ) -> bool {
        let sig = app.present_signature(mx, my, show_reference, picture_guide);
        if sig.is_some() && sig == self.last_present_sig {
            return false;
        }
        if show_reference && let Some(session) = &app.session {
            self.puzzle_renderer.render_reference(session, &mut self.fb);
        } else {
            self.render(app, mx, my);
        }

        self.fb.to_rgba8(&mut self.rgba_buf);
        if picture_guide
            && !show_reference
            && app.screen == AppScreen::Playing
            && let Some(session) = &app.session
        {
            let (px, py) = session.active_pointer(mx, my);
            self.puzzle_renderer
                .blend_picture_guide(session, &mut self.rgba_buf, px, py);
        }
        self.last_present_sig = sig;
        true
    }

    pub fn render(&mut self, app: &GameApp, mx: i32, my: i32) {
        let Self {
            fb,
            scratch_fb,
            preview_cache,
            puzzle_renderer,
            ..
        } = self;
        fb.reset_clip();
        match app.screen {
            AppScreen::Title => {
                if let Some(img) = app.title_images.first() {
                    fb.draw_ilbm(img);
                }
            }
            AppScreen::Credits { page } => {
                if let Some(img) = app
                    .credit_images
                    .get(page)
                    .or_else(|| app.credit_images.first())
                {
                    fb.draw_ilbm(img);
                }
            }
            AppScreen::Setup { .. } => {
                fb.reset_default_palette();
                app.setup_menu.render(fb, &app.font);
            }
            AppScreen::Browser => {
                let key = (app.preview_asset.id, app.preview_asset.uses_ega());
                let needs_rebuild = preview_cache
                    .as_ref()
                    .is_none_or(|c| (c.puzzle_id, c.uses_ega) != key);
                if needs_rebuild {
                    let mut cached_fb = preview_cache
                        .take()
                        .map(|c| c.fb)
                        .unwrap_or_else(|| Box::new(Framebuffer::new()));
                    render_scene_view_with_scratch(
                        &mut cached_fb,
                        scratch_fb,
                        &app.preview_asset,
                        app.preview_asset.static_frame_idx(),
                    );
                    let white = cached_fb.color_index([255; 3]);
                    let black = cached_fb.color_index([0; 3]);
                    let mut button_bgs = [0u8; BROWSER_BUTTONS.len()];
                    for (dst, &(_, _, _, rgb)) in button_bgs.iter_mut().zip(&BROWSER_BUTTONS) {
                        *dst = cached_fb.color_index(rgb);
                    }
                    *preview_cache = Some(BrowserPreviewCache {
                        puzzle_id: key.0,
                        uses_ega: key.1,
                        fb: cached_fb,
                        white,
                        black,
                        button_bgs,
                    });
                }
                let cache = preview_cache.as_ref().unwrap();
                fb.palette = cache.fb.palette;
                fb.pixels.copy_from_slice(&cache.fb.pixels);
                let white = cache.white;
                let black = cache.black;

                // Top banner showing puzzle title
                fb.fill_rect(6, 4, SCREEN_W - 12, 15, black);
                fb.draw_rect_outline(6, 4, SCREEN_W - 12, 15, white);
                let can_resume = app.can_resume_selected();
                let title = if can_resume {
                    format!("Puzzle {:02} of 15   R: restart", app.selected_puzzle)
                } else {
                    format!("Puzzle {:02} of 15", app.selected_puzzle)
                };
                fb.draw_text_shadowed(&app.font, &title, 14, 7, black, white);

                // Bottom interactive control bar (`y = 168..198`)
                fb.fill_rect(2, 168, SCREEN_W - 4, 30, black);
                fb.draw_rect_outline(2, 168, SCREEN_W - 4, 30, white);

                let pcs_label = if app.selected_grid_dim == 1 {
                    format!("{} PARTS", app.preview_asset.preset_piece_count())
                } else {
                    format!("{} PCS", app.selected_grid_dim * app.selected_grid_dim)
                };
                let edge_label = if app.selected_grid_dim == 1 {
                    "ORIGINAL"
                } else if app.curved_edges {
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
                    let tw = app.font.measure_text(label);
                    let tx = x + (w - tw) / 2;
                    fb.draw_text_shadowed(&app.font, label, tx.max(x + 2), 178, black, white);
                }
            }
            AppScreen::Playing => {
                if let Some(ref sess) = app.session {
                    let toolbox = if sess.asset.uses_ega() {
                        &app.tbx_ega
                    } else {
                        &app.tbx
                    };
                    let (px, py) = sess.active_pointer(mx, my);
                    puzzle_renderer.render(sess, fb, toolbox, &app.font, px, py);
                }
            }
        }
        let caption = (app.prefs.text_msg_on != 0
            && matches!(app.screen, AppScreen::Browser | AppScreen::Playing))
        .then(|| app.audio.common_caption())
        .flatten();
        let kbd_pointer = (app.screen == AppScreen::Playing)
            .then(|| app.session.as_ref().and_then(|s| s.keyboard_pointer))
            .flatten();
        if caption.is_some() || kbd_pointer.is_some() || app.error.is_some() {
            let black = fb.color_index([0; 3]);
            let white = fb.color_index([255; 3]);
            if let Some(caption) = caption {
                let line_count = caption.split('\r').count() as i32;
                let height = line_count * app.font.height as i32 + 8;
                let y = 164 - height;
                fb.fill_rect(4, y, SCREEN_W - 8, height, white);
                fb.draw_rect_outline(4, y, SCREEN_W - 8, height, black);
                for (row, line) in caption.split('\r').enumerate() {
                    let x = (SCREEN_W - app.font.measure_text(line)) / 2;
                    fb.draw_text(
                        &app.font,
                        line,
                        x,
                        y + 4 + row as i32 * app.font.height as i32,
                        black,
                    );
                }
            }
            if let Some((x, y)) = kbd_pointer {
                let (x, y) = (x.round() as i32, y.round() as i32);
                fb.draw_rect_outline(x - 3, y - 3, 7, 7, black);
                fb.draw_line(x - 2, y, x + 2, y, white);
                fb.draw_line(x, y - 2, x, y + 2, white);
            }
            if app.error.is_some() {
                fb.fill_rect(0, 20, SCREEN_W, 26, black);
                fb.draw_text(&app.font, "Asset load failed (see terminal)", 5, 23, white);
            }
        }
    }
}
