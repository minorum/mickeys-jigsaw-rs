//! Application state machine, input handling, and software presentation layer.
//!
//! Kept independent of windowing/GPU backends so all screen transitions, input handling,
//! cache invalidation, and framebuffer rendering can be tested headlessly.

use std::path::PathBuf;

use crate::assets::{FontAsset, Preferences, PuzzleAsset, ToolboxAsset, load_ilbm_container};
use crate::audio::{AudioEngine, CommonSound};
use crate::decompress::IlbmImage;
use crate::graphics::{
    Framebuffer, SCREEN_H, SCREEN_W, copy_scene_interior, render_scene_view_with_scratch,
};
use crate::puzzle::{PuzzleAction, PuzzlePresentKey, PuzzleSession};
use crate::puzzle_render::{PuzzleRenderer, draw_playfield_border_remapped};
use crate::setup::SetupMenu;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserAction {
    PrevPuzzle,
    NextPuzzle,
    CyclePieces,
    ToggleEdges,
    OpenSetup,
    StartOrResume,
    PrintPuzzle,
    QuitGame,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BrowserButton {
    pub action: BrowserAction,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserTarget {
    Button(BrowserAction),
    PuzzleSlot(usize),
}

pub const BROWSER_RIGHT_BAR_X: i32 = 304;
pub const BROWSER_RIGHT_BAR_W: i32 = 16;
pub const BROWSER_SLOT_Y0: i32 = 35;
pub const BROWSER_SLOT_H: i32 = 11;
pub const BROWSER_SLOT_COUNT: usize = 15;

pub const BROWSER_BUTTONS: [BrowserButton; 8] = [
    BrowserButton {
        action: BrowserAction::PrevPuzzle,
        x: 148,
        y: 160,
        w: 49,
        h: 32,
    },
    BrowserButton {
        action: BrowserAction::NextPuzzle,
        x: 250,
        y: 160,
        w: 49,
        h: 32,
    },
    BrowserButton {
        action: BrowserAction::CyclePieces,
        x: 88,
        y: 160,
        w: 57,
        h: 15,
    },
    BrowserButton {
        action: BrowserAction::ToggleEdges,
        x: 88,
        y: 177,
        w: 57,
        h: 15,
    },
    BrowserButton {
        action: BrowserAction::OpenSetup,
        x: 215,
        y: 4,
        w: 41,
        h: 15,
    },
    BrowserButton {
        action: BrowserAction::StartOrResume,
        x: 199,
        y: 160,
        w: 49,
        h: 32,
    },
    BrowserButton {
        action: BrowserAction::PrintPuzzle,
        x: 36,
        y: 160,
        w: 49,
        h: 32,
    },
    BrowserButton {
        action: BrowserAction::QuitGame,
        x: 258,
        y: 4,
        w: 41,
        h: 15,
    },
];

impl BrowserTarget {
    /// Returns the interactive Browser control under `(mx, my)`, if any.
    pub fn hit_test(mx: i32, my: i32) -> Option<Self> {
        for btn in BROWSER_BUTTONS {
            if mx >= btn.x && mx < btn.x + btn.w && my >= btn.y && my < btn.y + btn.h {
                return Some(Self::Button(btn.action));
            }
        }
        if (BROWSER_RIGHT_BAR_X..BROWSER_RIGHT_BAR_X + BROWSER_RIGHT_BAR_W).contains(&mx)
            && my >= BROWSER_SLOT_Y0
            && my < BROWSER_SLOT_Y0 + (BROWSER_SLOT_COUNT as i32) * BROWSER_SLOT_H
        {
            let slot = ((my - BROWSER_SLOT_Y0) / BROWSER_SLOT_H) as usize;
            return Some(Self::PuzzleSlot(slot + 1));
        }
        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InGameMenuCommand {
    OtherPicture,
    Setup,
    PieceTray,
    Replay,
    Quit,
    Resume,
}

impl InGameMenuCommand {
    pub const ALL: [Self; 6] = [
        Self::OtherPicture,
        Self::Setup,
        Self::PieceTray,
        Self::Replay,
        Self::Quit,
        Self::Resume,
    ];

    pub fn sprite_idx(self) -> Option<usize> {
        match self {
            Self::OtherPicture => Some(5),
            Self::Setup => Some(6),
            Self::PieceTray => Some(7),
            Self::Replay => Some(8),
            Self::Quit => Some(9),
            Self::Resume => None,
        }
    }
}

pub const IN_GAME_MENU_X: i32 = 84;
pub const IN_GAME_MENU_Y: i32 = 26;
pub const IN_GAME_MENU_W: i32 = 152;
pub const IN_GAME_MENU_H: i32 = 148;
pub const IN_GAME_MENU_ROW_X: i32 = 92;
pub const IN_GAME_MENU_ROW_Y0: i32 = 46;
pub const IN_GAME_MENU_ROW_W: i32 = 136;
pub const IN_GAME_MENU_ROW_H: i32 = 18;
pub const IN_GAME_MENU_ROW_STEP: i32 = 20;

pub fn in_game_menu_row_rect(idx: usize) -> (i32, i32, i32, i32) {
    (
        IN_GAME_MENU_ROW_X,
        IN_GAME_MENU_ROW_Y0 + idx as i32 * IN_GAME_MENU_ROW_STEP,
        IN_GAME_MENU_ROW_W,
        IN_GAME_MENU_ROW_H,
    )
}

pub fn in_game_menu_command_at(mx: i32, my: i32) -> Option<InGameMenuCommand> {
    for (idx, cmd) in InGameMenuCommand::ALL.into_iter().enumerate() {
        let (x, y, w, h) = in_game_menu_row_rect(idx);
        if (x..x + w).contains(&mx) && (y..y + h).contains(&my) {
            return Some(cmd);
        }
    }
    None
}

pub fn in_game_menu_contains(mx: i32, my: i32) -> bool {
    (IN_GAME_MENU_X..IN_GAME_MENU_X + IN_GAME_MENU_W).contains(&mx)
        && (IN_GAME_MENU_Y..IN_GAME_MENU_Y + IN_GAME_MENU_H).contains(&my)
}

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
        hover: Option<BrowserTarget>,
        pressed: Option<BrowserTarget>,
    },
    Reference {
        session_id: u64,
        asset_rev: u64,
    },
    Playing {
        app_rev: u64,
        puzzle: PuzzlePresentKey,
        caption_key: Option<u64>,
        menu_state: Option<(usize, Option<InGameMenuCommand>, Option<InGameMenuCommand>)>,
    },
}

/// Platform-agnostic per-frame input snapshot consumed by [`GameApp::handle_input`].
#[derive(Clone, Copy, Debug, Default)]
pub struct AppInput {
    pub dt: f32,
    pub pointer_x: i32,
    pub pointer_y: i32,
    pub pointer_moved: bool,
    pub left_pressed: bool,
    pub left_released: bool,
    pub right_pressed: bool,
    pub pointer_canceled: bool,
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
    pub key_q: bool,
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
    tbx_remap: [u8; 256],
    white: u8,
    black: u8,
    yellow: u8,
    panel_bg: u8,
    panel_hi: u8,
    panel_lo: u8,
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
    pub confirm_quit_puzzle: bool,
    pub in_game_menu_open: bool,
    pub in_game_menu_selected: usize,
    pub in_game_menu_pressed: Option<InGameMenuCommand>,
    pub browser_status: Option<(String, f32)>,
    pub browser_hover: Option<BrowserTarget>,
    pub browser_pressed: Option<BrowserTarget>,
    pub quit_requested: bool,
    pub(crate) quit_elapsed: f32,
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
            confirm_quit_puzzle: false,
            in_game_menu_open: false,
            in_game_menu_selected: 0,
            in_game_menu_pressed: None,
            browser_status: None,
            browser_hover: None,
            browser_pressed: None,
            quit_requested: false,
            quit_elapsed: 0.0,
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
            confirm_quit_puzzle: false,
            in_game_menu_open: false,
            in_game_menu_selected: 0,
            in_game_menu_pressed: None,
            browser_status: None,
            browser_hover: None,
            browser_pressed: None,
            quit_requested: false,
            quit_elapsed: 0.0,
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
        self.confirm_quit_puzzle = false;
        self.in_game_menu_open = false;
        self.in_game_menu_pressed = None;
        self.audio.stop_all();
        self.audio.play_common(CommonSound::OtherPicture);
        self.set_screen(AppScreen::Browser);
    }

    fn return_to_browser(&mut self) {
        self.in_game_menu_open = false;
        self.in_game_menu_pressed = None;
        self.confirm_quit_puzzle = false;
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

    pub fn open_in_game_menu(&mut self) {
        if self.screen != AppScreen::Playing {
            return;
        }
        if let Some(sess) = &mut self.session {
            sess.cancel_pointer_drag();
            sess.pressed_toolbox_target = None;
        }
        self.in_game_menu_open = true;
        self.in_game_menu_selected = 0;
        self.in_game_menu_pressed = None;
        self.audio.play_ui_click();
        self.bump_rev();
    }

    pub fn close_in_game_menu(&mut self, play_cancel: bool) {
        self.in_game_menu_open = false;
        self.in_game_menu_pressed = None;
        if play_cancel {
            self.audio.play_common(CommonSound::Cancel);
        }
        self.bump_rev();
    }

    pub fn is_in_game_menu_command_enabled(&self, cmd: InGameMenuCommand) -> bool {
        match cmd {
            InGameMenuCommand::PieceTray => self.session.as_ref().is_some_and(|s| !s.solved),
            InGameMenuCommand::Replay => self.session.as_ref().is_some_and(|s| s.solved),
            _ => true,
        }
    }

    fn step_in_game_menu_selection(&mut self, delta: isize) {
        let len = InGameMenuCommand::ALL.len() as isize;
        let mut idx = self.in_game_menu_selected as isize;
        for _ in 0..len {
            idx = (idx + delta).rem_euclid(len);
            let cmd = InGameMenuCommand::ALL[idx as usize];
            if self.is_in_game_menu_command_enabled(cmd) {
                self.in_game_menu_selected = idx as usize;
                self.audio.play_ui_click();
                self.bump_rev();
                return;
            }
        }
    }

    pub fn execute_in_game_menu_command(&mut self, cmd: InGameMenuCommand) {
        if !self.is_in_game_menu_command_enabled(cmd) {
            self.audio.play_error();
            return;
        }
        self.in_game_menu_open = false;
        self.in_game_menu_pressed = None;
        match cmd {
            InGameMenuCommand::OtherPicture => self.return_to_browser(),
            InGameMenuCommand::Setup => self.open_setup(true),
            InGameMenuCommand::PieceTray => {
                if let Some(sess) = &mut self.session {
                    if sess.held_piece.is_some() {
                        if sess.held_group.is_empty() {
                            sess.return_held_to_tray(&mut self.audio);
                            if !sess.toolbox_open {
                                sess.toggle_toolbox();
                            }
                        } else {
                            self.audio.play_error();
                        }
                    } else {
                        sess.toggle_toolbox();
                        self.audio.play_ui_click();
                    }
                }
                self.bump_rev();
            }
            InGameMenuCommand::Replay => {
                if let Some(sess) = &mut self.session {
                    sess.replay_sequence(&mut self.audio);
                }
                self.bump_rev();
            }
            InGameMenuCommand::Quit => self.request_quit_from_puzzle(),
            InGameMenuCommand::Resume => {
                self.audio.play_common(CommonSound::Cancel);
                self.bump_rev();
            }
        }
    }

    pub fn request_quit_from_puzzle(&mut self) {
        self.in_game_menu_open = false;
        self.in_game_menu_pressed = None;
        self.confirm_exit_puzzle = false;
        if self.confirm_quit_puzzle {
            return;
        }
        self.confirm_quit_puzzle = true;
        let solved = self.session.as_ref().is_some_and(|s| s.solved);
        if !solved {
            self.audio.play_common(CommonSound::NotDoneYet);
        }
        if let Some(sess) = &mut self.session {
            sess.cancel_pointer_drag();
            sess.status_message = Some(("Quit game? (Enter/Click=Yes, Esc=No)".to_string(), 8.0));
        }
        self.bump_rev();
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

    pub fn select_puzzle(&mut self, puzzle_id: usize) {
        let clamped = puzzle_id.clamp(1, 15);
        if self.selected_puzzle != clamped {
            self.selected_puzzle = clamped;
            self.reload_preview();
        }
        self.audio.play_common(CommonSound::UiClick);
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

    pub fn print_selected_puzzle(&mut self) {
        let msg = match crate::puzzle::export_print_pbm(&self.game_dir, self.selected_puzzle) {
            Ok(ok_msg) => {
                self.audio.play_common(CommonSound::UiClick);
                ok_msg
            }
            Err(err_msg) => {
                self.audio.play_error();
                err_msg
            }
        };
        self.browser_status = Some((msg, 3.0));
        self.bump_rev();
    }

    pub fn request_quit(&mut self) {
        if self.quit_requested {
            return;
        }
        self.audio.play_common(CommonSound::Quit);
        self.quit_requested = true;
        self.quit_elapsed = 0.0;
        self.bump_rev();
    }

    /// Returns `true` once the quitting phase has finished playing the exit cue (or timed out).
    pub fn should_exit(&self) -> bool {
        self.quit_requested && (!self.audio.is_effect_playing() || self.quit_elapsed >= 3.0)
    }

    pub fn has_mickey_cursor(&self) -> bool {
        self.tbx.sprites.len() > 64
    }

    pub fn open_setup(&mut self, return_to_playing: bool) {
        self.in_game_menu_open = false;
        self.in_game_menu_pressed = None;
        self.confirm_exit_puzzle = false;
        self.confirm_quit_puzzle = false;
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
        if self.quit_requested {
            self.quit_elapsed += dt;
            return;
        }
        self.process_input(input, dt);
        self.audio
            .set_paused(matches!(self.screen, AppScreen::Setup { .. }));
    }

    fn process_input(&mut self, input: &AppInput, dt: f32) {
        if input.pointer_canceled {
            self.browser_pressed = None;
            self.in_game_menu_pressed = None;
            if let Some(session) = &mut self.session {
                session.cancel_pointer_drag();
            }
        }
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
                if let Some((_, ref mut timer)) = self.browser_status {
                    *timer -= dt;
                    if *timer <= 0.0 {
                        self.browser_status = None;
                        self.bump_rev();
                    }
                }
                if input.pointer_moved {
                    self.browser_hover = BrowserTarget::hit_test(input.pointer_x, input.pointer_y);
                }
                if input.left_released {
                    self.browser_pressed = None;
                }
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
                } else if input.key_p {
                    self.print_selected_puzzle();
                } else if input.escape || input.key_q {
                    self.request_quit();
                } else if input.left_pressed {
                    let target = BrowserTarget::hit_test(input.pointer_x, input.pointer_y);
                    self.browser_hover = target;
                    self.browser_pressed = target;
                    self.handle_browser_click(input.pointer_x, input.pointer_y);
                }
                if self.screen != AppScreen::Browser {
                    self.browser_hover = None;
                    self.browser_pressed = None;
                }
            }
            AppScreen::Playing => {
                if self.confirm_exit_puzzle || self.confirm_quit_puzzle {
                    if input.enter || input.left_pressed {
                        if self.confirm_quit_puzzle {
                            self.confirm_quit_puzzle = false;
                            if let Some(sess) = &mut self.session {
                                sess.status_message = None;
                            }
                            self.request_quit();
                        } else {
                            self.confirm_exit_puzzle = false;
                            self.do_return_to_browser();
                        }
                    } else if input.escape {
                        self.confirm_exit_puzzle = false;
                        self.confirm_quit_puzzle = false;
                        self.audio.play_common(CommonSound::Cancel);
                        if let Some(sess) = &mut self.session {
                            sess.status_message = None;
                        }
                        self.bump_rev();
                    }
                    return;
                }
                if self.in_game_menu_open {
                    if let Some(sess) = &mut self.session {
                        if input.pointer_moved
                            && input.move_x == 0
                            && input.move_y == 0
                            && !input.enter
                        {
                            sess.clear_keyboard_pointer();
                        }
                        if !input.left_pressed
                            && !input.right_pressed
                            && (input.move_x != 0 || input.move_y != 0)
                        {
                            sess.move_keyboard_pointer(
                                input.move_x as f32,
                                input.move_y as f32,
                                dt,
                            );
                        }
                    }
                    let (px, py) = self
                        .session
                        .as_ref()
                        .map_or((input.pointer_x, input.pointer_y), |s| {
                            s.active_pointer(input.pointer_x, input.pointer_y)
                        });
                    if (input.pointer_moved || input.move_x != 0 || input.move_y != 0)
                        && let Some(cmd) = in_game_menu_command_at(px, py)
                        && self.is_in_game_menu_command_enabled(cmd)
                        && let Some(idx) = InGameMenuCommand::ALL.iter().position(|&c| c == cmd)
                        && self.in_game_menu_selected != idx
                    {
                        self.in_game_menu_selected = idx;
                        self.bump_rev();
                    }
                    if input.escape {
                        self.close_in_game_menu(true);
                        return;
                    }
                    if input.up {
                        self.step_in_game_menu_selection(-1);
                        return;
                    }
                    if input.down {
                        self.step_in_game_menu_selection(1);
                        return;
                    }
                    if input.key_r
                        && self.is_in_game_menu_command_enabled(InGameMenuCommand::Replay)
                    {
                        self.execute_in_game_menu_command(InGameMenuCommand::Replay);
                        return;
                    }
                    if input.key_q {
                        self.execute_in_game_menu_command(InGameMenuCommand::Quit);
                        return;
                    }
                    if input.enter || input.space {
                        let cmd = InGameMenuCommand::ALL[self.in_game_menu_selected];
                        self.execute_in_game_menu_command(cmd);
                        return;
                    }
                    if input.left_pressed {
                        if let Some(sess) = &mut self.session {
                            sess.clear_keyboard_pointer();
                        }
                        if let Some(cmd) = in_game_menu_command_at(input.pointer_x, input.pointer_y)
                        {
                            if self.is_in_game_menu_command_enabled(cmd) {
                                self.in_game_menu_pressed = Some(cmd);
                                if let Some(idx) =
                                    InGameMenuCommand::ALL.iter().position(|&c| c == cmd)
                                {
                                    self.in_game_menu_selected = idx;
                                }
                                self.bump_rev();
                            } else {
                                self.audio.play_error();
                            }
                        } else if !in_game_menu_contains(input.pointer_x, input.pointer_y) {
                            self.close_in_game_menu(true);
                        }
                        return;
                    }
                    if input.left_released {
                        if let Some(sess) = &mut self.session {
                            sess.clear_keyboard_pointer();
                        }
                        if let Some(pressed_cmd) = self.in_game_menu_pressed.take() {
                            if in_game_menu_command_at(input.pointer_x, input.pointer_y)
                                == Some(pressed_cmd)
                            {
                                self.execute_in_game_menu_command(pressed_cmd);
                            } else {
                                self.bump_rev();
                            }
                        }
                        return;
                    }
                    return;
                }
                if input.escape {
                    self.return_to_browser();
                    return;
                }
                if input.key_q {
                    self.request_quit_from_puzzle();
                    return;
                }
                let mut open_menu = false;

                if let Some(ref mut sess) = self.session {
                    let was_animating = sess.active_anim.is_some();
                    sess.update(dt, &mut self.audio);

                    if input.right_pressed
                        || (input.pointer_moved
                            && input.move_x == 0
                            && input.move_y == 0
                            && !input.enter)
                    {
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
                    if input.key_r && sess.replay_sequence(&mut self.audio) {
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
                    self.open_in_game_menu();
                }
            }
        }
    }

    pub fn handle_browser_click(&mut self, mx: i32, my: i32) {
        match BrowserTarget::hit_test(mx, my) {
            Some(BrowserTarget::Button(action)) => match action {
                BrowserAction::PrevPuzzle => self.step_puzzle(-1),
                BrowserAction::NextPuzzle => self.step_puzzle(1),
                BrowserAction::CyclePieces => self.step_grid_dim(1),
                BrowserAction::ToggleEdges => self.toggle_curved_edges(),
                BrowserAction::OpenSetup => {
                    self.audio.play_common(CommonSound::SetupStart);
                    self.open_setup(false);
                }
                BrowserAction::StartOrResume => self.start_selected_puzzle(),
                BrowserAction::PrintPuzzle => self.print_selected_puzzle(),
                BrowserAction::QuitGame => self.request_quit(),
            },
            Some(BrowserTarget::PuzzleSlot(puzzle_id)) => self.select_puzzle(puzzle_id),
            None => {
                if (34..302).contains(&mx) && (22..158).contains(&my) {
                    self.start_selected_puzzle();
                }
            }
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
                hover: self.browser_hover,
                pressed: self.browser_pressed,
            },
            AppScreen::Playing => {
                let session = self.session.as_ref()?;
                if show_reference {
                    PresentSignature::Reference {
                        session_id: session.session_id(),
                        asset_rev: session.asset_rev(),
                    }
                } else {
                    let menu_state = self.in_game_menu_open.then(|| {
                        let (px, py) = session.active_pointer(mx, my);
                        let hovered = in_game_menu_command_at(px, py)
                            .filter(|&cmd| self.is_in_game_menu_command_enabled(cmd));
                        (
                            self.in_game_menu_selected,
                            hovered,
                            self.in_game_menu_pressed,
                        )
                    });
                    let (board_mx, board_my) = if self.in_game_menu_open {
                        (-1, -1)
                    } else {
                        (mx, my)
                    };
                    PresentSignature::Playing {
                        app_rev: self.app_rev,
                        puzzle: session.present_key(board_mx, board_my, picture_guide),
                        caption_key,
                        menu_state,
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
    base_rgba_buf: Vec<u8>,
    scratch_fb: Box<Framebuffer>,
    preview_cache: Option<BrowserPreviewCache>,
    pub puzzle_renderer: PuzzleRenderer,
    last_present_sig: Option<PresentSignature>,
    last_cursor_sig: Option<(usize, i32, i32)>,
}

impl Default for AppRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl AppRenderer {
    pub fn new() -> Self {
        let buf_len = (SCREEN_W as usize) * (SCREEN_H as usize) * 4;
        Self {
            fb: Framebuffer::new(),
            rgba_buf: vec![0u8; buf_len],
            base_rgba_buf: vec![0u8; buf_len],
            scratch_fb: Box::new(Framebuffer::new()),
            preview_cache: None,
            puzzle_renderer: PuzzleRenderer::new(),
            last_present_sig: None,
            last_cursor_sig: None,
        }
    }

    pub(crate) fn cursor_hotspot(sprite_idx: usize) -> (i32, i32) {
        match sprite_idx {
            62 => (-6, 0),
            63 => (-10, 0),
            64 => (-7, 0),
            _ => (0, 0),
        }
    }

    pub(crate) fn desired_cursor(
        app: &GameApp,
        mx: i32,
        my: i32,
        show_reference: bool,
    ) -> Option<(usize, i32, i32)> {
        if show_reference {
            return None;
        }
        let (px, py) = if app.screen == AppScreen::Playing {
            app.session
                .as_ref()
                .map_or((mx, my), |s| s.active_pointer(mx, my))
        } else {
            (mx, my)
        };
        if !(0..SCREEN_W).contains(&px) || !(0..SCREEN_H).contains(&py) {
            return None;
        }
        let sprite_idx = match app.screen {
            AppScreen::Playing => match app.session.as_ref() {
                Some(_) if app.confirm_exit_puzzle || app.confirm_quit_puzzle => 62,
                Some(_) if app.in_game_menu_pressed.is_some() => 63,
                Some(_) if app.in_game_menu_open => 62,
                Some(sess)
                    if sess.held_piece.is_some() || sess.pressed_toolbox_target.is_some() =>
                {
                    63
                }
                Some(sess)
                    if sess.toolbox_target_at(px, py).is_some()
                        || (sess.toolbox_open && px < 46)
                        || sess.solved =>
                {
                    62
                }
                Some(_) => 64,
                None => 62,
            },
            AppScreen::Browser if app.browser_pressed.is_some() => 63,
            _ => 62,
        };
        app.tbx.sprites.get(sprite_idx)?;
        let (hx, hy) = Self::cursor_hotspot(sprite_idx);
        Some((sprite_idx, px + hx, py + hy))
    }

    fn blit_cursor_rgba(&mut self, app: &GameApp, sprite_idx: usize, px: i32, py: i32) {
        let Some(sprite) = app.tbx.sprites.get(sprite_idx) else {
            return;
        };
        let w = sprite.width as i32;
        let h = sprite.height as i32;
        let stride = sprite.stride as usize;
        for sy in 0..h {
            let dy = py + sy;
            if !(0..SCREEN_H).contains(&dy) {
                continue;
            }
            let row_off = sy as usize * stride;
            for sx in 0..w {
                let dx = px + sx;
                if !(0..SCREEN_W).contains(&dx) {
                    continue;
                }
                let idx = row_off + sx as usize;
                let opaque = sprite
                    .opacity
                    .as_ref()
                    .and_then(|op| op.get(idx).copied())
                    .unwrap_or_else(|| sprite.pixels[idx] != sprite.transparent_color);
                if !opaque {
                    continue;
                }
                let rgb = app.tbx.palette[sprite.pixels[idx] as usize];
                let dst = ((dy as usize * SCREEN_W as usize) + dx as usize) * 4;
                self.rgba_buf[dst] = rgb[0];
                self.rgba_buf[dst + 1] = rgb[1];
                self.rgba_buf[dst + 2] = rgb[2];
                self.rgba_buf[dst + 3] = 255;
            }
        }
    }

    /// Renders a cursor sprite (`62`, `63`, `64`) into `rgba_out` with transparent background.
    /// Returns `Some((w, h))` on success.
    pub fn render_cursor_rgba(
        app: &GameApp,
        sprite_idx: usize,
        rgba_out: &mut Vec<u8>,
    ) -> Option<(i32, i32)> {
        let sprite = app.tbx.sprites.get(sprite_idx)?;
        let w = sprite.width as i32;
        let h = sprite.height as i32;
        let stride = sprite.stride as usize;
        rgba_out.clear();
        rgba_out.resize((w * h * 4) as usize, 0);
        for sy in 0..h {
            let row_off = sy as usize * stride;
            let dst_row = (sy as usize) * (w as usize) * 4;
            for sx in 0..w {
                let idx = row_off + sx as usize;
                let opaque = sprite
                    .opacity
                    .as_ref()
                    .and_then(|op| op.get(idx).copied())
                    .unwrap_or_else(|| sprite.pixels[idx] != sprite.transparent_color);
                if !opaque {
                    continue;
                }
                let rgb = app.tbx.palette[sprite.pixels[idx] as usize];
                let d = dst_row + (sx as usize) * 4;
                rgba_out[d] = rgb[0];
                rgba_out[d + 1] = rgb[1];
                rgba_out[d + 2] = rgb[2];
                rgba_out[d + 3] = 255;
            }
        }
        Some((w, h))
    }

    fn update_base_rgba(
        &mut self,
        app: &GameApp,
        mx: i32,
        my: i32,
        show_reference: bool,
        picture_guide: bool,
    ) -> bool {
        let sig = app.present_signature(mx, my, show_reference, picture_guide);
        let base_dirty = sig.is_none() || sig != self.last_present_sig;
        if !base_dirty {
            return false;
        }
        if show_reference && let Some(session) = &app.session {
            self.puzzle_renderer.render_reference(session, &mut self.fb);
        } else {
            self.render(app, mx, my);
        }

        self.fb.to_rgba8(&mut self.base_rgba_buf);
        if picture_guide
            && !show_reference
            && app.screen == AppScreen::Playing
            && let Some(session) = &app.session
        {
            let (px, py) = session.active_pointer(mx, my);
            self.puzzle_renderer
                .blend_picture_guide(session, &mut self.base_rgba_buf, px, py);
        }
        self.last_present_sig = sig;
        true
    }

    /// Renders only the base frame (no cursor) into `self.base_rgba_buf` when `present_signature`
    /// changes. Used by the GPU-composited SDL3 backend for non-board screens, animations, and menus.
    pub fn render_base_only(
        &mut self,
        app: &GameApp,
        mx: i32,
        my: i32,
        show_reference: bool,
        picture_guide: bool,
    ) -> bool {
        self.update_base_rgba(app, mx, my, show_reference, picture_guide)
    }

    pub fn base_rgba(&self) -> &[u8] {
        &self.base_rgba_buf
    }

    /// Renders interactive-board overlays (`status_message`, `caption`, `app.error`) into a
    /// `320x200x4` transparent RGBA8 buffer. Returns `false` if no overlay is active.
    pub fn render_playing_overlay_rgba(&mut self, app: &GameApp, rgba_out: &mut [u8]) -> bool {
        assert_eq!(rgba_out.len(), (SCREEN_W * SCREEN_H * 4) as usize);
        let sess = match &app.session {
            Some(s) => s,
            None => return false,
        };
        let status = sess.status_message();
        let caption = (app.prefs.text_msg_on != 0)
            .then(|| app.audio.common_caption())
            .flatten();
        let has_error = app.error.is_some();
        if status.is_none() && caption.is_none() && !has_error {
            return false;
        }

        self.scratch_fb.reset_clip();
        self.scratch_fb.palette = sess.asset.palette;
        let black = self.scratch_fb.color_index([0; 3]);
        let white = self.scratch_fb.color_index([255; 3]);
        let yellow = self.scratch_fb.color_index([255, 255, 85]);
        // Pick a sentinel byte distinct from all colors drawn in this overlay.
        let sentinel = (0u8..=255)
            .find(|&c| c != black && c != white && c != yellow)
            .unwrap_or(255);
        self.scratch_fb.clear(sentinel);

        if let Some(status) = status {
            self.scratch_fb.draw_text_shadowed(
                &app.font,
                status,
                sess.playfield_x() + 6,
                186,
                black,
                yellow,
            );
        }
        if let Some(caption) = caption {
            let line_count = caption.split('\r').count() as i32;
            let height = line_count * app.font.height as i32 + 8;
            let y = 164 - height;
            self.scratch_fb.fill_rect(4, y, SCREEN_W - 8, height, white);
            self.scratch_fb
                .draw_rect_outline(4, y, SCREEN_W - 8, height, black);
            for (row, line) in caption.split('\r').enumerate() {
                let x = (SCREEN_W - app.font.measure_text(line)) / 2;
                self.scratch_fb.draw_text(
                    &app.font,
                    line,
                    x,
                    y + 4 + row as i32 * app.font.height as i32,
                    black,
                );
            }
        }
        if has_error {
            self.scratch_fb.fill_rect(0, 20, SCREEN_W, 26, black);
            self.scratch_fb
                .draw_text(&app.font, "Asset load failed (see terminal)", 5, 23, white);
        }

        for (i, &px) in self.scratch_fb.pixels.iter().enumerate() {
            let d = i * 4;
            if px == sentinel {
                rgba_out[d..d + 4].copy_from_slice(&[0, 0, 0, 0]);
            } else {
                let rgb = self.scratch_fb.palette[px as usize];
                rgba_out[d] = rgb[0];
                rgba_out[d + 1] = rgb[1];
                rgba_out[d + 2] = rgb[2];
                rgba_out[d + 3] = 255;
            }
        }
        true
    }

    /// Renders the current frame into `self.rgba_buf` only if `present_signature` or cursor changed.
    /// Returns `true` when `rgba_buf` was updated.
    pub fn render_if_needed(
        &mut self,
        app: &GameApp,
        mx: i32,
        my: i32,
        show_reference: bool,
        picture_guide: bool,
    ) -> bool {
        let cursor_sig = Self::desired_cursor(app, mx, my, show_reference);
        let base_dirty = self.update_base_rgba(app, mx, my, show_reference, picture_guide);
        if !base_dirty && cursor_sig == self.last_cursor_sig {
            return false;
        }
        self.rgba_buf.copy_from_slice(&self.base_rgba_buf);
        if let Some((sprite_idx, px, py)) = cursor_sig {
            self.blit_cursor_rgba(app, sprite_idx, px, py);
        }
        self.last_cursor_sig = cursor_sig;
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
                let tbx = if key.1 { &app.tbx_ega } else { &app.tbx };
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
                    let black = cached_fb.color_index([0; 3]);
                    cached_fb.clear(black);
                    copy_scene_interior(&mut cached_fb, scratch_fb, 34);

                    let mut tbx_remap = [0u8; 256];
                    for (i, &rgb) in tbx.palette.iter().enumerate() {
                        tbx_remap[i] = cached_fb.color_index(rgb);
                    }
                    draw_playfield_border_remapped(&mut cached_fb, tbx, 32, 272, &tbx_remap);
                    if tbx.sprites.len() > 12 {
                        cached_fb.blit_sprite_remapped(&tbx.sprites[10], 0, 0, 0, &tbx_remap);
                        for row in 0..10 {
                            cached_fb.blit_sprite_remapped(
                                &tbx.sprites[11],
                                0,
                                8 + row * 19,
                                0,
                                &tbx_remap,
                            );
                        }
                        cached_fb.blit_sprite_remapped(&tbx.sprites[12], 0, 198, 0, &tbx_remap);
                    }
                    if tbx.sprites.len() > 13 {
                        cached_fb.blit_sprite_remapped(
                            &tbx.sprites[13],
                            BROWSER_RIGHT_BAR_X,
                            0,
                            0,
                            &tbx_remap,
                        );
                    }
                    if tbx.sprites.len() > 17 {
                        cached_fb.blit_sprite_remapped(
                            &tbx.sprites[17],
                            BROWSER_RIGHT_BAR_X,
                            199,
                            0,
                            &tbx_remap,
                        );
                    }
                    let white = cached_fb.color_index([255; 3]);
                    let yellow = cached_fb.color_index([255, 255, 85]);
                    let panel_bg = cached_fb.color_index([140, 140, 148]);
                    let panel_hi = cached_fb.color_index([220, 220, 228]);
                    let panel_lo = cached_fb.color_index([64, 64, 72]);
                    *preview_cache = Some(BrowserPreviewCache {
                        puzzle_id: key.0,
                        uses_ega: key.1,
                        fb: cached_fb,
                        tbx_remap,
                        white,
                        black,
                        yellow,
                        panel_bg,
                        panel_hi,
                        panel_lo,
                    });
                }
                let cache = preview_cache.as_ref().unwrap();
                fb.palette = cache.fb.palette;
                fb.pixels.copy_from_slice(&cache.fb.pixels);
                let white = cache.white;
                let black = cache.black;
                let yellow = cache.yellow;
                let can_resume = app.can_resume_selected();

                // Right sidebar puzzle slots (`1..=15`)
                for i in 0..BROWSER_SLOT_COUNT {
                    let puzzle_id = i + 1;
                    let y = BROWSER_SLOT_Y0 + (i as i32) * BROWSER_SLOT_H;
                    let slot_target = BrowserTarget::PuzzleSlot(puzzle_id);
                    let is_selected = app.selected_puzzle == puzzle_id;
                    let is_hovered = app.browser_hover == Some(slot_target);
                    let is_pressed = app.browser_pressed == Some(slot_target);
                    if tbx.sprites.len() > 15 {
                        fb.blit_sprite_remapped(
                            &tbx.sprites[14 + (i & 1)],
                            BROWSER_RIGHT_BAR_X,
                            y,
                            0,
                            &cache.tbx_remap,
                        );
                    } else {
                        fb.fill_rect(
                            BROWSER_RIGHT_BAR_X,
                            y,
                            BROWSER_RIGHT_BAR_W,
                            BROWSER_SLOT_H,
                            cache.panel_bg,
                        );
                    }
                    if is_selected && tbx.sprites.len() > 16 {
                        fb.blit_sprite_remapped(
                            &tbx.sprites[16],
                            BROWSER_RIGHT_BAR_X + 3,
                            y,
                            0,
                            &cache.tbx_remap,
                        );
                    }
                    if tbx.sprites.len() > 27 {
                        if puzzle_id < 10 {
                            fb.blit_sprite_remapped(
                                &tbx.sprites[18 + puzzle_id],
                                BROWSER_RIGHT_BAR_X + 6,
                                y + 2,
                                0,
                                &cache.tbx_remap,
                            );
                        } else {
                            fb.blit_sprite_remapped(
                                &tbx.sprites[18 + puzzle_id / 10],
                                BROWSER_RIGHT_BAR_X + 4,
                                y + 2,
                                0,
                                &cache.tbx_remap,
                            );
                            fb.blit_sprite_remapped(
                                &tbx.sprites[18 + puzzle_id % 10],
                                BROWSER_RIGHT_BAR_X + 9,
                                y + 2,
                                0,
                                &cache.tbx_remap,
                            );
                        }
                    } else {
                        let num = format!("{puzzle_id}");
                        fb.draw_text(&app.font, &num, BROWSER_RIGHT_BAR_X + 2, y + 2, white);
                    }
                    if is_pressed {
                        fb.draw_rect_outline(
                            BROWSER_RIGHT_BAR_X,
                            y,
                            BROWSER_RIGHT_BAR_W,
                            BROWSER_SLOT_H,
                            white,
                        );
                    } else if is_hovered {
                        fb.draw_rect_outline(
                            BROWSER_RIGHT_BAR_X,
                            y,
                            BROWSER_RIGHT_BAR_W,
                            BROWSER_SLOT_H,
                            yellow,
                        );
                    }
                }

                // Top banner showing puzzle title
                fb.fill_rect(36, 4, 176, 15, black);
                fb.draw_rect_outline(36, 4, 176, 15, white);
                let title = if can_resume {
                    format!("Puzzle {:02} of 15  R:reset", app.selected_puzzle)
                } else {
                    format!("Puzzle {:02} of 15", app.selected_puzzle)
                };
                fb.draw_text_shadowed(&app.font, &title, 42, 7, black, white);

                let pcs_label = if app.selected_grid_dim == 1 {
                    format!("{} PC", app.preview_asset.preset_piece_count())
                } else {
                    format!("{} PCS", app.selected_grid_dim * app.selected_grid_dim)
                };
                let edge_label = if app.selected_grid_dim == 1 {
                    "ORIG"
                } else if app.curved_edges {
                    "CURVED"
                } else {
                    "SQUARE"
                };

                for btn in BROWSER_BUTTONS {
                    let target = BrowserTarget::Button(btn.action);
                    let is_hovered = app.browser_hover == Some(target);
                    let is_pressed = app.browser_pressed == Some(target);
                    match btn.action {
                        BrowserAction::PrintPuzzle
                        | BrowserAction::PrevPuzzle
                        | BrowserAction::StartOrResume
                        | BrowserAction::NextPuzzle => {
                            if tbx.sprites.len() > 28 {
                                fb.blit_sprite_remapped(
                                    &tbx.sprites[28],
                                    btn.x,
                                    btn.y,
                                    0,
                                    &cache.tbx_remap,
                                );
                            } else {
                                fb.fill_rect(btn.x, btn.y, btn.w, btn.h, cache.panel_bg);
                                fb.draw_rect_outline(btn.x, btn.y, btn.w, btn.h, white);
                            }
                            let icon_spec = match btn.action {
                                BrowserAction::PrintPuzzle => (33usize, 6, 5, "PRT"),
                                BrowserAction::PrevPuzzle => (31, 11, 7, "<PREV"),
                                BrowserAction::StartOrResume => (29, 12, 9, "OK"),
                                BrowserAction::NextPuzzle => (32, 11, 7, "NEXT>"),
                                _ => unreachable!(),
                            };
                            if let Some(spr) = tbx.sprites.get(icon_spec.0) {
                                fb.blit_sprite_remapped(
                                    spr,
                                    btn.x + icon_spec.1,
                                    btn.y + icon_spec.2,
                                    0,
                                    &cache.tbx_remap,
                                );
                            } else {
                                let tw = app.font.measure_text(icon_spec.3);
                                fb.draw_text_shadowed(
                                    &app.font,
                                    icon_spec.3,
                                    btn.x + (btn.w - tw) / 2,
                                    btn.y + 12,
                                    black,
                                    white,
                                );
                            }
                            if btn.action == BrowserAction::StartOrResume && can_resume {
                                let tw = app.font.measure_text("RES");
                                fb.draw_text_shadowed(
                                    &app.font,
                                    "RES",
                                    btn.x + (btn.w - tw) / 2,
                                    btn.y + 23,
                                    black,
                                    yellow,
                                );
                            }
                            if is_pressed {
                                fb.draw_rect_outline(btn.x, btn.y, btn.w, btn.h, white);
                                fb.draw_rect_outline(
                                    btn.x + 1,
                                    btn.y + 1,
                                    btn.w - 2,
                                    btn.h - 2,
                                    black,
                                );
                            } else if is_hovered {
                                fb.draw_rect_outline(btn.x, btn.y, btn.w, btn.h, yellow);
                            }
                        }
                        BrowserAction::OpenSetup
                        | BrowserAction::QuitGame
                        | BrowserAction::CyclePieces
                        | BrowserAction::ToggleEdges => {
                            fb.fill_rect(btn.x, btn.y, btn.w, btn.h, cache.panel_bg);
                            let (tl, br) = if is_pressed {
                                (cache.panel_lo, cache.panel_hi)
                            } else {
                                (cache.panel_hi, cache.panel_lo)
                            };
                            fb.fill_rect(btn.x, btn.y, btn.w, 1, tl);
                            fb.fill_rect(btn.x, btn.y, 1, btn.h, tl);
                            fb.fill_rect(btn.x, btn.y + btn.h - 1, btn.w, 1, br);
                            fb.fill_rect(btn.x + btn.w - 1, btn.y, 1, btn.h, br);
                            if is_pressed {
                                fb.draw_rect_outline(btn.x, btn.y, btn.w, btn.h, white);
                            } else if is_hovered {
                                fb.draw_rect_outline(btn.x, btn.y, btn.w, btn.h, yellow);
                            }
                            match btn.action {
                                BrowserAction::OpenSetup => {
                                    let tw = app.font.measure_text("SETUP");
                                    fb.draw_text_shadowed(
                                        &app.font,
                                        "SETUP",
                                        btn.x + (btn.w - tw) / 2,
                                        btn.y + 4,
                                        black,
                                        white,
                                    );
                                }
                                BrowserAction::QuitGame => {
                                    if let Some(spr) = tbx.sprites.get(55) {
                                        let sx = btn.x + (btn.w - spr.width as i32) / 2;
                                        let sy = btn.y + (btn.h - spr.sub_h as i32) / 2;
                                        fb.blit_sprite_remapped(spr, sx, sy, 0, &cache.tbx_remap);
                                    } else {
                                        let tw = app.font.measure_text("QUIT");
                                        fb.draw_text_shadowed(
                                            &app.font,
                                            "QUIT",
                                            btn.x + (btn.w - tw) / 2,
                                            btn.y + 4,
                                            black,
                                            white,
                                        );
                                    }
                                }
                                BrowserAction::CyclePieces => {
                                    let count_idx = (2..=8)
                                        .contains(&app.selected_grid_dim)
                                        .then(|| 34 + app.selected_grid_dim - 2);
                                    if let Some(spr) = count_idx.and_then(|i| tbx.sprites.get(i)) {
                                        fb.blit_sprite_remapped(
                                            spr,
                                            btn.x + 4,
                                            btn.y + 2,
                                            0,
                                            &cache.tbx_remap,
                                        );
                                        fb.draw_text_shadowed(
                                            &app.font,
                                            "PCS",
                                            btn.x + 28,
                                            btn.y + 4,
                                            black,
                                            white,
                                        );
                                    } else {
                                        let tw = app.font.measure_text(&pcs_label);
                                        fb.draw_text_shadowed(
                                            &app.font,
                                            &pcs_label,
                                            btn.x + (btn.w - tw) / 2,
                                            btn.y + 4,
                                            black,
                                            white,
                                        );
                                    }
                                }
                                BrowserAction::ToggleEdges => {
                                    let tw = app.font.measure_text(edge_label);
                                    fb.draw_text_shadowed(
                                        &app.font,
                                        edge_label,
                                        btn.x + (btn.w - tw) / 2,
                                        btn.y + 4,
                                        black,
                                        white,
                                    );
                                }
                                _ => unreachable!(),
                            }
                        }
                    }
                }
                if let Some((ref status, _)) = app.browser_status {
                    fb.fill_rect(36, 140, 264, 16, black);
                    fb.draw_rect_outline(36, 140, 264, 16, white);
                    fb.draw_text(&app.font, status, 40, 144, white);
                }
            }
            AppScreen::Playing => {
                if let Some(ref sess) = app.session {
                    let toolbox = if sess.asset.uses_ega() {
                        &app.tbx_ega
                    } else {
                        &app.tbx
                    };
                    let (px, py) = if app.in_game_menu_open {
                        (-1, -1)
                    } else {
                        sess.active_pointer(mx, my)
                    };
                    puzzle_renderer.render(sess, fb, toolbox, &app.font, px, py);
                    if app.in_game_menu_open {
                        let (mpx, mpy) = sess.active_pointer(mx, my);
                        render_in_game_menu(
                            app,
                            sess,
                            fb,
                            toolbox,
                            puzzle_renderer.tbx_remap(),
                            mpx,
                            mpy,
                        );
                    }
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

fn render_in_game_menu(
    app: &GameApp,
    sess: &PuzzleSession,
    fb: &mut Framebuffer,
    tbx: &ToolboxAsset,
    tbx_remap: &[u8; 256],
    mx: i32,
    my: i32,
) {
    let black = fb.color_index([0, 0, 0]);
    let white = fb.color_index([255, 255, 255]);
    let gray = fb.color_index([48, 48, 48]);
    let card = fb.color_index([85, 85, 85]);
    let hover = fb.color_index([170, 170, 170]);
    let hi = fb.color_index([220, 220, 228]);
    let lo = fb.color_index([40, 40, 48]);
    let yellow = fb.color_index([255, 255, 85]);
    let disabled_fg = fb.color_index([128, 128, 128]);

    fb.fill_rect(
        IN_GAME_MENU_X,
        IN_GAME_MENU_Y,
        IN_GAME_MENU_W,
        IN_GAME_MENU_H,
        black,
    );
    crate::puzzle_render::draw_beveled_box(
        fb,
        IN_GAME_MENU_X + 1,
        IN_GAME_MENU_Y + 1,
        IN_GAME_MENU_W - 2,
        IN_GAME_MENU_H - 2,
        gray,
        hi,
        lo,
        false,
        false,
        yellow,
        white,
    );
    let title = "MENU";
    let title_w = app.font.measure_text(title);
    fb.draw_text_shadowed(
        &app.font,
        title,
        IN_GAME_MENU_X + (IN_GAME_MENU_W - title_w) / 2,
        IN_GAME_MENU_Y + 6,
        black,
        yellow,
    );

    let hovered_cmd = in_game_menu_command_at(mx, my);
    for (idx, cmd) in InGameMenuCommand::ALL.into_iter().enumerate() {
        let (rx, ry, rw, rh) = in_game_menu_row_rect(idx);
        let enabled = app.is_in_game_menu_command_enabled(cmd);
        let is_hovered = enabled && hovered_cmd == Some(cmd);
        let is_pressed = enabled && app.in_game_menu_pressed == Some(cmd);
        let is_selected = enabled && app.in_game_menu_selected == idx;
        let bg = if !enabled {
            gray
        } else if (is_hovered || is_selected) && !is_pressed {
            hover
        } else {
            card
        };
        crate::puzzle_render::draw_beveled_box(
            fb,
            rx,
            ry,
            rw,
            rh,
            bg,
            hi,
            lo,
            is_pressed,
            is_hovered || is_selected,
            yellow,
            white,
        );
        if let Some(spr_idx) = cmd.sprite_idx()
            && let Some(spr) = tbx.sprites.get(spr_idx)
        {
            fb.set_clip(rx + 2, ry + 1, 18, rh - 2);
            fb.blit_sprite_remapped(spr, rx + 2, ry + 1, 0, tbx_remap);
            fb.reset_clip();
        }
        let label = match cmd {
            InGameMenuCommand::OtherPicture => "Other Picture",
            InGameMenuCommand::Setup => "Setup (F4)",
            InGameMenuCommand::PieceTray => {
                if sess.held_piece.is_some() {
                    "Return Piece"
                } else if sess.toolbox_open {
                    "Hide Tray"
                } else {
                    "Show Tray"
                }
            }
            InGameMenuCommand::Replay => "Replay (R)",
            InGameMenuCommand::Quit => "Quit (Q)",
            InGameMenuCommand::Resume => "Resume (Esc)",
        };
        let text_x = rx + 24;
        let text_y = ry + 5;
        if enabled {
            fb.draw_text_shadowed(&app.font, label, text_x, text_y, black, white);
        } else {
            fb.draw_text(&app.font, label, text_x, text_y, disabled_fg);
        }
    }
}
