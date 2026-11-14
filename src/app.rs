//! ECS application host, UI layout definitions, and software presentation layer.
//!
//! Kept independent of windowing/GPU backends so all screen transitions, input handling,
//! ECS systems, and framebuffer rendering can be tested headlessly.

use std::path::PathBuf;
use std::time::Duration;

use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use bevy_state::prelude::{State, States};
use bevy_time::TimeUpdateStrategy;

use crate::assets::{FontAsset, Preferences, PuzzleAsset, ToolboxAsset, load_ilbm_container};
use crate::audio::AudioEngine;
use crate::ecs::components::{AnimationState, PieceLocation};
use crate::ecs::plugins::{
    AudioSyncPlugin, BrowserPlugin, CoreGamePlugin, PuzzlePlugin, RenderTrackingPlugin,
    SetupPlugin, sync_audio_status,
};
use crate::ecs::resources::{
    ActivePuzzle, AudioStatus, BrowserState, GameConfig, InGameMenuState, InputSnapshot,
    RenderVersions, ScreenState, SetupState, StaticAssets,
};
use crate::ecs::systems::browser::can_resume_selected;
use crate::ecs::systems::menu::is_menu_command_enabled;
use crate::graphics::{
    Framebuffer, SCREEN_H, SCREEN_W, copy_scene_interior, render_scene_view_with_scratch,
};
use crate::puzzle::PuzzlePresentKey;
use crate::puzzle_render::{PuzzleRenderer, draw_playfield_border_remapped, puzzle_present_key};
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

#[derive(States, Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum AppScreen {
    #[default]
    Title,
    Credits {
        page: usize,
    },
    Setup {
        return_to_playing: bool,
    },
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

/// Builds the modular Bevy [`App`] for the game, loading DOS assets from `game_dir`.
pub fn build_game_app(game_dir: PathBuf, start_in_setup: bool) -> Result<App, String> {
    let font = FontAsset::load(&game_dir.join("MJP.NFT"))?;
    let tbx = ToolboxAsset::load(&game_dir.join("MJP.TBX"), 0)?;
    let tbx_ega = ToolboxAsset::load(&game_dir.join("MJP.TBX"), 1)?;
    let title_images = load_ilbm_container(&game_dir.join("MJP.TTL"))?;
    let mut credit_images = load_ilbm_container(&game_dir.join("MJP.CRD")).unwrap_or_else(|err| {
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
    let initial_screen = if start_in_setup {
        AppScreen::Setup {
            return_to_playing: false,
        }
    } else {
        audio.play_iss("TITLE", 0);
        AppScreen::Title
    };

    Ok(assemble_app(
        initial_screen,
        GameConfig {
            game_dir,
            prefs,
            session_use_ega: use_ega,
        },
        StaticAssets {
            font,
            tbx,
            tbx_ega,
            title_images,
            credit_images,
        },
        BrowserState {
            selected_puzzle,
            preview_asset,
            selected_grid_dim: 3,
            curved_edges: true,
            status: None,
            hover: None,
            pressed: None,
        },
        SetupState { menu: setup_menu },
        audio,
    ))
}

/// Builds a headless [`App`] from pre-loaded or synthetic parts (used by tests and benchmarks).
pub fn build_game_app_from_parts(
    game_dir: PathBuf,
    font: FontAsset,
    tbx: ToolboxAsset,
    tbx_ega: ToolboxAsset,
    preview_asset: PuzzleAsset,
    prefs: Preferences,
    start_in_setup: bool,
) -> App {
    let use_ega = prefs.video_mode >= 2;
    let audio = AudioEngine::new(&game_dir, false, false);
    let setup_menu = SetupMenu::new(prefs);
    let initial_screen = if start_in_setup {
        AppScreen::Setup {
            return_to_playing: false,
        }
    } else {
        AppScreen::Browser
    };
    assemble_app(
        initial_screen,
        GameConfig {
            game_dir,
            prefs,
            session_use_ega: use_ega,
        },
        StaticAssets {
            font,
            tbx,
            tbx_ega,
            title_images: Vec::new(),
            credit_images: Vec::new(),
        },
        BrowserState {
            selected_puzzle: preview_asset.id,
            preview_asset,
            selected_grid_dim: 3,
            curved_edges: true,
            status: None,
            hover: None,
            pressed: None,
        },
        SetupState { menu: setup_menu },
        audio,
    )
}

fn assemble_app(
    initial_screen: AppScreen,
    config: GameConfig,
    assets: StaticAssets,
    browser: BrowserState,
    setup: SetupState,
    audio: AudioEngine,
) -> App {
    let mut audio_status = AudioStatus::default();
    sync_audio_status(&audio, &mut audio_status);

    let mut app = App::new();
    app.add_plugins((
        CoreGamePlugin { initial_screen },
        AudioSyncPlugin,
        BrowserPlugin,
        SetupPlugin,
        PuzzlePlugin,
        RenderTrackingPlugin,
    ));
    app.insert_resource(config)
        .insert_resource(assets)
        .insert_resource(browser)
        .insert_resource(setup)
        .insert_resource(audio_status)
        .insert_non_send(audio);
    app.finish();
    app.cleanup();
    app
}

/// Feeds one frame of [`AppInput`] and deterministic delta time into `bevy_time` and runs `App::update()`.
pub fn step_app(app: &mut App, input: &AppInput) {
    let dt = if input.dt.is_finite() {
        input.dt.max(0.0)
    } else {
        0.0
    };
    {
        let world = app.world_mut();
        *world.resource_mut::<TimeUpdateStrategy>() =
            TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(dt));
        let mut snap = world.resource_mut::<InputSnapshot>();
        snap.input = *input;
        snap.dt = dt;
        snap.consumed_global = false;
    }
    app.update();
}

pub fn app_screen(app: &App) -> AppScreen {
    *app.world().resource::<State<AppScreen>>().get()
}

pub fn should_exit(app: &App) -> bool {
    let s = app.world().resource::<ScreenState>();
    let is_effect_playing = app.world().resource::<AudioStatus>().is_effect_playing;
    s.quit_requested && (!is_effect_playing || s.quit_timer.elapsed_secs() >= 3.0)
}

pub fn has_mickey_cursor(app: &App) -> bool {
    app.world().resource::<StaticAssets>().tbx.sprites.len() > 64
}

fn world_caption_key(world: &World) -> Option<u64> {
    if let Some(audio) = world.get_non_send::<AudioEngine>() {
        audio.caption_key()
    } else {
        world
            .get_resource::<AudioStatus>()
            .and_then(|s| s.caption_key)
    }
}

fn world_common_caption(world: &World) -> Option<String> {
    if let Some(audio) = world.get_non_send::<AudioEngine>() {
        audio.common_caption().map(str::to_owned)
    } else {
        world
            .get_resource::<AudioStatus>()
            .and_then(|s| s.common_caption.clone())
    }
}

pub fn present_signature(
    world: &mut World,
    mx: i32,
    my: i32,
    show_reference: bool,
    picture_guide: bool,
) -> Option<PresentSignature> {
    let text_msg_on = world.resource::<GameConfig>().prefs.text_msg_on != 0;
    let caption_key = text_msg_on.then(|| world_caption_key(world)).flatten();
    let screen = *world.resource::<State<AppScreen>>().get();
    let app_rev = world.resource::<RenderVersions>().app_rev;

    Some(match screen {
        AppScreen::Title => PresentSignature::Title,
        AppScreen::Credits { page } => PresentSignature::Credits(page),
        AppScreen::Setup { .. } => PresentSignature::Setup {
            app_rev,
            setup_rev: world.resource::<SetupState>().menu.revision(),
        },
        AppScreen::Browser => {
            let b = world.resource::<BrowserState>();
            PresentSignature::Browser {
                app_rev,
                caption_key,
                hover: b.hover,
                pressed: b.pressed,
            }
        }
        AppScreen::Playing => {
            let (session_id, asset_rev, solved, pointer) = {
                let meta = world.resource::<ActivePuzzle>().0.as_ref()?;
                (
                    meta.session_id,
                    meta.asset_rev,
                    meta.solved,
                    meta.active_pointer(mx, my),
                )
            };
            if show_reference {
                PresentSignature::Reference {
                    session_id,
                    asset_rev,
                }
            } else {
                let menu = world.resource::<InGameMenuState>().clone();
                let menu_state = menu.open.then(|| {
                    let hovered = in_game_menu_command_at(pointer.0, pointer.1)
                        .filter(|&cmd| is_menu_command_enabled(cmd, solved));
                    (menu.selected, hovered, menu.pressed)
                });
                let (board_mx, board_my) = if menu.open { (-1, -1) } else { (mx, my) };
                let puzzle = puzzle_present_key(world, board_mx, board_my, picture_guide)?;
                PresentSignature::Playing {
                    app_rev,
                    puzzle,
                    caption_key,
                    menu_state,
                }
            }
        }
    })
}

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
        world: &mut World,
        mx: i32,
        my: i32,
        show_reference: bool,
    ) -> Option<(usize, i32, i32)> {
        if show_reference {
            return None;
        }
        let screen = *world.resource::<State<AppScreen>>().get();
        let (px, py) = if screen == AppScreen::Playing {
            world
                .resource::<ActivePuzzle>()
                .0
                .as_ref()
                .map_or((mx, my), |m| m.active_pointer(mx, my))
        } else {
            (mx, my)
        };
        if !(0..SCREEN_W).contains(&px) || !(0..SCREEN_H).contains(&py) {
            return None;
        }

        let sprite_idx = match screen {
            AppScreen::Playing => {
                let menu = world.resource::<InGameMenuState>().clone();
                if menu.confirm_exit_puzzle || menu.confirm_quit_puzzle {
                    62
                } else if menu.pressed.is_some() {
                    63
                } else if menu.open {
                    62
                } else {
                    let has_anim = world
                        .query::<&AnimationState>()
                        .iter(world)
                        .next()
                        .is_some();
                    let is_holding = world
                        .query::<&PieceLocation>()
                        .iter(world)
                        .any(|loc| loc.is_held());
                    let tray_len = world
                        .query::<&PieceLocation>()
                        .iter(world)
                        .filter(|loc| loc.is_tray())
                        .count();
                    let meta = world.resource::<ActivePuzzle>().0.as_ref();
                    match meta {
                        Some(m) if is_holding || m.pressed_toolbox_target.is_some() => 63,
                        Some(m)
                            if m.toolbox_target_at(px, py, has_anim, tray_len).is_some()
                                || (m.toolbox_open && px < 46)
                                || m.solved =>
                        {
                            62
                        }
                        Some(_) => 64,
                        None => 62,
                    }
                }
            }
            AppScreen::Browser if world.resource::<BrowserState>().pressed.is_some() => 63,
            _ => 62,
        };
        world
            .resource::<StaticAssets>()
            .tbx
            .sprites
            .get(sprite_idx)?;
        let (hx, hy) = Self::cursor_hotspot(sprite_idx);
        Some((sprite_idx, px + hx, py + hy))
    }

    fn blit_cursor_rgba(&mut self, world: &World, sprite_idx: usize, px: i32, py: i32) {
        let tbx = &world.resource::<StaticAssets>().tbx;
        let Some(sprite) = tbx.sprites.get(sprite_idx) else {
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
                let rgb = tbx.palette[sprite.pixels[idx] as usize];
                let dst = ((dy as usize * SCREEN_W as usize) + dx as usize) * 4;
                self.rgba_buf[dst] = rgb[0];
                self.rgba_buf[dst + 1] = rgb[1];
                self.rgba_buf[dst + 2] = rgb[2];
                self.rgba_buf[dst + 3] = 255;
            }
        }
    }

    /// Renders a cursor sprite (`62`, `63`, `64`) into `rgba_out` with transparent background.
    pub fn render_cursor_rgba(
        world: &World,
        sprite_idx: usize,
        rgba_out: &mut Vec<u8>,
    ) -> Option<(i32, i32)> {
        let tbx = &world.resource::<StaticAssets>().tbx;
        let sprite = tbx.sprites.get(sprite_idx)?;
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
                let rgb = tbx.palette[sprite.pixels[idx] as usize];
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
        world: &mut World,
        mx: i32,
        my: i32,
        show_reference: bool,
        picture_guide: bool,
    ) -> bool {
        let sig = present_signature(world, mx, my, show_reference, picture_guide);
        let base_dirty = sig.is_none() || sig != self.last_present_sig;
        if !base_dirty {
            return false;
        }
        let has_active = world.resource::<ActivePuzzle>().0.is_some();
        if show_reference && has_active {
            self.puzzle_renderer.render_reference(world, &mut self.fb);
        } else {
            self.render(world, mx, my);
        }

        self.fb.to_rgba8(&mut self.base_rgba_buf);
        let screen = *world.resource::<State<AppScreen>>().get();
        if picture_guide && !show_reference && screen == AppScreen::Playing {
            let pointer = world
                .resource::<ActivePuzzle>()
                .0
                .as_ref()
                .map(|m| m.active_pointer(mx, my));
            if let Some((px, py)) = pointer {
                self.puzzle_renderer
                    .blend_picture_guide(world, &mut self.base_rgba_buf, px, py);
            }
        }
        self.last_present_sig = sig;
        true
    }

    pub fn render_base_only(
        &mut self,
        world: &mut World,
        mx: i32,
        my: i32,
        show_reference: bool,
        picture_guide: bool,
    ) -> bool {
        self.update_base_rgba(world, mx, my, show_reference, picture_guide)
    }

    pub fn base_rgba(&self) -> &[u8] {
        &self.base_rgba_buf
    }

    /// Renders interactive-board overlays (`status_message`, `caption`, `error`) into a
    /// `320x200x4` transparent RGBA8 buffer. Returns `false` if no overlay is active.
    pub fn render_playing_overlay_rgba(&mut self, world: &World, rgba_out: &mut [u8]) -> bool {
        assert_eq!(rgba_out.len(), (SCREEN_W * SCREEN_H * 4) as usize);
        let Some(meta) = world.resource::<ActivePuzzle>().0.as_ref() else {
            return false;
        };
        let status = meta.status_message();
        let text_msg_on = world.resource::<GameConfig>().prefs.text_msg_on != 0;
        let caption = text_msg_on.then(|| world_common_caption(world)).flatten();
        let has_error = world.resource::<ScreenState>().error.is_some();
        if status.is_none() && caption.is_none() && !has_error {
            return false;
        }

        let font = &world.resource::<StaticAssets>().font;
        self.scratch_fb.reset_clip();
        self.scratch_fb.palette = meta.asset.palette;
        let black = self.scratch_fb.color_index([0; 3]);
        let white = self.scratch_fb.color_index([255; 3]);
        let yellow = self.scratch_fb.color_index([255, 255, 85]);
        let sentinel = (0u8..=255)
            .find(|&c| c != black && c != white && c != yellow)
            .unwrap_or(255);
        self.scratch_fb.clear(sentinel);

        if let Some(status) = status {
            self.scratch_fb.draw_text_shadowed(
                font,
                status,
                meta.playfield_x() + 6,
                186,
                black,
                yellow,
            );
        }
        if let Some(caption) = caption {
            let line_count = caption.split('\r').count() as i32;
            let height = line_count * font.height as i32 + 8;
            let y = 164 - height;
            self.scratch_fb.fill_rect(4, y, SCREEN_W - 8, height, white);
            self.scratch_fb
                .draw_rect_outline(4, y, SCREEN_W - 8, height, black);
            for (row, line) in caption.split('\r').enumerate() {
                let x = (SCREEN_W - font.measure_text(line)) / 2;
                self.scratch_fb.draw_text(
                    font,
                    line,
                    x,
                    y + 4 + row as i32 * font.height as i32,
                    black,
                );
            }
        }
        if has_error {
            self.scratch_fb.fill_rect(0, 20, SCREEN_W, 26, black);
            self.scratch_fb
                .draw_text(font, "Asset load failed (see terminal)", 5, 23, white);
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
    pub fn render_if_needed(
        &mut self,
        world: &mut World,
        mx: i32,
        my: i32,
        show_reference: bool,
        picture_guide: bool,
    ) -> bool {
        let cursor_sig = Self::desired_cursor(world, mx, my, show_reference);
        let base_dirty = self.update_base_rgba(world, mx, my, show_reference, picture_guide);
        if !base_dirty && cursor_sig == self.last_cursor_sig {
            return false;
        }
        self.rgba_buf.copy_from_slice(&self.base_rgba_buf);
        if let Some((sprite_idx, px, py)) = cursor_sig {
            self.blit_cursor_rgba(world, sprite_idx, px, py);
        }
        self.last_cursor_sig = cursor_sig;
        true
    }

    pub fn render(&mut self, world: &mut World, mx: i32, my: i32) {
        let Self {
            fb,
            scratch_fb,
            preview_cache,
            puzzle_renderer,
            ..
        } = self;
        fb.reset_clip();
        let screen = *world.resource::<State<AppScreen>>().get();

        match screen {
            AppScreen::Title => {
                let assets = world.resource::<StaticAssets>();
                if let Some(img) = assets.title_images.first() {
                    fb.draw_ilbm(img);
                }
            }
            AppScreen::Credits { page } => {
                let assets = world.resource::<StaticAssets>();
                if let Some(img) = assets
                    .credit_images
                    .get(page)
                    .or_else(|| assets.credit_images.first())
                {
                    fb.draw_ilbm(img);
                }
            }
            AppScreen::Setup { .. } => {
                fb.reset_default_palette();
                let font = &world.resource::<StaticAssets>().font;
                let setup = &world.resource::<SetupState>().menu;
                setup.render(fb, font);
            }
            AppScreen::Browser => {
                let browser = world.resource::<BrowserState>();
                let assets = world.resource::<StaticAssets>();
                let active_puzzle = world.resource::<ActivePuzzle>();
                let config = world.resource::<GameConfig>();
                let font = &assets.font;
                let key = (browser.preview_asset.id, browser.preview_asset.uses_ega());
                let tbx = if key.1 { &assets.tbx_ega } else { &assets.tbx };
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
                        &browser.preview_asset,
                        browser.preview_asset.static_frame_idx(),
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
                let can_resume = can_resume_selected(active_puzzle, browser, config);

                for i in 0..BROWSER_SLOT_COUNT {
                    let puzzle_id = i + 1;
                    let y = BROWSER_SLOT_Y0 + (i as i32) * BROWSER_SLOT_H;
                    let slot_target = BrowserTarget::PuzzleSlot(puzzle_id);
                    let is_selected = browser.selected_puzzle == puzzle_id;
                    let is_hovered = browser.hover == Some(slot_target);
                    let is_pressed = browser.pressed == Some(slot_target);
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
                        fb.draw_text(font, &num, BROWSER_RIGHT_BAR_X + 2, y + 2, white);
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

                fb.fill_rect(36, 4, 176, 15, black);
                fb.draw_rect_outline(36, 4, 176, 15, white);
                let title = if can_resume {
                    format!("Puzzle {:02} of 15  R:reset", browser.selected_puzzle)
                } else {
                    format!("Puzzle {:02} of 15", browser.selected_puzzle)
                };
                fb.draw_text_shadowed(font, &title, 42, 7, black, white);

                let pcs_label = if browser.selected_grid_dim == 1 {
                    format!("{} PC", browser.preview_asset.preset_piece_count())
                } else {
                    format!(
                        "{} PCS",
                        browser.selected_grid_dim * browser.selected_grid_dim
                    )
                };
                let edge_label = if browser.selected_grid_dim == 1 {
                    "ORIG"
                } else if browser.curved_edges {
                    "CURVED"
                } else {
                    "SQUARE"
                };

                for btn in BROWSER_BUTTONS {
                    let target = BrowserTarget::Button(btn.action);
                    let is_hovered = browser.hover == Some(target);
                    let is_pressed = browser.pressed == Some(target);
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
                                let tw = font.measure_text(icon_spec.3);
                                fb.draw_text_shadowed(
                                    font,
                                    icon_spec.3,
                                    btn.x + (btn.w - tw) / 2,
                                    btn.y + 12,
                                    black,
                                    white,
                                );
                            }
                            if btn.action == BrowserAction::StartOrResume && can_resume {
                                let tw = font.measure_text("RES");
                                fb.draw_text_shadowed(
                                    font,
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
                                    let tw = font.measure_text("SETUP");
                                    fb.draw_text_shadowed(
                                        font,
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
                                        let tw = font.measure_text("QUIT");
                                        fb.draw_text_shadowed(
                                            font,
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
                                        .contains(&browser.selected_grid_dim)
                                        .then(|| 34 + browser.selected_grid_dim - 2);
                                    if let Some(spr) = count_idx.and_then(|i| tbx.sprites.get(i)) {
                                        fb.blit_sprite_remapped(
                                            spr,
                                            btn.x + 4,
                                            btn.y + 2,
                                            0,
                                            &cache.tbx_remap,
                                        );
                                        fb.draw_text_shadowed(
                                            font,
                                            "PCS",
                                            btn.x + 28,
                                            btn.y + 4,
                                            black,
                                            white,
                                        );
                                    } else {
                                        let tw = font.measure_text(&pcs_label);
                                        fb.draw_text_shadowed(
                                            font,
                                            &pcs_label,
                                            btn.x + (btn.w - tw) / 2,
                                            btn.y + 4,
                                            black,
                                            white,
                                        );
                                    }
                                }
                                BrowserAction::ToggleEdges => {
                                    let tw = font.measure_text(edge_label);
                                    fb.draw_text_shadowed(
                                        font,
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
                if let Some((ref status, _)) = browser.status {
                    fb.fill_rect(36, 140, 264, 16, black);
                    fb.draw_rect_outline(36, 140, 264, 16, white);
                    fb.draw_text(font, status, 40, 144, white);
                }
            }
            AppScreen::Playing => {
                let menu = world.resource::<InGameMenuState>().clone();
                let (has_meta, uses_ega, pointer) = {
                    let meta_opt = world.resource::<ActivePuzzle>().0.as_ref();
                    match meta_opt {
                        Some(m) => (true, m.asset.uses_ega(), m.active_pointer(mx, my)),
                        None => (false, false, (mx, my)),
                    }
                };
                if has_meta {
                    let assets = world.resource::<StaticAssets>();
                    let toolbox = if uses_ega {
                        assets.tbx_ega.clone()
                    } else {
                        assets.tbx.clone()
                    };
                    let font = assets.font.clone();
                    let (px, py) = if menu.open { (-1, -1) } else { pointer };
                    puzzle_renderer.render(world, fb, &toolbox, &font, px, py);
                    if menu.open {
                        let is_holding = world
                            .query::<&PieceLocation>()
                            .iter(world)
                            .any(|loc| loc.is_held());
                        let meta = world.resource::<ActivePuzzle>().0.as_ref().unwrap();
                        render_in_game_menu(
                            &menu,
                            meta.solved,
                            meta.toolbox_open,
                            is_holding,
                            &font,
                            fb,
                            &toolbox,
                            puzzle_renderer.tbx_remap(),
                            pointer.0,
                            pointer.1,
                        );
                    }
                }
            }
        }

        let text_msg_on = world.resource::<GameConfig>().prefs.text_msg_on != 0;
        let caption = (text_msg_on && matches!(screen, AppScreen::Browser | AppScreen::Playing))
            .then(|| world_common_caption(world))
            .flatten();
        let kbd_pointer = (screen == AppScreen::Playing)
            .then(|| {
                world
                    .resource::<ActivePuzzle>()
                    .0
                    .as_ref()
                    .and_then(|m| m.keyboard_pointer)
            })
            .flatten();
        let has_error = world.resource::<ScreenState>().error.is_some();
        if caption.is_some() || kbd_pointer.is_some() || has_error {
            let font = &world.resource::<StaticAssets>().font;
            let black = fb.color_index([0; 3]);
            let white = fb.color_index([255; 3]);
            if let Some(caption) = caption {
                let line_count = caption.split('\r').count() as i32;
                let height = line_count * font.height as i32 + 8;
                let y = 164 - height;
                fb.fill_rect(4, y, SCREEN_W - 8, height, white);
                fb.draw_rect_outline(4, y, SCREEN_W - 8, height, black);
                for (row, line) in caption.split('\r').enumerate() {
                    let x = (SCREEN_W - font.measure_text(line)) / 2;
                    fb.draw_text(
                        font,
                        line,
                        x,
                        y + 4 + row as i32 * font.height as i32,
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
            if has_error {
                fb.fill_rect(0, 20, SCREEN_W, 26, black);
                fb.draw_text(font, "Asset load failed (see terminal)", 5, 23, white);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn render_in_game_menu(
    menu: &InGameMenuState,
    solved: bool,
    toolbox_open: bool,
    is_holding: bool,
    font: &FontAsset,
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
    let title_w = font.measure_text(title);
    fb.draw_text_shadowed(
        font,
        title,
        IN_GAME_MENU_X + (IN_GAME_MENU_W - title_w) / 2,
        IN_GAME_MENU_Y + 6,
        black,
        yellow,
    );

    let hovered_cmd = in_game_menu_command_at(mx, my);
    for (idx, cmd) in InGameMenuCommand::ALL.into_iter().enumerate() {
        let (rx, ry, rw, rh) = in_game_menu_row_rect(idx);
        let enabled = is_menu_command_enabled(cmd, solved);
        let is_hovered = enabled && hovered_cmd == Some(cmd);
        let is_pressed = enabled && menu.pressed == Some(cmd);
        let is_selected = enabled && menu.selected == idx;
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
                if is_holding {
                    "Return Piece"
                } else if toolbox_open {
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
            fb.draw_text_shadowed(font, label, text_x, text_y, black, white);
        } else {
            fb.draw_text(font, label, text_x, text_y, disabled_fg);
        }
    }
}
