//! ECS resources representing global application, browser, setup, menu, and active puzzle metadata.

use std::collections::HashSet;
use std::path::PathBuf;

use bevy_ecs::prelude::*;
use bevy_time::{Stopwatch, Timer, TimerMode};

use crate::app::{AppInput, BrowserTarget, InGameMenuCommand};
use crate::assets::{FontAsset, Preferences, PuzzleAsset, ToolboxAsset};
use crate::decompress::IlbmImage;
use crate::graphics::Framebuffer;
use crate::puzzle::{LcgRng, PLAYFIELD_H, PLAYFIELD_W, TOOLBOX_W, ToolboxTarget, grid_col_widths, grid_row_heights};
use crate::setup::SetupMenu;

/// Global configuration and user preferences.
#[derive(Resource, Debug, Clone)]
pub struct GameConfig {
    pub game_dir: PathBuf,
    pub prefs: Preferences,
    pub session_use_ega: bool,
}

/// Immutable static game assets loaded once at startup.
#[derive(Resource)]
pub struct StaticAssets {
    pub font: FontAsset,
    pub tbx: ToolboxAsset,
    pub tbx_ega: ToolboxAsset,
    pub title_images: Vec<IlbmImage>,
    pub credit_images: Vec<IlbmImage>,
}

/// Screen transition timer, quit lifecycle, and error banner.
/// The active screen itself lives authoritatively in `State<AppScreen>`.
#[derive(Resource, Debug, Clone, Default)]
pub struct ScreenState {
    pub screen_timer: Stopwatch,
    pub quit_requested: bool,
    pub quit_timer: Stopwatch,
    pub error: Option<String>,
}

/// Puzzle Browser screen state.
#[derive(Resource)]
pub struct BrowserState {
    pub selected_puzzle: usize,
    pub preview_asset: PuzzleAsset,
    pub selected_grid_dim: usize,
    pub curved_edges: bool,
    pub status: Option<(String, Timer)>,
    pub hover: Option<BrowserTarget>,
    pub pressed: Option<BrowserTarget>,
}

impl BrowserState {
    pub fn set_status(&mut self, msg: impl Into<String>, secs: f32) {
        self.status = Some((msg.into(), Timer::from_seconds(secs, TimerMode::Once)));
    }
}

/// DOS Setup screen state.
#[derive(Resource, Debug, Clone)]
pub struct SetupState {
    pub menu: SetupMenu,
}

/// In-game overlay menu and confirmation dialog state.
#[derive(Resource, Debug, Clone, Default)]
pub struct InGameMenuState {
    pub open: bool,
    pub selected: usize,
    pub pressed: Option<InGameMenuCommand>,
    pub confirm_exit_puzzle: bool,
    pub confirm_quit_puzzle: bool,
}

/// Session-wide metadata for the currently loaded puzzle (if any).
///
/// Piece state (tray/board/held) lives authoritatively in `PieceLocation` components
/// on piece entities rather than inside this resource.
#[derive(Clone)]
pub struct PuzzleMeta {
    pub session_id: u64,
    pub puzzle_num: usize,
    pub asset: PuzzleAsset,
    pub asset_rev: u64,
    pub grid_dim: usize,
    pub curved_edges: bool,
    pub toolbox_open: bool,
    pub grid_lines: bool,
    pub timer_enabled: bool,
    pub text_enabled: bool,
    pub elapsed_secs: f64,
    pub tray_scroll: usize,
    pub keyboard_pointer: Option<(f32, f32)>,
    pub drag_origin: Option<(i32, i32)>,
    pub held_group_origin_cell: Option<(usize, usize)>,
    pub pressed_toolbox_target: Option<ToolboxTarget>,
    pub solved: bool,
    pub pending_completion_feedback: bool,
    pub voice_rng: LcgRng,
    pub status_message: Option<(String, Timer)>,
    pub full_picture_fb: Box<Framebuffer>,
    pub grid_polylines_v: Vec<Vec<(i32, i32)>>,
    pub grid_polylines_h: Vec<Vec<(i32, i32)>>,
}

impl PuzzleMeta {
    /// Playfield X offset on screen (`48` when toolbox is open on the left, `25` when hidden).
    pub fn playfield_x(&self) -> i32 {
        if self.toolbox_open && !self.solved {
            48
        } else {
            25
        }
    }

    pub fn playfield_y(&self) -> i32 {
        2
    }

    pub fn visible_tray_slots(&self) -> usize {
        match self.grid_dim {
            2 => 2,
            3 => 3,
            4 | 5 => 4,
            _ => 5,
        }
    }

    pub fn tray_slot_height(&self) -> i32 {
        (156 / self.visible_tray_slots().max(1)) as i32
    }

    pub fn clamp_tray_scroll(&mut self, tray_len: usize) {
        let max_scroll = tray_len.saturating_sub(self.visible_tray_slots());
        self.tray_scroll = self.tray_scroll.min(max_scroll);
    }

    pub fn active_pointer(&self, mouse_x: i32, mouse_y: i32) -> (i32, i32) {
        self.keyboard_pointer
            .map(|(x, y)| (x.round() as i32, y.round() as i32))
            .unwrap_or((mouse_x, mouse_y))
    }

    pub fn status_message(&self) -> Option<&str> {
        self.status_message.as_ref().map(|(s, _)| s.as_str())
    }

    pub fn set_status_message(&mut self, msg: impl Into<String>, secs: f32) {
        self.status_message = Some((msg.into(), Timer::from_seconds(secs, TimerMode::Once)));
    }

    /// Formats `elapsed_secs` as `H:MM:SS` (`0:00:00`, `seg_0b43:0cf8`).
    pub fn formatted_time(&self) -> String {
        let total = self.elapsed_secs as u64;
        let hrs = total / 3600;
        let mins = (total % 3600) / 60;
        let secs = total % 60;
        format!("{}:{:02}:{:02}", hrs, mins, secs)
    }

    pub fn grid_cell_at(&self, px: i32, py: i32) -> Option<(usize, usize)> {
        if self.grid_dim <= 1 || !(0..PLAYFIELD_W).contains(&px) || !(0..PLAYFIELD_H).contains(&py)
        {
            return None;
        }
        let dim = self.grid_dim;
        let col_w = grid_col_widths(dim);
        let row_h = grid_row_heights(dim);
        let mut x = 0;
        let mut found_c = None;
        for (c, &w) in col_w.iter().enumerate() {
            if px >= x && px < x + w {
                found_c = Some(c);
                break;
            }
            x += w;
        }
        let mut y = 0;
        let mut found_r = None;
        for (r, &h) in row_h.iter().enumerate() {
            if py >= y && py < y + h {
                found_r = Some(r);
                break;
            }
            y += h;
        }
        Some((found_r?, found_c?))
    }

    pub fn toolbox_target_at(
        &self,
        mouse_x: i32,
        mouse_y: i32,
        has_active_anim: bool,
        tray_len: usize,
    ) -> Option<ToolboxTarget> {
        if self.solved || has_active_anim {
            return None;
        }
        if !self.toolbox_open {
            return ((0..18).contains(&mouse_x) && (0..16).contains(&mouse_y))
                .then_some(ToolboxTarget::ClosedTab);
        }
        if !(0..TOOLBOX_W).contains(&mouse_x) || !(0..200).contains(&mouse_y) {
            return None;
        }
        if mouse_y < 18 {
            return Some(ToolboxTarget::Menu);
        }
        if mouse_y < 30 {
            return Some(ToolboxTarget::ScrollUp);
        }
        if mouse_y >= 186 {
            return Some(ToolboxTarget::ScrollDown);
        }
        let slot_h = self.tray_slot_height();
        let slot = ((mouse_y - 30) / slot_h.max(1)) as usize;
        let vis = self.visible_tray_slots();
        if slot < vis && self.tray_scroll + slot < tray_len {
            Some(ToolboxTarget::TraySlot(slot))
        } else {
            None
        }
    }
}

/// Active puzzle resource (`None` before a puzzle is started).
#[derive(Resource, Default)]
pub struct ActivePuzzle(pub Option<PuzzleMeta>);

/// Per-frame platform input snapshot fed into the ECS schedule.
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct InputSnapshot {
    pub input: AppInput,
    pub dt: f32,
    pub consumed_global: bool,
}

/// Read-only audio status and asset availability synced from `AudioEngine`.
#[derive(Resource, Default, Debug, Clone)]
pub struct AudioStatus {
    pub is_playing: bool,
    pub is_effect_playing: bool,
    pub common_caption: Option<String>,
    pub caption_key: Option<u64>,
    pub sfx_enabled: bool,
    pub music_enabled: bool,
    pub available_nnv: HashSet<String>,
    pub available_iss_stems: HashSet<String>,
}

/// Monotonic render revision counters maintained automatically by ECS change-tracking.
#[derive(Resource, Debug, Clone, Copy)]
pub struct RenderVersions {
    pub app_rev: u64,
    pub board_rev: u64,
    pub tray_rev: u64,
    pub visual_rev: u64,
    pub last_hint_phase: Option<(Entity, i32)>,
    pub last_anim_vis: Option<(usize, Option<(usize, usize)>)>,
    pub last_tray_scroll: Option<usize>,
    pub last_toolbox_open: Option<bool>,
    pub last_grid_lines: Option<bool>,
    pub last_solved: Option<bool>,
    pub last_session_id: Option<u64>,
}

impl Default for RenderVersions {
    fn default() -> Self {
        Self {
            app_rev: 1,
            board_rev: 1,
            tray_rev: 1,
            visual_rev: 1,
            last_hint_phase: None,
            last_anim_vis: None,
            last_tray_scroll: None,
            last_toolbox_open: None,
            last_grid_lines: None,
            last_solved: None,
            last_session_id: None,
        }
    }
}
