//! ECS components representing puzzle pieces, hotspots, hints, and animations.

use bevy_ecs::prelude::*;

use crate::assets::{SequencePlayback, Sprite8061};

/// Stable identity of a puzzle piece within the active puzzle.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PieceIdentity {
    pub index: usize,
    pub home_row: usize,
    pub home_col: usize,
}

/// Cut geometry and playfield target coordinates of a puzzle piece.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PieceGeometry {
    pub target_x: i32,
    pub target_y: i32,
    pub base_x: i32,
    pub base_y: i32,
    pub base_w: i32,
    pub base_h: i32,
}

/// Edge compatibility signatures for grid puzzle pieces (`grid_dim > 1`).
/// `None` denotes a flat outer border; `Some((variant, dir))` denotes an interior edge.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PieceEdges {
    pub left: Option<(usize, i32)>,
    pub right: Option<(usize, i32)>,
    pub top: Option<(usize, i32)>,
    pub bottom: Option<(usize, i32)>,
}

/// Cut piece bitmap sprite.
#[derive(Component, Clone)]
pub struct PieceSprite {
    pub sprite: Sprite8061,
}

/// Authoritative gameplay state of a puzzle piece.
///
/// By construction, a piece exists in exactly one of these three states:
/// in the tray, placed on the board, or currently held by the player.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum PieceLocation {
    /// In the left-hand tray at dense 0-based rank `order`.
    Tray { order: usize },
    /// Placed on the playfield at grid cell `(row, col)` (for preset puzzles: `row = 0, col = index`).
    Board { row: usize, col: usize },
    /// Currently held by the player (either as a single piece or part of a connected group).
    Held {
        is_anchor: bool,
        /// Grid-cell offset `(dr, dc)` relative to the anchor piece (`(0, 0)` for single/anchor).
        group_offset: (i32, i32),
        /// Deterministic BFS order within the held group (`0` for anchor).
        group_order: usize,
    },
}

impl PieceLocation {
    pub fn is_placed(self) -> bool {
        matches!(self, Self::Board { .. })
    }

    pub fn is_held(self) -> bool {
        matches!(self, Self::Held { .. })
    }

    pub fn is_tray(self) -> bool {
        matches!(self, Self::Tray { .. })
    }
}

/// Marker/timer component attached to the currently hinted piece entity (`F1`).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct HintHighlight {
    pub timer: bevy_time::Timer,
}

impl HintHighlight {
    pub fn new(secs: f32) -> Self {
        Self {
            timer: bevy_time::Timer::from_seconds(secs, bevy_time::TimerMode::Once),
        }
    }
}

/// Interactive hotspot entity in a solved puzzle's `FILM` scene.
#[derive(Component, Clone, Copy, Debug)]
pub struct SceneHotspot {
    pub index: usize,
    pub x: i16,
    pub y: i16,
    pub width: i16,
    pub height: i16,
    pub frame_idx: usize,
    pub dx: i16,
    pub dy: i16,
    pub flags: u16,
    pub playback: SequencePlayback,
}

/// Active completion/hotspot animation instance (`FILM`, `F_ALL`, `F_MICKEY`, etc.).
#[derive(Component, Clone, Debug)]
pub struct AnimationState {
    pub seq_idx: usize,
    pub start_frame: usize,
    pub previous_frame: usize,
    pub cur_frame: usize,
    pub end_frame: usize,
    pub tick_accum: f32,
    pub frame_delay_secs: f32,
    pub repetitions_left: u16,
    pub default_delay_secs: f32,
    pub transition_mode: u8,
    pub bubble: Option<(usize, usize)>,
    pub wait_for_audio: bool,
    pub looping_sound: Option<(usize, usize)>,
}
