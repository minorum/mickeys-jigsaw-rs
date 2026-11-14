//! Jigsaw Puzzle Engine (`seg_0e59`, `seg_0b43`):
//! - Exact 7 difficulty grids (`2x2=4` up to `8x8=64` pieces) over the `270 x 196` playfield
//! - Original interlocking tab/blank polygon curves (`DS:0x4B86` & `DS:0x4B94`) + straight-edge mode
//! - Piece cutting, tray/toolbox pagination, drag-and-drop, snapping, F1 hint, F2 auto-solve
//! - Completion & interactive hotspot animations (`FILM`, `F_ALL`, `F_MICKEY`, etc.)
//! - `.PRT` monochrome line-art export

use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::assets::{
    FrameCommand, PrintAsset, PuzzleAsset, SceneRecord, SequencePlayback, Sprite8061,
};
use crate::audio::{AudioEngine, CommonSound};
use crate::graphics::{
    Framebuffer, SCENE_OFFSET_X, compute_frame_bounds, dissolve_visible_step, hit_test_frame,
    rasterize_polygon, render_scene_frame,
};

pub const PLAYFIELD_W: i32 = 270;
pub const PLAYFIELD_H: i32 = 196;
pub const TOOLBOX_W: i32 = 48;
const DOS_TICK_SECS: f32 = 65536.0 / 1193182.0;

/// Exact column widths (`sum = 270`) for grid dimensions `2..=8` (`DS:0x12AE`).
pub fn grid_col_widths(dim: usize) -> &'static [i32] {
    match dim {
        2 => &[135, 135],
        3 => &[90, 90, 90],
        4 => &[67, 68, 68, 67],
        5 => &[54, 54, 54, 54, 54],
        6 => &[45, 45, 45, 45, 45, 45],
        7 => &[39, 38, 39, 38, 39, 38, 39],
        _ => &[33, 34, 34, 34, 34, 34, 34, 33],
    }
}

/// Exact row heights (`sum = 196`) for grid dimensions `2..=8` (`DS:0x12AE`).
pub fn grid_row_heights(dim: usize) -> &'static [i32] {
    match dim {
        2 => &[98, 98],
        3 => &[65, 66, 65],
        4 => &[49, 49, 49, 49],
        5 => &[39, 39, 40, 39, 39],
        6 => &[32, 33, 33, 33, 33, 32],
        7 => &[28, 28, 28, 28, 28, 28, 28],
        _ => &[25, 24, 25, 24, 25, 24, 25, 24],
    }
}

/// Original vertical edge curve polygon `(extent, points)` from `DS:0x4B86`.
#[rustfmt::skip]
pub fn vert_edge_curve(dim: usize, variant: usize) -> (i32, &'static [(i16, i16)]) {
    match (dim, variant & 3) {
        (2, 0) => (38, &[(0, 32), (1, 36), (3, 37), (5, 38), (7, 38), (10, 37), (12, 35), (15, 33), (18, 32), (24, 32), (27, 33), (29, 34), (31, 35), (32, 36), (35, 39), (36, 41), (37, 45), (37, 50), (36, 53), (35, 56), (32, 59), (31, 60), (29, 61), (27, 62), (23, 63), (19, 63), (16, 62), (14, 61), (12, 60), (10, 58), (8, 57), (5, 57), (2, 58), (0, 60), (0, 63), (0, 32)]),
        (2, 1) => (35, &[(0, 15), (0, 17), (1, 20), (4, 24), (7, 26), (12, 28), (15, 29), (20, 31), (23, 33), (26, 34), (28, 36), (30, 38), (31, 39), (33, 42), (34, 46), (34, 50), (33, 54), (31, 57), (27, 61), (25, 62), (22, 63), (19, 64), (12, 64), (9, 63), (6, 62), (4, 61), (2, 60), (0, 58), (0, 15)]),
        (2, 2) => (9, &[(0, 4), (1, 7), (3, 15), (6, 29), (8, 41), (8, 54), (7, 63), (6, 71), (4, 81), (2, 88), (0, 93), (0, 4)]),
        (2, _) => (9, &[(0, 1), (2, 3), (4, 5), (6, 10), (7, 13), (8, 17), (8, 31), (7, 38), (6, 43), (4, 52), (3, 57), (1, 63), (0, 75), (0, 1)]),

        (3, 0) => (28, &[(0, 19), (1, 23), (3, 24), (7, 23), (10, 20), (13, 19), (17, 19), (20, 20), (23, 21), (25, 23), (26, 25), (27, 28), (27, 32), (26, 34), (25, 36), (24, 38), (23, 39), (21, 40), (17, 41), (13, 41), (10, 40), (7, 37), (5, 36), (3, 36), (0, 39), (0, 41), (0, 19)]),
        (3, 1) => (27, &[(0, 7), (1, 11), (5, 15), (8, 17), (15, 20), (17, 21), (20, 22), (23, 24), (24, 25), (26, 30), (26, 32), (25, 35), (24, 38), (22, 40), (19, 42), (15, 43), (11, 43), (5, 42), (2, 40), (0, 38), (0, 7)]),
        (3, 2) => (7, &[(0, 3), (1, 5), (2, 8), (3, 11), (4, 15), (5, 20), (6, 27), (6, 37), (5, 44), (4, 49), (3, 53), (2, 56), (1, 58), (0, 61), (0, 3)]),
        (3, _) => (6, &[(0, 1), (2, 3), (4, 8), (5, 13), (5, 22), (3, 31), (1, 37), (0, 44), (0, 52), (0, 1)]),

        (4, 0) => (19, &[(0, 14), (1, 18), (3, 18), (6, 15), (8, 14), (12, 14), (14, 15), (16, 16), (17, 17), (18, 19), (18, 24), (17, 26), (16, 28), (15, 29), (12, 30), (9, 30), (6, 29), (4, 27), (3, 26), (1, 26), (0, 28), (0, 30), (0, 14)]),
        (4, 1) => (17, &[(0, 6), (1, 10), (3, 12), (5, 14), (7, 15), (12, 17), (15, 20), (16, 23), (16, 25), (15, 28), (13, 30), (10, 31), (7, 31), (2, 30), (0, 28), (0, 6)]),
        (4, 2) => (6, &[(0, 3), (1, 5), (2, 8), (4, 14), (5, 20), (5, 29), (4, 34), (2, 39), (1, 41), (0, 44), (0, 3)]),
        (4, _) => (5, &[(0, 1), (2, 3), (3, 5), (4, 10), (4, 19), (3, 23), (1, 28), (0, 33), (0, 38), (0, 1)]),

        (5, 0) => (15, &[(0, 11), (2, 13), (4, 13), (5, 11), (6, 10), (8, 10), (11, 11), (13, 13), (14, 15), (14, 19), (13, 22), (11, 24), (9, 25), (6, 25), (4, 23), (3, 22), (1, 22), (0, 24), (0, 11)]),
        (5, 1) => (12, &[(0, 5), (1, 9), (6, 14), (9, 16), (10, 17), (11, 19), (11, 21), (10, 24), (7, 26), (3, 26), (1, 25), (0, 24), (0, 5)]),
        (5, 2) => (5, &[(0, 3), (1, 5), (2, 7), (3, 11), (4, 16), (4, 23), (3, 26), (2, 30), (1, 32), (0, 35), (0, 3)]),
        (5, _) => (4, &[(0, 1), (1, 2), (2, 4), (3, 7), (3, 14), (2, 18), (1, 22), (0, 26), (0, 32), (0, 1)]),

        (6, 0) => (12, &[(0, 10), (1, 11), (3, 10), (4, 9), (6, 9), (9, 10), (10, 11), (11, 13), (11, 15), (10, 18), (9, 19), (7, 20), (4, 20), (4, 19), (1, 18), (0, 19), (0, 10)]),
        (6, 1) => (10, &[(0, 4), (1, 8), (4, 11), (6, 13), (8, 15), (9, 17), (9, 19), (8, 21), (6, 22), (2, 22), (0, 21), (0, 4)]),
        (6, 2) => (4, &[(0, 3), (1, 5), (2, 9), (3, 13), (3, 17), (2, 22), (1, 25), (0, 28), (0, 3)]),
        (6, _) => (4, &[(0, 1), (1, 2), (2, 4), (3, 6), (3, 13), (2, 16), (1, 19), (0, 22), (0, 26), (0, 1)]),

        (7, 0) => (11, &[(0, 9), (1, 10), (3, 9), (4, 8), (5, 8), (8, 9), (10, 11), (10, 15), (8, 17), (6, 18), (4, 18), (3, 17), (2, 16), (0, 17), (0, 9)]),
        (7, 1) => (8, &[(0, 3), (1, 7), (6, 12), (7, 14), (7, 17), (4, 19), (1, 19), (0, 18), (0, 3)]),
        (7, 2) => (3, &[(0, 2), (1, 5), (2, 8), (2, 15), (1, 19), (0, 22), (0, 2)]),
        (7, _) => (3, &[(0, 1), (1, 2), (2, 4), (2, 9), (1, 13), (0, 19), (0, 23), (0, 1)]),

        (_, 0) => (10, &[(0, 7), (1, 8), (2, 7), (3, 6), (4, 6), (7, 7), (9, 9), (9, 12), (7, 14), (5, 15), (4, 15), (2, 14), (1, 13), (0, 14), (0, 7)]),
        (_, 1) => (7, &[(0, 3), (1, 7), (2, 8), (5, 11), (6, 13), (6, 15), (4, 17), (1, 17), (0, 16), (0, 3)]),
        (_, 2) => (3, &[(0, 2), (1, 4), (1, 6), (2, 8), (2, 14), (1, 17), (1, 18), (0, 21), (0, 2)]),
        (_, _) => (3, &[(0, 1), (1, 2), (2, 4), (2, 7), (1, 10), (0, 17), (0, 1)]),
    }
}

/// Original horizontal edge curve polygon `(extent, points)` from `DS:0x4B94`.
#[rustfmt::skip]
pub fn horiz_edge_curve(dim: usize, variant: usize) -> (i32, &'static [(i16, i16)]) {
    match (dim, variant & 3) {
        (2, 0) => (33, &[(48, 0), (51, 1), (53, 2), (55, 4), (55, 7), (54, 8), (51, 10), (49, 12), (48, 15), (47, 18), (47, 21), (48, 25), (52, 29), (55, 31), (60, 32), (68, 32), (74, 31), (77, 29), (81, 25), (82, 22), (82, 18), (81, 15), (80, 12), (77, 9), (75, 8), (74, 7), (74, 4), (76, 2), (78, 1), (81, 0), (48, 0)]),
        (2, 1) => (27, &[(53, 0), (50, 3), (49, 6), (48, 9), (48, 15), (49, 17), (50, 20), (53, 23), (55, 24), (58, 25), (61, 26), (67, 26), (71, 25), (75, 24), (77, 22), (79, 20), (81, 17), (82, 14), (84, 11), (86, 8), (89, 5), (92, 3), (95, 1), (99, 0), (103, 0), (53, 0)]),
        (2, 2) => (9, &[(6, 0), (12, 2), (23, 4), (38, 6), (48, 7), (60, 8), (73, 8), (85, 7), (95, 6), (110, 4), (121, 2), (127, 0), (6, 0)]),
        (2, _) => (7, &[(24, 0), (34, 0), (46, 2), (54, 3), (64, 4), (75, 5), (90, 6), (112, 6), (118, 5), (122, 4), (125, 3), (128, 2), (131, 1), (132, 0), (24, 0)]),

        (3, 0) => (22, &[(33, 0), (34, 0), (36, 2), (37, 3), (37, 5), (33, 9), (32, 12), (32, 13), (33, 16), (36, 19), (39, 20), (42, 21), (46, 21), (50, 20), (53, 19), (56, 16), (57, 13), (57, 12), (56, 9), (52, 5), (52, 4), (53, 1), (56, 0), (33, 0)]),
        (3, 1) => (18, &[(34, 0), (31, 3), (30, 6), (30, 10), (31, 13), (34, 16), (37, 17), (44, 17), (48, 16), (51, 14), (54, 10), (55, 7), (58, 4), (60, 2), (63, 1), (69, 0), (34, 0)]),
        (3, 2) => (7, &[(5, 0), (13, 2), (16, 3), (22, 4), (29, 5), (37, 6), (52, 6), (60, 5), (66, 4), (71, 3), (76, 2), (79, 1), (82, 0), (5, 0)]),
        (3, _) => (6, &[(18, 0), (26, 0), (40, 2), (51, 4), (59, 5), (71, 5), (78, 4), (84, 2), (86, 1), (87, 0), (18, 0)]),

        (4, 0) => (16, &[(24, 0), (27, 1), (27, 2), (26, 5), (24, 7), (23, 8), (23, 10), (25, 13), (28, 15), (36, 15), (39, 14), (41, 12), (42, 10), (42, 9), (41, 7), (39, 5), (38, 3), (38, 1), (41, 0), (24, 0)]),
        (4, 1) => (14, &[(26, 0), (24, 2), (23, 5), (23, 8), (24, 11), (25, 12), (29, 13), (33, 13), (36, 12), (38, 10), (40, 7), (44, 3), (47, 1), (51, 0), (26, 0)]),
        (4, 2) => (6, &[(6, 0), (11, 2), (16, 3), (23, 4), (30, 5), (36, 5), (44, 4), (49, 3), (55, 2), (61, 0), (6, 0)]),
        (4, _) => (5, &[(14, 0), (21, 0), (28, 1), (49, 4), (56, 4), (59, 3), (64, 1), (65, 0), (14, 0)]),

        (5, 0) => (12, &[(18, 0), (20, 2), (20, 3), (18, 5), (17, 6), (17, 8), (18, 9), (19, 10), (21, 11), (28, 11), (31, 10), (32, 9), (33, 8), (33, 6), (32, 5), (30, 3), (31, 0), (32, 0), (18, 0)]),
        (5, 1) => (9, &[(20, 0), (18, 2), (18, 5), (20, 7), (23, 8), (26, 8), (29, 7), (32, 4), (35, 1), (40, 0), (20, 0)]),
        (5, 2) => (5, &[(4, 0), (7, 1), (10, 2), (15, 3), (22, 4), (31, 4), (37, 3), (42, 2), (45, 1), (48, 0), (4, 0)]),
        (5, _) => (4, &[(11, 0), (17, 0), (24, 1), (31, 2), (36, 3), (43, 3), (47, 2), (49, 1), (51, 0), (11, 0)]),

        (6, 0) => (10, &[(16, 0), (17, 1), (17, 2), (15, 4), (15, 6), (17, 8), (20, 9), (24, 9), (27, 8), (29, 6), (29, 4), (27, 2), (27, 1), (28, 0), (16, 0)]),
        (6, 1) => (8, &[(17, 0), (16, 2), (16, 5), (17, 6), (20, 7), (24, 7), (27, 5), (29, 3), (31, 1), (35, 0), (17, 0)]),
        (6, 2) => (4, &[(3, 0), (8, 1), (12, 2), (18, 3), (26, 3), (31, 2), (36, 1), (41, 0), (3, 0)]),
        (6, _) => (4, &[(8, 0), (14, 0), (19, 1), (24, 2), (28, 3), (37, 3), (40, 2), (41, 1), (43, 0), (8, 0)]),

        (7, 0) => (9, &[(12, 0), (13, 1), (13, 2), (11, 4), (11, 5), (13, 7), (16, 8), (19, 8), (22, 7), (24, 5), (24, 4), (22, 2), (23, 0), (12, 0)]),
        (7, 1) => (7, &[(14, 0), (13, 2), (13, 4), (14, 5), (16, 6), (18, 6), (21, 5), (23, 3), (25, 1), (27, 0), (29, 0), (14, 0)]),
        (7, 2) => (3, &[(4, 0), (9, 1), (14, 2), (22, 2), (28, 1), (33, 0), (4, 0)]),
        (7, _) => (3, &[(7, 0), (13, 0), (18, 1), (23, 2), (32, 2), (35, 1), (36, 0), (7, 0)]),

        (_, 0) => (8, &[(11, 0), (12, 1), (10, 3), (10, 4), (12, 6), (15, 7), (17, 7), (20, 6), (22, 4), (22, 3), (20, 1), (21, 0), (11, 0)]),
        (_, 1) => (6, &[(11, 0), (10, 1), (10, 3), (13, 5), (15, 5), (18, 4), (21, 1), (24, 0), (25, 0), (11, 0)]),
        (_, 2) => (3, &[(2, 0), (7, 1), (11, 2), (19, 2), (24, 1), (29, 0), (2, 0)]),
        (_, _) => (3, &[(5, 0), (11, 0), (16, 1), (21, 2), (27, 2), (30, 1), (31, 0), (5, 0)]),
    }
}

#[inline]
fn edge_curve(is_vert: bool, dim: usize, variant: usize) -> (i32, &'static [(i16, i16)]) {
    if is_vert {
        vert_edge_curve(dim, variant)
    } else {
        horiz_edge_curve(dim, variant)
    }
}

/// Cut jigsaw piece ready for rendering in the tray, while dragging, or when placed on the board.
#[derive(Clone)]
pub struct JigsawPiece {
    pub index: usize,
    pub row: usize,
    pub col: usize,
    /// Playfield-relative target bounding box `(target_x, target_y, width, height)`.
    pub target_x: i32,
    pub target_y: i32,
    pub base_x: i32,
    pub base_y: i32,
    pub base_w: i32,
    pub base_h: i32,
    pub sprite: Sprite8061,
    pub placed: bool,
}

/// Active animation playback state (`FILM`, `F_ALL`, `F_MICKEY`, etc.).
#[derive(Clone, Debug)]
pub struct AnimationPlayer {
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

struct LcgRng(u32);

impl LcgRng {
    fn new(seed: u32) -> Self {
        Self(seed | 1)
    }

    fn next(&mut self) -> usize {
        self.0 = self.0.wrapping_mul(214013).wrapping_add(2531011);
        ((self.0 >> 16) & 0x7FFF) as usize
    }

    fn shuffled_indices(&mut self, count: usize) -> Vec<usize> {
        let mut order: Vec<usize> = (0..count).collect();
        for i in (1..order.len()).rev() {
            let j = self.next() % (i + 1);
            order.swap(i, j);
        }
        order
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolboxTarget {
    Menu,
    ScrollUp,
    ScrollDown,
    TraySlot(usize),
    ClosedTab,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PointerVisualState {
    Anim {
        dissolve_step: usize,
    },
    Solved,
    Board {
        hovered_toolbox: Option<ToolboxTarget>,
        pressed_toolbox: Option<ToolboxTarget>,
        held_piece_pos: Option<(usize, i32, i32)>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PuzzlePresentKey {
    session_id: u64,
    asset_rev: u64,
    visual_rev: u64,
    picture_guide: bool,
    pointer_state: PointerVisualState,
}

#[inline]
pub(crate) fn dissolve_progress(anim: &AnimationPlayer) -> f32 {
    let duration = (anim.frame_delay_secs * 0.5).min(4.0 * DOS_TICK_SECS);
    if duration > 0.0 {
        anim.tick_accum / duration
    } else {
        1.0
    }
}

static NEXT_SESSION_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct PieceEdges {
    left: Option<(usize, i32)>,
    right: Option<(usize, i32)>,
    top: Option<(usize, i32)>,
    bottom: Option<(usize, i32)>,
}

/// Complete active puzzle session state (`seg_0e59`, `seg_0b43`).
pub struct PuzzleSession {
    session_id: u64,
    pub(crate) puzzle_num: usize,
    pub(crate) asset: PuzzleAsset,
    pub(crate) grid_dim: usize,
    pub(crate) curved_edges: bool,
    pub(crate) toolbox_open: bool,
    pub(crate) grid_lines: bool,
    pub(crate) timer_enabled: bool,
    pub(crate) text_enabled: bool,
    pub(crate) elapsed_secs: f64,
    pub(crate) pieces: Vec<JigsawPiece>,
    pub(crate) board_cells: Vec<Option<usize>>,
    piece_edges: Vec<PieceEdges>,
    pub(crate) tray_order: Vec<usize>,
    pub(crate) tray_scroll: usize,
    pub(crate) held_piece: Option<usize>,
    pub(crate) held_group: Vec<(usize, i32, i32)>,
    held_group_anchor: Option<(usize, usize)>,
    pub(crate) keyboard_pointer: Option<(f32, f32)>,
    drag_origin: Option<(i32, i32)>,
    pub(crate) pressed_toolbox_target: Option<ToolboxTarget>,
    pub(crate) hint_piece: Option<(usize, f32)>,
    pub(crate) solved: bool,
    pending_completion_feedback: bool,
    pub(crate) active_anim: Option<AnimationPlayer>,
    voice_rng: LcgRng,
    pub(crate) status_message: Option<(String, f32)>,
    pub(crate) full_picture_fb: Box<Framebuffer>,
    pub(crate) grid_polylines_v: Vec<Vec<(i32, i32)>>,
    pub(crate) grid_polylines_h: Vec<Vec<(i32, i32)>>,
    asset_rev: u64,
    board_rev: u64,
    tray_rev: u64,
    visual_rev: u64,
}

/// Exports puzzle `puzzle_num`'s `.PRT` monochrome line-art image as `prints/P{num:02}.pbm`.
pub fn export_print_pbm(game_dir: &Path, puzzle_num: usize) -> Result<String, String> {
    let prt = PrintAsset::load(game_dir, puzzle_num).map_err(|e| format!("Print error: {e}"))?;
    let out_dir = game_dir.join("prints");
    let rel_path = format!("prints/P{puzzle_num:02}.pbm");
    let pbm_path = out_dir.join(format!("P{puzzle_num:02}.pbm"));
    let mut pbm = Vec::with_capacity(8032);
    pbm.extend_from_slice(b"P4\n320 200\n");
    pbm.extend_from_slice(&prt.bitmap);
    fs::create_dir_all(&out_dir)
        .and_then(|()| fs::write(&pbm_path, &pbm))
        .map_err(|err| format!("Print error: {err}"))?;
    Ok(format!("Saved printable line-art to {rel_path}"))
}

impl PuzzleSession {
    pub fn new(
        puzzle_num: usize,
        asset: PuzzleAsset,
        grid_dim: usize,
        curved_edges: bool,
        grid_lines: bool,
        timer_enabled: bool,
    ) -> Self {
        let dim = grid_dim.clamp(1, 8);
        let full_picture_fb = Self::render_full_picture(&asset);

        let mut session = Self {
            session_id: NEXT_SESSION_ID.fetch_add(1, Ordering::Relaxed),
            puzzle_num,
            asset,
            grid_dim: dim,
            curved_edges,
            toolbox_open: true,
            grid_lines,
            timer_enabled,
            text_enabled: true,
            elapsed_secs: 0.0,
            pieces: Vec::new(),
            board_cells: Vec::new(),
            piece_edges: Vec::new(),
            tray_order: Vec::new(),
            tray_scroll: 0,
            held_piece: None,
            held_group: Vec::new(),
            held_group_anchor: None,
            keyboard_pointer: None,
            drag_origin: None,
            pressed_toolbox_target: None,
            hint_piece: None,
            solved: false,
            pending_completion_feedback: false,
            active_anim: None,
            voice_rng: LcgRng::new((puzzle_num as u32).wrapping_mul(0x9e37_79b9) | 1),
            status_message: None,
            full_picture_fb,
            grid_polylines_v: Vec::new(),
            grid_polylines_h: Vec::new(),
            asset_rev: 1,
            board_rev: 1,
            tray_rev: 1,
            visual_rev: 1,
        };
        session.cut_pieces();
        session
    }

    #[inline]
    fn bump_visual(&mut self) {
        self.visual_rev = self.visual_rev.wrapping_add(1);
    }

    #[inline]
    fn bump_tray(&mut self) {
        self.tray_rev = self.tray_rev.wrapping_add(1);
        self.visual_rev = self.visual_rev.wrapping_add(1);
    }

    #[inline]
    fn bump_board(&mut self) {
        self.board_rev = self.board_rev.wrapping_add(1);
        self.visual_rev = self.visual_rev.wrapping_add(1);
    }

    pub fn session_id(&self) -> u64 {
        self.session_id
    }

    pub fn placed_count(&self) -> usize {
        self.pieces.iter().filter(|p| p.placed).count()
    }

    pub fn status_message(&self) -> Option<&str> {
        self.status_message.as_ref().map(|(s, _)| s.as_str())
    }

    pub fn asset_rev(&self) -> u64 {
        self.asset_rev
    }

    pub fn board_rev(&self) -> u64 {
        self.board_rev
    }

    pub fn tray_rev(&self) -> u64 {
        self.tray_rev
    }

    pub fn active_pointer(&self, mouse_x: i32, mouse_y: i32) -> (i32, i32) {
        self.keyboard_pointer
            .map(|(x, y)| (x.round() as i32, y.round() as i32))
            .unwrap_or((mouse_x, mouse_y))
    }

    pub fn set_preferences(&mut self, grid_lines: bool, timer_enabled: bool, text_enabled: bool) {
        if self.grid_lines != grid_lines {
            self.grid_lines = grid_lines;
            self.bump_board();
        }
        self.timer_enabled = timer_enabled;
        if self.text_enabled != text_enabled {
            self.text_enabled = text_enabled;
            if self.active_anim.is_some() {
                self.bump_visual();
            }
        }
    }

    pub fn toggle_grid_lines(&mut self) -> bool {
        self.grid_lines = !self.grid_lines;
        self.bump_board();
        self.grid_lines
    }

    pub fn clear_keyboard_pointer(&mut self) {
        if self.keyboard_pointer.take().is_some() {
            self.bump_visual();
        }
    }

    fn render_full_picture_into(asset: &PuzzleAsset, fb: &mut Framebuffer) {
        fb.reset_clip();
        fb.palette = asset.palette;
        let static_frame_idx = asset.static_frame_idx();
        render_scene_frame(fb, asset, static_frame_idx, 0, 0, 0, true, 0, 0);
    }

    fn render_full_picture(asset: &PuzzleAsset) -> Box<Framebuffer> {
        let mut full_picture_fb = Box::new(Framebuffer::new());
        Self::render_full_picture_into(asset, &mut full_picture_fb);
        full_picture_fb
    }

    pub fn replace_scene_asset(&mut self, asset: PuzzleAsset) {
        assert_eq!(asset.id, self.puzzle_num);
        Self::render_full_picture_into(&asset, &mut self.full_picture_fb);
        let old_placed: Vec<bool> = self.pieces.iter().map(|p| p.placed).collect();
        let old_board_cells = std::mem::take(&mut self.board_cells);
        let old_tray_order = std::mem::take(&mut self.tray_order);
        let old_tray_scroll = self.tray_scroll;
        let old_held = self.held_piece;
        let old_held_group = std::mem::take(&mut self.held_group);
        let old_held_group_anchor = self.held_group_anchor.take();
        let old_solved = self.solved;

        self.asset = asset;
        self.cut_pieces();

        if self.pieces.len() == old_placed.len() {
            for (new_piece, placed) in self.pieces.iter_mut().zip(old_placed) {
                new_piece.placed = placed;
            }
            if old_board_cells.len() == self.board_cells.len() {
                self.board_cells = old_board_cells;
            }
            self.tray_order = old_tray_order;
            self.tray_scroll = old_tray_scroll;
            self.clamp_tray_scroll();
            self.held_piece = old_held;
            self.held_group = old_held_group;
            self.held_group_anchor = old_held_group_anchor;
            self.solved = old_solved;
        } else {
            self.hint_piece = None;
            self.toolbox_open = true;
        }
        self.active_anim = None;
        self.drag_origin = None;
        self.asset_rev = self.asset_rev.wrapping_add(1);
        self.bump_board();
    }

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

    /// Cuts the `270 x 196` puzzle picture into `dim x dim` interlocking (or straight) jigsaw pieces (`FUN_1e59_011c`).
    pub fn cut_pieces(&mut self) {
        if self.grid_dim == 1 {
            self.cut_preset_pieces();
            return;
        }
        let dim = self.grid_dim;
        let col_w = grid_col_widths(dim);
        let row_h = grid_row_heights(dim);

        let mut col_x = [0i32; 9];
        for c in 0..dim {
            col_x[c + 1] = col_x[c] + col_w[c];
        }
        let mut row_y = [0i32; 9];
        for r in 0..dim {
            row_y[r + 1] = row_y[r] + row_h[r];
        }

        // Deterministic PRNG seeded by puzzle number and grid dimension (`FUN_1239_4386`)
        let mut rng = LcgRng::new(
            (self.puzzle_num as u32)
                .wrapping_mul(1103515245)
                .wrapping_add((dim as u32) * 12345),
        );

        let mut next_edge = || {
            let code = rng.next() & 7;
            (code & 3, if code < 4 { 1 } else { -1 })
        };

        // Shared vertical & horizontal borders: `(variant 0..3, dir -1 or +1)`
        let v_stride = dim + 1;
        let mut v_edges = vec![(0usize, 1i32); dim * v_stride];
        for r in 0..dim {
            for c in 1..dim {
                v_edges[r * v_stride + c] = next_edge();
            }
        }
        let mut h_edges = vec![(0usize, 1i32); (dim + 1) * dim];
        for r in 1..dim {
            for c in 0..dim {
                h_edges[r * dim + c] = next_edge();
            }
        }

        // Build full-playfield grid polylines for the Grid Lines overlay (`FUN_1b43_0448`)
        self.grid_polylines_v.clear();
        for c in 1..dim {
            let bx = col_x[c];
            let mut line = Vec::new();
            line.push((bx, 0));
            for r in 0..dim {
                let by = row_y[r];
                if self.curved_edges {
                    let (variant, dir) = v_edges[r * v_stride + c];
                    let (_, pts) = vert_edge_curve(dim, variant);
                    for &(dx, dy) in pts {
                        line.push((bx + (dx as i32) * dir, by + (dy as i32)));
                    }
                }
                line.push((bx, row_y[r + 1]));
            }
            self.grid_polylines_v.push(line);
        }

        self.grid_polylines_h.clear();
        for r in 1..dim {
            let by = row_y[r];
            let mut line = Vec::new();
            line.push((0, by));
            for c in 0..dim {
                let bx = col_x[c];
                if self.curved_edges {
                    let (variant, dir) = h_edges[r * dim + c];
                    let (_, pts) = horiz_edge_curve(dim, variant);
                    for &(dx, dy) in pts {
                        line.push((bx + (dx as i32), by + (dy as i32) * dir));
                    }
                }
                line.push((col_x[c + 1], by));
            }
            self.grid_polylines_h.push(line);
        }

        // Cut each piece directly into its local `(tw, th)` mask and pixel buffer (`FUN_1e59_011c`)
        let mut pieces = Vec::with_capacity(dim * dim);
        let mut piece_edges = Vec::with_capacity(dim * dim);
        let mut poly = Vec::with_capacity(48);
        let mut nodes = Vec::with_capacity(48);
        // Source picture uses playfield coordinates `2..272, 2..198`
        let src_ox = 2;
        let src_oy = 2;

        for r in 0..dim {
            for c in 0..dim {
                let idx = r * dim + c;
                let bx = col_x[c];
                let by = row_y[r];
                let bw = col_w[c];
                let bh = row_h[r];

                // `(active, is_vert, edge, origin_x, origin_y, tab_out_dir)`
                let borders = [
                    (c > 0, true, v_edges[r * v_stride + c], bx, by, -1),
                    (
                        c + 1 < dim,
                        true,
                        v_edges[r * v_stride + c + 1],
                        bx + bw,
                        by,
                        1,
                    ),
                    (r > 0, false, h_edges[r * dim + c], bx, by, -1),
                    (
                        r + 1 < dim,
                        false,
                        h_edges[(r + 1) * dim + c],
                        bx,
                        by + bh,
                        1,
                    ),
                ];

                let mut pads = [0i32; 4];
                if self.curved_edges {
                    for (i, &(active, is_vert, (var, dir), _, _, out_dir)) in
                        borders.iter().enumerate()
                    {
                        if active && dir == out_dir {
                            pads[i] = edge_curve(is_vert, dim, var).0;
                        }
                    }
                }
                let [pad_left, pad_right, pad_top, pad_bottom] = pads;

                let tx = bx - pad_left;
                let ty = by - pad_top;
                let tw = bw + pad_left + pad_right;
                let th = bh + pad_top + pad_bottom;

                let mut opacity = vec![false; (tw * th) as usize];
                for dy in pad_top..pad_top + bh {
                    let row = (dy * tw) as usize;
                    opacity[row + pad_left as usize..row + (pad_left + bw) as usize].fill(true);
                }

                if self.curved_edges {
                    for &(active, is_vert, (var, dir), ex, ey, out_dir) in &borders {
                        if !active {
                            continue;
                        }
                        let (_, pts) = edge_curve(is_vert, dim, var);
                        let lx = ex - tx;
                        let ly = ey - ty;
                        poly.clear();
                        poly.extend(pts.iter().map(|&(dx, dy)| {
                            if is_vert {
                                (lx + (dx as i32) * dir, ly + (dy as i32))
                            } else {
                                (lx + (dx as i32), ly + (dy as i32) * dir)
                            }
                        }));
                        let fill_val = dir == out_dir;
                        rasterize_polygon(&poly, 0, 0, [0, 0, tw, th], &mut nodes, |y, x0, x1| {
                            let row = (y * tw) as usize;
                            opacity[row + x0 as usize..=row + x1 as usize].fill(fill_val);
                        });
                        // `rasterize_polygon` skips `min_y` on horizontal edges extending downward (`dir == 1`),
                        // which would leave a 1px bridge across downward blanks and a 1px gap above tabs.
                        if !is_vert && dir == 1 && (0..th).contains(&ly) {
                            let (mut x_min, mut x_max) = (i32::MAX, i32::MIN);
                            for &(px, py) in &poly {
                                if py == ly {
                                    x_min = x_min.min(px);
                                    x_max = x_max.max(px);
                                }
                            }
                            let x0 = x_min.max(0);
                            let x1 = x_max.min(tw - 1);
                            if x0 <= x1 {
                                let row = (ly * tw) as usize;
                                opacity[row + x0 as usize..=row + x1 as usize].fill(fill_val);
                            }
                        }
                    }
                }

                let trans = 0xF8u8;
                let mut px_buf = vec![trans; opacity.len()];
                let dy_start = (-ty).max(0);
                let dy_end = (PLAYFIELD_H - ty).min(th);
                let dx_start = (-tx).max(0);
                let dx_end = (PLAYFIELD_W - tx).min(tw);
                for dy in dy_start..dy_end {
                    let src_row = ((src_oy + ty + dy) as usize) * 320 + (src_ox + tx) as usize;
                    let dst_row = (dy * tw) as usize;
                    for dx in dx_start..dx_end {
                        let i = dst_row + dx as usize;
                        if opacity[i] {
                            px_buf[i] = self.full_picture_fb.pixels[src_row + dx as usize];
                        }
                    }
                }

                let sprite = Sprite8061::from_8bpp(tw, th, trans, px_buf, opacity);

                pieces.push(JigsawPiece {
                    index: idx,
                    row: r,
                    col: c,
                    target_x: tx,
                    target_y: ty,
                    base_x: bx,
                    base_y: by,
                    base_w: bw,
                    base_h: bh,
                    sprite,
                    placed: false,
                });
                let edge_tag = |is_outer: bool, curved_val: (usize, i32)| {
                    if is_outer {
                        None
                    } else if self.curved_edges {
                        Some(curved_val)
                    } else {
                        Some((0, 0))
                    }
                };
                piece_edges.push(PieceEdges {
                    left: edge_tag(c == 0, v_edges[r * v_stride + c]),
                    right: edge_tag(c + 1 == dim, v_edges[r * v_stride + (c + 1)]),
                    top: edge_tag(r == 0, h_edges[r * dim + c]),
                    bottom: edge_tag(r + 1 == dim, h_edges[(r + 1) * dim + c]),
                });
            }
        }

        self.piece_edges = piece_edges;
        // Shuffle pieces into the tray (`FUN_1e59_0b0a`)
        self.finish_cutting(pieces, &mut rng);
    }

    fn cut_preset_pieces(&mut self) {
        let mut pieces = Vec::new();
        let mut pixels = Box::new(Framebuffer::new());
        let mut mask = Box::new(Framebuffer::new());
        if let Some(SceneRecord::Frame { commands, .. }) =
            self.asset.records.get(self.asset.preset_frame_idx())
        {
            for command in commands {
                let FrameCommand::SubFrame {
                    dx,
                    dy,
                    frame_idx,
                    flags,
                } = *command
                else {
                    continue;
                };
                let flags = flags & 3;
                let (x, y, w, h) = compute_frame_bounds(
                    &self.asset,
                    frame_idx,
                    i32::from(dx),
                    i32::from(dy),
                    flags,
                    0,
                );
                let (left, top) = (x.max(2), y.max(2));
                let (width, height) = (
                    ((x + w).min(272) - left).max(0),
                    ((y + h).min(198) - top).max(0),
                );
                if width == 0 || height == 0 {
                    continue;
                }
                pixels.set_clip(left, top, width, height);
                pixels.fill_rect(left, top, width, height, 0);
                render_scene_frame(
                    &mut pixels,
                    &self.asset,
                    frame_idx,
                    i32::from(dx),
                    i32::from(dy),
                    flags,
                    false,
                    0,
                    1,
                );
                mask.set_clip(left, top, width, height);
                mask.fill_rect(left, top, width, height, 0);
                render_scene_frame(
                    &mut mask,
                    &self.asset,
                    frame_idx,
                    i32::from(dx),
                    i32::from(dy),
                    flags | 0x0120,
                    false,
                    0,
                    1,
                );
                let cap = (width * height) as usize;
                let mut colors = Vec::with_capacity(cap);
                let mut opacity = Vec::with_capacity(cap);
                for py in top..top + height {
                    let row = (py as usize) * 320 + left as usize;
                    let w = width as usize;
                    colors.extend_from_slice(&pixels.pixels[row..row + w]);
                    opacity.extend(mask.pixels[row..row + w].iter().map(|&p| p != 0));
                }
                pieces.push(JigsawPiece {
                    index: pieces.len(),
                    row: 0,
                    col: pieces.len(),
                    target_x: left - 2,
                    target_y: top - 2,
                    base_x: left - 2,
                    base_y: top - 2,
                    base_w: width,
                    base_h: height,
                    sprite: Sprite8061::from_8bpp(width, height, 0, colors, opacity),
                    placed: false,
                });
            }
        }
        let mut rng = LcgRng::new(
            (self.puzzle_num as u32)
                .wrapping_mul(1103515245)
                .wrapping_add(12345),
        );
        self.piece_edges.clear();
        self.finish_cutting(pieces, &mut rng);
        self.grid_polylines_v.clear();
        self.grid_polylines_h.clear();
    }

    fn finish_cutting(&mut self, pieces: Vec<JigsawPiece>, rng: &mut LcgRng) {
        self.tray_order = rng.shuffled_indices(pieces.len());
        self.board_cells = if self.grid_dim > 1 {
            vec![None; self.grid_dim * self.grid_dim]
        } else {
            Vec::new()
        };
        self.pieces = pieces;
        self.tray_scroll = 0;
        self.held_piece = None;
        self.held_group.clear();
        self.held_group_anchor = None;
        self.drag_origin = None;
        self.solved = false;
        self.bump_tray();
    }

    /// Number of pieces visible at once in the left-hand tray (`1..=5` depending on piece size).
    pub fn visible_tray_slots(&self) -> usize {
        match self.grid_dim {
            2 => 2,
            3 => 3,
            4 | 5 => 4,
            _ => 5,
        }
    }

    pub(crate) fn tray_slot_height(&self) -> i32 {
        (156 / self.visible_tray_slots().max(1)) as i32
    }

    fn clamp_tray_scroll(&mut self) {
        let max_scroll = self
            .tray_order
            .len()
            .saturating_sub(self.visible_tray_slots());
        self.tray_scroll = self.tray_scroll.min(max_scroll);
    }

    pub fn scroll_tray(&mut self, delta: i32) -> bool {
        if !self.toolbox_open || self.solved {
            return false;
        }
        let prev = self.tray_scroll;
        if delta < 0 {
            self.tray_scroll = self
                .tray_scroll
                .saturating_sub(delta.unsigned_abs() as usize);
        } else if delta > 0 {
            self.tray_scroll = self.tray_scroll.saturating_add(delta as usize);
            self.clamp_tray_scroll();
        }
        if self.tray_scroll != prev {
            self.bump_tray();
            true
        } else {
            false
        }
    }

    /// Piece hint (`F1`). Returns `true` if a hint was issued.
    pub fn trigger_hint(&mut self) -> bool {
        if self.solved {
            return false;
        }
        let target_idx = self
            .held_piece
            .or_else(|| self.tray_order.get(self.tray_scroll).copied())
            .or_else(|| self.tray_order.first().copied())
            .or_else(|| self.first_misplaced_piece());
        if let Some(pidx) = target_idx {
            self.hint_piece = Some((pidx, 1.5));
            self.bump_visual();
            true
        } else {
            false
        }
    }

    /// Triggers `F2` Auto-Solve (`FUN_1e59_1136`): places all remaining pieces and starts completion animation.
    pub fn auto_solve(&mut self, audio: &mut AudioEngine) {
        if self.solved {
            return;
        }
        for p in &mut self.pieces {
            p.placed = true;
        }
        for (idx, slot) in self.board_cells.iter_mut().enumerate() {
            *slot = Some(idx);
        }
        self.tray_order.clear();
        self.tray_scroll = 0;
        self.held_group.clear();
        self.held_group_anchor = None;
        self.on_puzzle_completed(audio);
    }

    pub fn on_puzzle_completed(&mut self, audio: &mut AudioEngine) {
        audio.stop_all();
        self.solved = true;
        self.toolbox_open = false;
        self.held_piece = None;
        self.held_group.clear();
        self.held_group_anchor = None;
        self.hint_piece = None;
        self.drag_origin = None;
        if self.timer_enabled {
            let total_secs = self.elapsed_secs.max(0.0).floor() as u64;
            let hours = total_secs / 3600;
            let mins = (total_secs % 3600) / 60;
            let secs = total_secs % 60;
            self.status_message = Some((format!("Your time was {hours}:{mins:02}:{secs:02}"), 6.0));
        }
        self.bump_board();
        self.start_sequence(self.default_playback(), audio);
        if self.active_anim.is_some() {
            self.pending_completion_feedback = true;
        } else {
            self.pending_completion_feedback = false;
            audio.play_common(CommonSound::DoneWithThisOne);
        }
    }

    fn default_playback(&self) -> SequencePlayback {
        self.asset
            .sequences
            .get(1)
            .and_then(|film| {
                let SceneRecord::SequenceHeader {
                    default_playback, ..
                } = self.asset.records.get(film.header_idx)?
                else {
                    return None;
                };
                *default_playback
            })
            .unwrap_or(SequencePlayback {
                seq_idx: self.asset.full_anim_seq_idx,
                repetitions: 0,
                frame_delay_ticks: 2,
            })
    }

    pub fn start_sequence(&mut self, playback: SequencePlayback, audio: &mut AudioEngine) {
        let seq_idx = playback.seq_idx;
        let default_delay_secs = f32::from(playback.frame_delay_ticks) * DOS_TICK_SECS;
        if let Some((start_frame, end_frame)) = self
            .asset
            .sequences
            .get(seq_idx)
            .filter(|seq| seq.start_frame < seq.end_frame)
            .map(|seq| (seq.start_frame, seq.end_frame))
        {
            let mut anim = AnimationPlayer {
                seq_idx,
                start_frame,
                previous_frame: self.asset.static_frame_idx(),
                cur_frame: start_frame,
                end_frame,
                tick_accum: 0.0,
                frame_delay_secs: default_delay_secs,
                repetitions_left: playback.repetitions.max(1),
                default_delay_secs,
                transition_mode: 0,
                bubble: None,
                wait_for_audio: false,
                looping_sound: None,
            };
            Self::apply_frame_side_effects(
                &mut self.asset,
                &mut self.voice_rng,
                self.text_enabled,
                &mut anim,
                start_frame,
                audio,
            );
            self.active_anim = Some(anim);
            self.bump_visual();
        }
    }

    #[cfg(test)]
    fn trigger_frame_side_effects(&mut self, frame_idx: usize, audio: &mut AudioEngine) {
        if let Some(anim) = &mut self.active_anim {
            Self::apply_frame_side_effects(
                &mut self.asset,
                &mut self.voice_rng,
                self.text_enabled,
                anim,
                frame_idx,
                audio,
            );
        }
    }

    fn apply_frame_side_effects(
        asset: &mut PuzzleAsset,
        voice_rng: &mut LcgRng,
        text_enabled: bool,
        anim: &mut AnimationPlayer,
        frame_idx: usize,
        audio: &mut AudioEngine,
    ) {
        let mut delay_ticks = None;
        let mut sound_to_play = None;
        anim.wait_for_audio = false;
        anim.frame_delay_secs = anim.default_delay_secs;
        anim.transition_mode = 0;

        if let Some(SceneRecord::Frame { commands, .. }) = asset.records.get(frame_idx) {
            for cmd in commands {
                match *cmd {
                    FrameCommand::Delay { ticks, raw } => {
                        delay_ticks = Some((ticks, (raw & 0x3f) as u8));
                    }
                    FrameCommand::Sound {
                        snd_idx: Some(sidx),
                    } => sound_to_play = Some(sidx),
                    _ => {}
                }
            }
        }

        if let Some((ticks, transition)) = delay_ticks {
            anim.transition_mode = transition;
            anim.wait_for_audio = ticks == 998;
            if ticks != 0 && ticks != 998 {
                anim.frame_delay_secs = (ticks as f32 + 1.0) * DOS_TICK_SECS;
            }
        }

        if let Some(sidx) = sound_to_play {
            audio.stop_all();
            anim.bubble = None;
            anim.looping_sound = None;
            if let Some(entry) = asset.sounds.get_mut(sidx) {
                let mut voice_chosen = false;
                let looping = entry.selection_mode & 0x80 != 0;
                let has_music = entry
                    .sounds
                    .first()
                    .is_some_and(|desc| audio.iss_songs.keys().any(|(stem, _)| stem == &desc.stem));
                if (audio.sfx_enabled || !audio.music_enabled || !has_music)
                    && entry.sounds.len() > 1
                    && let Some(voice) = entry.select_voice(|| (voice_rng.next() & 1) != 0)
                {
                    let desc = &entry.sounds[voice + 1];
                    if audio.nnv_banks.contains_key(&desc.stem) {
                        voice_chosen = true;
                        audio.play_nnv(&desc.stem, desc.phrase_idx as usize);
                        if looping {
                            anim.looping_sound = Some((sidx, voice + 1));
                        }
                        if text_enabled {
                            anim.bubble = Some((sidx, voice));
                        }
                    }
                }
                if let Some(desc) = entry
                    .sounds
                    .first()
                    .filter(|desc| !voice_chosen && !desc.stem.is_empty())
                {
                    audio.play_iss(&desc.stem, desc.phrase_idx as usize);
                    if looping {
                        anim.looping_sound = Some((sidx, 0));
                    }
                }
            }
        }
        if anim.wait_for_audio && anim.bubble.is_some() && text_enabled && !audio.is_playing() {
            anim.frame_delay_secs = 20.0 * DOS_TICK_SECS;
        }
    }

    #[inline]
    fn hint_blink_phase(&self) -> Option<(usize, i32)> {
        self.hint_piece
            .map(|(idx, rem)| (idx, ((rem * 8.0) as i32) & 1))
    }

    pub fn update(&mut self, dt: f32, audio: &mut AudioEngine) {
        let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
        if !self.solved && self.timer_enabled {
            self.elapsed_secs += dt as f64;
        }
        let prev_hint = self.hint_blink_phase();
        if let Some((_, rem)) = &mut self.hint_piece {
            *rem -= dt;
            if *rem <= 0.0 {
                self.hint_piece = None;
            }
        }
        if self.hint_blink_phase() != prev_hint {
            self.bump_visual();
        }
        if let Some((_, rem)) = &mut self.status_message {
            *rem -= dt;
            if *rem <= 0.0 {
                self.status_message = None;
                self.bump_visual();
            }
        }
        let prev_anim_vis = self
            .active_anim
            .as_ref()
            .map(|a| (a.cur_frame, a.bubble.filter(|_| self.text_enabled)));
        if let Some(mut anim) = self.active_anim.take() {
            anim.tick_accum += dt;
            loop {
                if anim.tick_accum < anim.frame_delay_secs {
                    self.active_anim = Some(anim);
                    break;
                }
                if anim.wait_for_audio && audio.is_playing() {
                    anim.tick_accum = anim.frame_delay_secs;
                    self.active_anim = Some(anim);
                    break;
                }
                anim.tick_accum -= anim.frame_delay_secs;
                if anim.wait_for_audio {
                    anim.bubble = None;
                }
                anim.previous_frame = anim.cur_frame;
                if anim.cur_frame + 1 >= anim.end_frame {
                    if anim.repetitions_left <= 1 {
                        if anim.looping_sound.is_some() {
                            audio.stop_all();
                        }
                        if std::mem::take(&mut self.pending_completion_feedback) {
                            audio.play_common(CommonSound::DoneWithThisOne);
                        }
                        break;
                    }
                    anim.repetitions_left -= 1;
                    anim.cur_frame = anim.start_frame;
                } else {
                    anim.cur_frame += 1;
                }
                if !audio.is_playing()
                    && let Some((sound, descriptor)) = anim.looping_sound
                {
                    let desc = &self.asset.sounds[sound].sounds[descriptor];
                    if descriptor == 0 {
                        audio.play_iss(&desc.stem, desc.phrase_idx as usize);
                    } else {
                        audio.play_nnv(&desc.stem, desc.phrase_idx as usize);
                    }
                }
                let frame = anim.cur_frame;
                Self::apply_frame_side_effects(
                    &mut self.asset,
                    &mut self.voice_rng,
                    self.text_enabled,
                    &mut anim,
                    frame,
                    audio,
                );
            }
        }
        let next_anim_vis = self
            .active_anim
            .as_ref()
            .map(|a| (a.cur_frame, a.bubble.filter(|_| self.text_enabled)));
        if next_anim_vis != prev_anim_vis {
            self.bump_visual();
        }
    }

    /// Formats `elapsed_secs` as `H:MM:SS` (`0:00:00`, `seg_0b43:0cf8`).
    pub fn formatted_time(&self) -> String {
        let total = self.elapsed_secs as u64;
        let hrs = total / 3600;
        let mins = (total % 3600) / 60;
        let secs = total % 60;
        format!("{}:{:02}:{:02}", hrs, mins, secs)
    }

    fn solved_sequence_at(&self, mx: i32, my: i32) -> SequencePlayback {
        let fallback = self.default_playback();
        let Some(film) = self.asset.sequences.get(1) else {
            return fallback;
        };
        let Some(SceneRecord::SequenceHeader { hotspots, .. }) =
            self.asset.records.get(film.header_idx)
        else {
            return fallback;
        };
        let Some(SceneRecord::Frame { commands, .. }) = self.asset.records.get(film.start_frame)
        else {
            return fallback;
        };
        let mut selected = None;
        let mut child_index = 0;
        for command in commands {
            if let FrameCommand::SubFrame {
                dx,
                dy,
                frame_idx,
                flags,
            } = *command
            {
                let hotspot_idx = child_index;
                child_index += 1;
                let center_hit = hotspots.get(hotspot_idx).is_some_and(|hotspot| {
                    mx == hotspot.x as i32 + hotspot.width as i32 / 2
                        && my == hotspot.y as i32 + hotspot.height as i32 / 2
                });
                if flags & 0x00F0 != 0x0030
                    && (center_hit
                        || hit_test_frame(
                            &self.asset,
                            frame_idx,
                            dx as i32,
                            dy as i32,
                            flags,
                            mx,
                            my,
                            0,
                        ))
                {
                    selected = Some(hotspot_idx);
                }
            }
        }
        selected
            .and_then(|index| hotspots.get(index))
            .map(|hotspot| hotspot.playback)
            .unwrap_or(fallback)
    }

    pub fn cancel_pointer_drag(&mut self) {
        self.drag_origin = None;
    }

    pub fn move_keyboard_pointer(&mut self, dx: f32, dy: f32, dt: f32) {
        self.cancel_pointer_drag();
        let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
        let (x, y) = self.keyboard_pointer.unwrap_or((20.0, 40.0));
        let step = 120.0 * dt / dx.hypot(dy).max(1.0);
        let next = (
            (x + dx * step).clamp(0.0, 319.0),
            (y + dy * step).clamp(0.0, 199.0),
        );
        if self.keyboard_pointer != Some(next) {
            self.keyboard_pointer = Some(next);
            self.bump_visual();
        }
    }

    pub fn handle_keyboard_activate(&mut self, audio: &mut AudioEngine) -> Option<PuzzleAction> {
        self.cancel_pointer_drag();
        if self.active_anim.is_some() {
            return self.handle_click(0, 0, audio);
        }
        let had_pointer = self.keyboard_pointer.is_some();
        let (x, y) = *self.keyboard_pointer.get_or_insert((20.0, 40.0));
        if !had_pointer {
            self.bump_visual();
        }
        self.handle_click(x.round() as i32, y.round() as i32, audio)
    }

    pub fn toggle_toolbox(&mut self) {
        if self.solved {
            return;
        }
        self.cancel_pointer_drag();
        self.pressed_toolbox_target = None;
        self.toolbox_open = !self.toolbox_open;
        self.bump_board();
    }

    pub fn handle_pointer_press(
        &mut self,
        mx: i32,
        my: i32,
        audio: &mut AudioEngine,
    ) -> Option<PuzzleAction> {
        self.clear_keyboard_pointer();
        self.drag_origin = None;
        let pressed = self.toolbox_target_at(mx, my);
        let was_holding = self.held_piece.is_some();
        let action = self.handle_click(mx, my, audio);
        self.pressed_toolbox_target = if action == Some(PuzzleAction::OpenMenu) {
            None
        } else {
            pressed
        };
        if !was_holding && self.held_piece.is_some() {
            self.drag_origin = Some((mx, my));
        }
        action
    }

    pub fn handle_pointer_release(&mut self, mx: i32, my: i32, audio: &mut AudioEngine) {
        self.pressed_toolbox_target = None;
        let Some((start_x, start_y)) = self.drag_origin.take() else {
            return;
        };
        if !(0..320).contains(&mx) || !(0..200).contains(&my) || self.held_piece.is_none() {
            return;
        }
        if (mx - start_x).abs().max((my - start_y).abs()) < 4 {
            return;
        }
        if self.toolbox_open && mx < TOOLBOX_W && !(30..186).contains(&my) {
            return;
        }
        self.handle_click(mx, my, audio);
    }

    /// Returns `Some(PuzzleAction)` if a top-level navigation action was clicked.
    pub fn handle_click(
        &mut self,
        mx: i32,
        my: i32,
        audio: &mut AudioEngine,
    ) -> Option<PuzzleAction> {
        if !(0..320).contains(&mx) || !(0..200).contains(&my) {
            return None;
        }
        // If an animation is currently playing, clicking skips to the end of the animation
        if self.active_anim.is_some() {
            self.active_anim = None;
            self.pending_completion_feedback = false;
            audio.stop_all();
            self.bump_visual();
            return None;
        }

        // If the puzzle is solved, check interactive hotspots or top-left return corner!
        if self.solved {
            if mx < 28 && my < 24 {
                return Some(PuzzleAction::OpenMenu);
            }
            let playback = self.solved_sequence_at(mx - SCENE_OFFSET_X, my);
            self.start_sequence(playback, audio);
            return None;
        }

        // If toolbox is open on the left (`x < 48`), check toolbox buttons and tray slots
        if self.toolbox_open && mx < TOOLBOX_W {
            // Top bar buttons in toolbox:
            // y = 0..18: Menu / Puzzle Browser / Piece Count / Print / Quit icons
            if my < 18 {
                return Some(PuzzleAction::OpenMenu);
            }
            // Up scroll arrow (`y = 18..30`)
            if my < 30 {
                if self.scroll_tray(-1) {
                    audio.play_common(CommonSound::UiClick);
                }
                return None;
            }
            // Down scroll arrow (`y = 186..200`)
            if my >= 186 {
                if self.scroll_tray(1) {
                    audio.play_common(CommonSound::UiClick);
                }
                return None;
            }
            // Tray piece slots (`y = 30..186`)
            let vis = self.visible_tray_slots();
            let slot_h = self.tray_slot_height();
            let slot = (((my - 30) / slot_h.max(1)) as usize).min(vis - 1);
            let tray_pos = self.tray_scroll + slot;
            if let Some(held) = self.held_piece.take() {
                // Return held piece (or connected group) to tray
                let insert_at = tray_pos.min(self.tray_order.len());
                if self.held_group.len() > 1 {
                    let group: Vec<usize> = self.held_group.drain(..).map(|(p, _, _)| p).collect();
                    self.held_group_anchor = None;
                    for (offset, gp) in group.into_iter().enumerate() {
                        self.tray_order
                            .insert((insert_at + offset).min(self.tray_order.len()), gp);
                    }
                } else {
                    self.held_group.clear();
                    self.held_group_anchor = None;
                    self.tray_order.insert(insert_at, held);
                }
                audio.play_common(CommonSound::ReturnPiece);
                self.bump_tray();
            } else if tray_pos < self.tray_order.len() {
                let pidx = self.tray_order.remove(tray_pos);
                self.held_piece = Some(pidx);
                self.held_group.clear();
                self.held_group_anchor = None;
                if self.tray_scroll >= self.tray_order.len() {
                    self.clamp_tray_scroll();
                }
                audio.play_common(CommonSound::ChoosePiece);
                self.bump_tray();
            }
            return None;
        }

        // Click on the playfield (`mx >= playfield_x()`)
        let pfx = self.playfield_x();
        let pfy = self.playfield_y();
        if let Some(pidx) = self.held_piece {
            if self.grid_dim > 1 && self.piece_edges.len() == self.pieces.len() {
                let px = mx - pfx;
                let py = my - pfy;
                if let Some((target_r, target_c)) = self.grid_cell_at(px, py) {
                    if self.held_group.len() > 1 {
                        if self.try_place_held_group(target_r, target_c, audio) {
                            return None;
                        }
                    } else if self.can_place_piece_at(pidx, target_r, target_c) {
                        let cell = target_r * self.grid_dim + target_c;
                        self.board_cells[cell] = Some(pidx);
                        self.pieces[pidx].placed = true;
                        self.held_piece = None;
                        self.held_group.clear();
                        self.held_group_anchor = None;
                        self.hint_piece = None;
                        self.bump_board();
                        self.check_grid_completion(audio);
                        return None;
                    }
                }
                audio.play_error();
            } else {
                let piece = &self.pieces[pidx];
                let target_cx = pfx + piece.base_x + piece.base_w / 2;
                let target_cy = pfy + piece.base_y + piece.base_h / 2;
                let snap_tol_x = (piece.base_w / 2).max(12);
                let snap_tol_y = (piece.base_h / 2).max(12);

                if (mx - target_cx).abs() <= snap_tol_x && (my - target_cy).abs() <= snap_tol_y {
                    self.pieces[pidx].placed = true;
                    self.held_piece = None;
                    self.hint_piece = None;
                    self.bump_board();
                    if self.pieces.iter().all(|p| p.placed) {
                        self.on_puzzle_completed(audio);
                    }
                } else {
                    audio.play_error();
                }
            }
        } else if mx < 16 && my < 16 && !self.toolbox_open {
            self.toolbox_open = true;
            self.bump_board();
        } else if self.grid_dim > 1
            && let Some((r, c)) = self.grid_cell_at(mx - pfx, my - pfy)
            && let Some(placed_idx) = self
                .board_cells
                .get(r * self.grid_dim + c)
                .copied()
                .flatten()
        {
            // Pick up placed piece from board (`FUN_1e59_14b6`)
            self.board_cells[r * self.grid_dim + c] = None;
            self.pieces[placed_idx].placed = false;
            self.held_piece = Some(placed_idx);
            self.held_group.clear();
            self.held_group_anchor = None;
            audio.play_common(CommonSound::ChoosePiece);
            self.bump_board();
        }
        None
    }

    pub fn has_any_placed_piece(&self) -> bool {
        self.pieces.iter().any(|p| p.placed)
    }

    pub fn has_unfinished_progress(&self) -> bool {
        self.has_any_placed_piece() || !self.held_group.is_empty()
    }

    /// Playfield-relative draw origin `(x, y)` for a placed piece, accounting for non-target grid placement.
    pub fn piece_placed_origin(&self, piece_idx: usize) -> Option<(i32, i32)> {
        let piece = self.pieces.get(piece_idx)?;
        if !piece.placed {
            return None;
        }
        if self.grid_dim <= 1 || self.board_cells.len() != self.grid_dim * self.grid_dim {
            return Some((piece.target_x, piece.target_y));
        }
        let cell_idx = self
            .board_cells
            .iter()
            .position(|&slot| slot == Some(piece_idx))
            .unwrap_or(piece_idx);
        let dim = self.grid_dim;
        let cell_r = cell_idx / dim;
        let cell_c = cell_idx % dim;
        let col_x: i32 = grid_col_widths(dim)[..cell_c].iter().sum();
        let row_y: i32 = grid_row_heights(dim)[..cell_r].iter().sum();
        Some((
            piece.target_x + col_x - piece.base_x,
            piece.target_y + row_y - piece.base_y,
        ))
    }

    /// Returns `(piece_idx, cell_dx, cell_dy)` for each piece in the currently held group relative to `held_piece`,
    /// using the destination anchor cell under `(pointer_x, pointer_y)` when valid on the grid.
    pub fn held_group_offsets_at(&self, pointer_x: i32, pointer_y: i32) -> Vec<(usize, i32, i32)> {
        let Some(anchor_idx) = self.held_piece else {
            return Vec::new();
        };
        if self.held_group.len() <= 1 || self.grid_dim <= 1 {
            return vec![(anchor_idx, 0, 0)];
        }
        let dim_i32 = self.grid_dim as i32;
        let dest_anchor = self
            .grid_cell_at(
                pointer_x - self.playfield_x(),
                pointer_y - self.playfield_y(),
            )
            .filter(|&(dr_cell, dc_cell)| {
                self.held_group.iter().all(|&(_, dr, dc)| {
                    (0..dim_i32).contains(&(dr_cell as i32 + dr))
                        && (0..dim_i32).contains(&(dc_cell as i32 + dc))
                })
            });
        let Some((anchor_r, anchor_c)) = dest_anchor.or(self.held_group_anchor) else {
            return vec![(anchor_idx, 0, 0)];
        };
        let dim = self.grid_dim;
        let col_w = grid_col_widths(dim);
        let row_h = grid_row_heights(dim);
        let mut col_x = [0i32; 9];
        let mut row_y = [0i32; 9];
        for i in 0..dim {
            col_x[i + 1] = col_x[i] + col_w[i];
            row_y[i + 1] = row_y[i] + row_h[i];
        }
        self.held_group
            .iter()
            .map(|&(pidx, dr, dc)| {
                let nr = (anchor_r as i32 + dr) as usize;
                let nc = (anchor_c as i32 + dc) as usize;
                (
                    pidx,
                    col_x[nc] - col_x[anchor_c],
                    row_y[nr] - row_y[anchor_r],
                )
            })
            .collect()
    }

    /// Right-click on a placed grid piece picks up its connected edge-matching group (`FUN_1e59_125f` + `FUN_1e59_2523`).
    /// Returns `true` if a group was picked up.
    pub fn handle_right_click(&mut self, mx: i32, my: i32, audio: &mut AudioEngine) -> bool {
        if self.solved || self.held_piece.is_some() || self.grid_dim <= 1 {
            return false;
        }
        if self.toolbox_open && mx < TOOLBOX_W {
            return false;
        }
        let pfx = self.playfield_x();
        let pfy = self.playfield_y();
        let Some((start_r, start_c)) = self.grid_cell_at(mx - pfx, my - pfy) else {
            return false;
        };
        let dim = self.grid_dim;
        let Some(anchor_pidx) = self
            .board_cells
            .get(start_r * dim + start_c)
            .copied()
            .flatten()
        else {
            return false;
        };

        // Flood-fill connected placed pieces (`FUN_1e59_125f`)
        let mut visited = vec![false; dim * dim];
        let mut queue = vec![(start_r, start_c)];
        visited[start_r * dim + start_c] = true;
        let mut group = Vec::new();
        let mut head = 0;
        while head < queue.len() {
            let (r, c) = queue[head];
            head += 1;
            let idx = r * dim + c;
            if let Some(pidx) = self.board_cells[idx] {
                group.push((pidx, r as i32 - start_r as i32, c as i32 - start_c as i32));
                let neighbors = [
                    (r > 0).then(|| (r - 1, c)),
                    (r + 1 < dim).then(|| (r + 1, c)),
                    (c > 0).then(|| (r, c - 1)),
                    (c + 1 < dim).then(|| (r, c + 1)),
                ];
                for (nr, nc) in neighbors.into_iter().flatten() {
                    let nidx = nr * dim + nc;
                    if !visited[nidx] && self.board_cells[nidx].is_some() {
                        visited[nidx] = true;
                        queue.push((nr, nc));
                    }
                }
            }
        }

        for &(_, dr, dc) in &group {
            let r = (start_r as i32 + dr) as usize;
            let c = (start_c as i32 + dc) as usize;
            if let Some(pidx) = self.board_cells[r * dim + c].take() {
                self.pieces[pidx].placed = false;
            }
        }
        self.held_piece = Some(anchor_pidx);
        self.held_group = group;
        self.held_group_anchor = Some((start_r, start_c));
        audio.play_common(CommonSound::ChoosePiece);
        self.bump_board();
        true
    }

    fn grid_cell_at(&self, px: i32, py: i32) -> Option<(usize, usize)> {
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

    fn can_place_piece_at(&self, pidx: usize, r: usize, c: usize) -> bool {
        let dim = self.grid_dim;
        if r >= dim || c >= dim || self.board_cells[r * dim + c].is_some() {
            return false;
        }
        let edges = self.piece_edges[pidx];
        // Outer borders must be flat (`None`), internal boundaries must be non-flat (`Some`) and match occupied neighbors (`FUN_1e59_266f`)
        if c == 0 {
            if edges.left.is_some() {
                return false;
            }
        } else {
            if edges.left.is_none() {
                return false;
            }
            if let Some(np) = self.board_cells[r * dim + (c - 1)]
                && edges.left != self.piece_edges[np].right
            {
                return false;
            }
        }
        if c + 1 == dim {
            if edges.right.is_some() {
                return false;
            }
        } else {
            if edges.right.is_none() {
                return false;
            }
            if let Some(np) = self.board_cells[r * dim + (c + 1)]
                && edges.right != self.piece_edges[np].left
            {
                return false;
            }
        }
        if r == 0 {
            if edges.top.is_some() {
                return false;
            }
        } else {
            if edges.top.is_none() {
                return false;
            }
            if let Some(np) = self.board_cells[(r - 1) * dim + c]
                && edges.top != self.piece_edges[np].bottom
            {
                return false;
            }
        }
        if r + 1 == dim {
            if edges.bottom.is_some() {
                return false;
            }
        } else {
            if edges.bottom.is_none() {
                return false;
            }
            if let Some(np) = self.board_cells[(r + 1) * dim + c]
                && edges.bottom != self.piece_edges[np].top
            {
                return false;
            }
        }
        true
    }

    fn try_place_held_group(
        &mut self,
        target_r: usize,
        target_c: usize,
        audio: &mut AudioEngine,
    ) -> bool {
        let dim = self.grid_dim as i32;
        let mut placements = Vec::with_capacity(self.held_group.len());
        for &(pidx, dr, dc) in &self.held_group {
            let nr = target_r as i32 + dr;
            let nc = target_c as i32 + dc;
            if !(0..dim).contains(&nr) || !(0..dim).contains(&nc) {
                return false;
            }
            let ur = nr as usize;
            let uc = nc as usize;
            if !self.can_place_piece_at(pidx, ur, uc) {
                return false;
            }
            placements.push((pidx, ur, uc));
        }
        for (pidx, ur, uc) in placements {
            self.board_cells[ur * self.grid_dim + uc] = Some(pidx);
            self.pieces[pidx].placed = true;
        }
        self.held_piece = None;
        self.held_group.clear();
        self.held_group_anchor = None;
        self.hint_piece = None;
        self.bump_board();
        self.check_grid_completion(audio);
        true
    }

    fn first_misplaced_piece(&self) -> Option<usize> {
        if self.grid_dim <= 1 || self.board_cells.len() != self.pieces.len() {
            return None;
        }
        self.board_cells
            .iter()
            .enumerate()
            .find_map(|(cell, slot)| match *slot {
                Some(pidx) if pidx != cell => Some(pidx),
                _ => None,
            })
    }

    fn check_grid_completion(&mut self, audio: &mut AudioEngine) {
        if !self.pieces.iter().all(|p| p.placed) {
            return;
        }
        let all_home = self
            .board_cells
            .iter()
            .enumerate()
            .all(|(cell, slot)| *slot == Some(cell));
        if all_home {
            self.on_puzzle_completed(audio);
        } else {
            audio.play_common(CommonSound::NotDoneYet);
            self.status_message = Some((
                "Not finished yet. Push F1 for a hint or F2 to finish.".to_string(),
                5.0,
            ));
            self.bump_visual();
        }
    }

    /// Exports the puzzle's `.PRT` monochrome line-art image as a portable PBM image in `prints/`.
    pub fn export_printable_picture(&mut self, game_dir: &Path) {
        let msg = match export_print_pbm(game_dir, self.puzzle_num) {
            Ok(ok_msg) => ok_msg,
            Err(err_msg) => err_msg,
        };
        self.status_message = Some((msg, 3.0));
        self.bump_visual();
    }

    /// Returns the interactive Toolbox control under `(mouse_x, mouse_y)`, if any.
    pub fn toolbox_target_at(&self, mouse_x: i32, mouse_y: i32) -> Option<ToolboxTarget> {
        if self.solved || self.active_anim.is_some() {
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
        if slot < vis && self.tray_scroll + slot < self.tray_order.len() {
            Some(ToolboxTarget::TraySlot(slot))
        } else {
            None
        }
    }

    pub fn hovered_tray_slot(&self, mouse_x: i32, mouse_y: i32) -> Option<usize> {
        match self.toolbox_target_at(mouse_x, mouse_y) {
            Some(ToolboxTarget::TraySlot(slot)) => Some(slot),
            _ => None,
        }
    }

    pub fn present_key(&self, mouse_x: i32, mouse_y: i32, picture_guide: bool) -> PuzzlePresentKey {
        let pointer_state = if let Some(ref anim) = self.active_anim {
            let dissolve_step = if anim.transition_mode == 8 {
                dissolve_visible_step(self.asset.uses_ega(), dissolve_progress(anim))
            } else {
                0
            };
            PointerVisualState::Anim { dissolve_step }
        } else if self.solved {
            PointerVisualState::Solved
        } else {
            let (px, py) = self.active_pointer(mouse_x, mouse_y);
            PointerVisualState::Board {
                hovered_toolbox: self.toolbox_target_at(px, py),
                pressed_toolbox: self.pressed_toolbox_target,
                held_piece_pos: self.held_piece.map(|p| (p, px, py)),
            }
        };
        PuzzlePresentKey {
            session_id: self.session_id,
            asset_rev: self.asset_rev,
            visual_rev: self.visual_rev,
            picture_guide: !self.solved && self.active_anim.is_none() && picture_guide,
            pointer_state,
        }
    }

    #[cfg(test)]
    pub(crate) fn set_piece_placed_for_test(&mut self, idx: usize, placed: bool) {
        if self.pieces[idx].placed != placed {
            self.pieces[idx].placed = placed;
            self.bump_board();
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PuzzleAction {
    OpenMenu,
}

#[cfg(test)]
#[path = "puzzle_tests.rs"]
mod tests;
