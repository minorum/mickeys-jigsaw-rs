//! Jigsaw Puzzle Engine (`seg_0e59`, `seg_0b43`):
//! - Exact 7 difficulty grids (`2x2=4` up to `8x8=64` pieces) over the `270 x 196` playfield
//! - Original interlocking tab/blank polygon curves (`DS:0x4B86` & `DS:0x4B94`) + straight-edge mode
//! - Piece cutting, tray/toolbox pagination, drag-and-drop, snapping, F1 hint, F2 auto-solve
//! - Completion & interactive hotspot animations (`FILM`, `F_ALL`, `F_MICKEY`, etc.)
//! - `.PRT` monochrome line-art export

use std::fs;
use std::path::Path;

use crate::assets::{
    FontAsset, FrameCommand, PrintAsset, PuzzleAsset, SceneRecord, SequencePlayback, Sprite8061,
    ToolboxAsset,
};
use crate::audio::{AudioEngine, CommonSound};
use crate::graphics::{
    Framebuffer, SCENE_OFFSET_X, SCREEN_H, compute_frame_bounds, copy_scene_interior,
    hit_test_frame, render_scene_dissolve_with_scratch, render_scene_frame,
    render_scene_view_with_scratch,
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

#[derive(Clone, Copy, Debug)]
struct UiColors {
    white: u8,
    black: u8,
    gray: u8,
    card: u8,
    hover: u8,
    yellow: u8,
    red: u8,
    board_bg: u8,
}

impl UiColors {
    fn from_framebuffer(fb: &Framebuffer) -> Self {
        Self {
            white: fb.color_index([255; 3]),
            black: fb.color_index([0; 3]),
            gray: fb.color_index([48; 3]),
            card: fb.color_index([85, 85, 85]),
            hover: fb.color_index([170, 170, 170]),
            yellow: fb.color_index([255, 255, 85]),
            red: fb.color_index([255, 0, 0]),
            board_bg: fb.color_index([204, 204, 187]),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct BoardCacheKey {
    toolbox_open: bool,
    grid_lines: bool,
    placed_bits: u128,
}

struct RenderCache {
    scratch_fb: Box<Framebuffer>,
    dissolve_next_fb: Box<Framebuffer>,
    placed_mask: Option<(u128, Vec<bool>)>,
    board_fb: Option<(BoardCacheKey, Box<Framebuffer>)>,
}

struct BoardDrawParams<'a> {
    asset: &'a PuzzleAsset,
    tbx: &'a ToolboxAsset,
    pieces: &'a [JigsawPiece],
    grid_dim: usize,
    grid_polylines_v: &'a [Vec<(i32, i32)>],
    grid_polylines_h: &'a [Vec<(i32, i32)>],
    colors: UiColors,
    pfx: i32,
    pfy: i32,
}

impl RenderCache {
    fn new() -> Box<Self> {
        Box::new(Self {
            scratch_fb: Box::new(Framebuffer::new()),
            dissolve_next_fb: Box::new(Framebuffer::new()),
            placed_mask: None,
            board_fb: None,
        })
    }

    fn invalidate(&mut self) {
        self.placed_mask = None;
        self.board_fb = None;
    }

    fn ensure_placed_mask(&mut self, bits: u128, pieces: &[JigsawPiece]) -> &[bool] {
        let rebuild = self
            .placed_mask
            .as_ref()
            .is_none_or(|(cached_bits, _)| *cached_bits != bits);
        if rebuild {
            let mut placed_cov = vec![false; (PLAYFIELD_W * PLAYFIELD_H) as usize];
            for piece in pieces.iter().filter(|piece| piece.placed) {
                let spr = &piece.sprite;
                let ox = piece.target_x;
                let oy = piece.target_y;
                let w = spr.width as i32;
                let h = spr.sub_h as i32;
                let dy_start = (-oy).max(0);
                let dy_end = (PLAYFIELD_H - oy).min(h);
                let dx_start = (-ox).max(0);
                let dx_end = (PLAYFIELD_W - ox).min(w);
                for dy in dy_start..dy_end {
                    let sy = spr.sub_y as usize + dy as usize;
                    let row_off = sy * spr.stride as usize + spr.sub_x as usize;
                    let cov_row = ((oy + dy) * PLAYFIELD_W) as usize;
                    for dx in dx_start..dx_end {
                        if spr.is_opaque(row_off + dx as usize) {
                            placed_cov[cov_row + (ox + dx) as usize] = true;
                        }
                    }
                }
            }
            self.placed_mask = Some((bits, placed_cov));
        }
        &self.placed_mask.as_ref().unwrap().1
    }

    fn ensure_board_fb(
        &mut self,
        key: BoardCacheKey,
        params: &BoardDrawParams<'_>,
    ) -> &Framebuffer {
        let BoardDrawParams {
            asset,
            tbx,
            pieces,
            grid_dim,
            grid_polylines_v,
            grid_polylines_h,
            colors,
            pfx,
            pfy,
        } = *params;
        let rebuild = self
            .board_fb
            .as_ref()
            .is_none_or(|(cached_key, _)| *cached_key != key);
        if rebuild {
            let mut board = self
                .board_fb
                .take()
                .map(|(_, buf)| buf)
                .unwrap_or_else(|| Box::new(Framebuffer::new()));
            board.palette = asset.palette;
            board.reset_clip();
            board.clear(colors.black);

            // Draw playfield background and decorative border (`FUN_1b43_0363`)
            board.fill_rect(pfx, pfy, PLAYFIELD_W, PLAYFIELD_H, colors.board_bg);
            draw_playfield_border(&mut board, tbx, pfx - 2, PLAYFIELD_W + 2);

            // Draw Grid Lines if enabled (`FUN_1b43_0448`)
            if key.grid_lines {
                board.set_clip(pfx, pfy, PLAYFIELD_W, PLAYFIELD_H);
                for line in grid_polylines_v {
                    for w in line.windows(2) {
                        board.draw_line(
                            pfx + w[0].0,
                            pfy + w[0].1,
                            pfx + w[1].0,
                            pfy + w[1].1,
                            colors.gray,
                        );
                    }
                }
                for line in grid_polylines_h {
                    for w in line.windows(2) {
                        board.draw_line(
                            pfx + w[0].0,
                            pfy + w[0].1,
                            pfx + w[1].0,
                            pfy + w[1].1,
                            colors.gray,
                        );
                    }
                }
                board.reset_clip();
            }

            // Draw all placed pieces onto the playfield
            board.set_clip(pfx, pfy, PLAYFIELD_W, PLAYFIELD_H);
            if grid_dim == 1 {
                if let Some(SceneRecord::Frame {
                    bg_frame_idx,
                    commands,
                    ..
                }) = asset.records.get(asset.preset_frame_idx())
                {
                    let native = &mut self.scratch_fb;
                    native.clear(0);
                    native.set_clip(2, 2, PLAYFIELD_W, PLAYFIELD_H);
                    render_scene_frame(native, asset, *bg_frame_idx, 0, 0, 0, true, 0, 0);
                    for (piece, command) in pieces.iter().zip(
                        commands
                            .iter()
                            .filter(|c| matches!(c, FrameCommand::SubFrame { .. })),
                    ) {
                        if let FrameCommand::SubFrame {
                            dx,
                            dy,
                            frame_idx,
                            flags,
                        } = *command
                        {
                            let flags = if piece.placed {
                                flags & 3
                            } else {
                                (flags & 0xff0f) | 0x20
                            };
                            render_scene_frame(
                                native,
                                asset,
                                frame_idx,
                                i32::from(dx),
                                i32::from(dy),
                                flags,
                                false,
                                0,
                                1,
                            );
                        }
                    }
                    copy_scene_interior(&mut board, native, pfx);
                }
            } else {
                for piece in pieces {
                    if piece.placed {
                        board.blit_sprite(
                            &piece.sprite,
                            pfx + piece.target_x,
                            pfy + piece.target_y,
                            0,
                        );
                    }
                }
            }
            board.reset_clip();
            self.board_fb = Some((key, board));
        }
        &self.board_fb.as_ref().unwrap().1
    }
}

/// Complete active puzzle session state (`seg_0e59`, `seg_0b43`).
pub struct PuzzleSession {
    pub puzzle_num: usize,
    pub asset: PuzzleAsset,
    pub grid_dim: usize,
    pub curved_edges: bool,
    pub toolbox_open: bool,
    pub grid_lines: bool,
    pub timer_enabled: bool,
    pub text_enabled: bool,
    pub elapsed_secs: f64,
    pub pieces: Vec<JigsawPiece>,
    pub tray_order: Vec<usize>,
    pub tray_scroll: usize,
    pub held_piece: Option<usize>,
    pub keyboard_pointer: Option<(f32, f32)>,
    drag_origin: Option<(i32, i32)>,
    pub hint_piece: Option<(usize, f32)>,
    pub solved: bool,
    pub active_anim: Option<AnimationPlayer>,
    pub status_message: Option<(String, f32)>,
    pub full_picture_fb: Framebuffer,
    pub grid_polylines_v: Vec<Vec<(i32, i32)>>,
    pub grid_polylines_h: Vec<Vec<(i32, i32)>>,
    ui_colors: UiColors,
    guide_lut: [[u8; 3]; 256],
    render_cache: Box<RenderCache>,
}

fn build_guide_lut(palette: &[[u8; 3]; 256], board_bg: u8) -> [[u8; 3]; 256] {
    let bg = palette[board_bg as usize];
    let mut lut = [[0u8; 3]; 256];
    for (dst, src) in lut.iter_mut().zip(palette.iter()) {
        for c in 0..3 {
            dst[c] = ((u16::from(bg[c]) * 4 + u16::from(src[c])) / 5) as u8;
        }
    }
    lut
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
        let (full_picture_fb, ui_colors, guide_lut) = Self::render_full_picture(&asset);

        let mut session = Self {
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
            tray_order: Vec::new(),
            tray_scroll: 0,
            held_piece: None,
            keyboard_pointer: None,
            drag_origin: None,
            hint_piece: None,
            solved: false,
            active_anim: None,
            status_message: None,
            full_picture_fb,
            grid_polylines_v: Vec::new(),
            grid_polylines_h: Vec::new(),
            ui_colors,
            guide_lut,
            render_cache: RenderCache::new(),
        };
        session.cut_pieces();
        session
    }

    fn placed_bits(&self) -> u128 {
        debug_assert!(self.pieces.len() <= 128);
        let mut bits = 0u128;
        for (i, piece) in self.pieces.iter().take(128).enumerate() {
            if piece.placed {
                bits |= 1u128 << i;
            }
        }
        bits
    }

    fn render_full_picture(asset: &PuzzleAsset) -> (Framebuffer, UiColors, [[u8; 3]; 256]) {
        let mut full_picture_fb = Framebuffer::new();
        full_picture_fb.palette = asset.palette;
        let static_frame_idx = asset.static_frame_idx();
        render_scene_frame(
            &mut full_picture_fb,
            asset,
            static_frame_idx,
            0,
            0,
            0,
            true,
            0,
            0,
        );
        let ui_colors = UiColors::from_framebuffer(&full_picture_fb);
        let guide_lut = build_guide_lut(&asset.palette, ui_colors.board_bg);
        (full_picture_fb, ui_colors, guide_lut)
    }

    pub fn replace_scene_asset(&mut self, asset: PuzzleAsset) {
        assert_eq!(asset.id, self.puzzle_num);
        let (full_picture_fb, ui_colors, guide_lut) = Self::render_full_picture(&asset);
        let old_placed: Vec<bool> = self.pieces.iter().map(|p| p.placed).collect();
        let old_tray_order = std::mem::take(&mut self.tray_order);
        let old_tray_scroll = self.tray_scroll;
        let old_held = self.held_piece;
        let old_solved = self.solved;

        self.asset = asset;
        self.full_picture_fb = full_picture_fb;
        self.ui_colors = ui_colors;
        self.guide_lut = guide_lut;
        self.cut_pieces();

        if self.pieces.len() == old_placed.len() {
            for (new_piece, placed) in self.pieces.iter_mut().zip(old_placed) {
                new_piece.placed = placed;
            }
            self.tray_order = old_tray_order;
            self.tray_scroll = old_tray_scroll;
            self.clamp_tray_scroll();
            self.held_piece = old_held;
            self.solved = old_solved;
        }
        self.active_anim = None;
        self.drag_origin = None;
        self.render_cache.invalidate();
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

        let mut col_x = vec![0i32; dim + 1];
        for c in 0..dim {
            col_x[c + 1] = col_x[c] + col_w[c];
        }
        let mut row_y = vec![0i32; dim + 1];
        for r in 0..dim {
            row_y[r + 1] = row_y[r] + row_h[r];
        }

        // Deterministic PRNG seeded by puzzle number and grid dimension (`FUN_1239_4386`)
        let mut rng = LcgRng::new(
            (self.puzzle_num as u32)
                .wrapping_mul(1103515245)
                .wrapping_add((dim as u32) * 12345),
        );

        // Shared vertical borders: `v_edges[r][c]` for `r in 0..dim`, `c in 1..dim` -> `(variant 0..3, dir -1 or +1)`
        let mut v_edges = vec![vec![(0usize, 1i32); dim + 1]; dim];
        for row in &mut v_edges {
            for edge in row.iter_mut().take(dim).skip(1) {
                let code = rng.next() & 7;
                let variant = code & 3;
                let dir = if code < 4 { 1 } else { -1 };
                *edge = (variant, dir);
            }
        }

        // Shared horizontal borders: `h_edges[r][c]` for `r in 1..dim`, `c in 0..dim` -> `(variant 0..3, dir -1 or +1)`
        let mut h_edges = vec![vec![(0usize, 1i32); dim]; dim + 1];
        for row in h_edges.iter_mut().take(dim).skip(1) {
            for edge in row {
                let code = rng.next() & 7;
                let variant = code & 3;
                let dir = if code < 4 { 1 } else { -1 };
                *edge = (variant, dir);
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
                    let (variant, dir) = v_edges[r][c];
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
                    let (variant, dir) = h_edges[r][c];
                    let (_, pts) = horiz_edge_curve(dim, variant);
                    for &(dx, dy) in pts {
                        line.push((bx + (dx as i32), by + (dy as i32) * dir));
                    }
                }
                line.push((col_x[c + 1], by));
            }
            self.grid_polylines_h.push(line);
        }

        // Cut each piece using an offscreen mask framebuffer (`FUN_1e59_011c`)
        let mut pieces = Vec::with_capacity(dim * dim);
        let mut mask_fb = Framebuffer::new();
        let mut poly = Vec::with_capacity(48);
        // Source picture uses playfield coordinates `2..272, 2..198` (or centered `(25, 2)`)
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
                    (c > 0, true, v_edges[r][c], bx, by, -1),
                    (c + 1 < dim, true, v_edges[r][c + 1], bx + bw, by, 1),
                    (r > 0, false, h_edges[r][c], bx, by, -1),
                    (r + 1 < dim, false, h_edges[r + 1][c], bx, by + bh, 1),
                ];

                let mut pads = [0i32; 4];
                if self.curved_edges {
                    for (i, &(active, is_vert, (var, dir), _, _, out_dir)) in
                        borders.iter().enumerate()
                    {
                        if active && dir == out_dir {
                            let (ext, _) = if is_vert {
                                vert_edge_curve(dim, var)
                            } else {
                                horiz_edge_curve(dim, var)
                            };
                            pads[i] = ext;
                        }
                    }
                }
                let [pad_left, pad_right, pad_top, pad_bottom] = pads;

                let tx = bx - pad_left;
                let ty = by - pad_top;
                let tw = bw + pad_left + pad_right;
                let th = bh + pad_top + pad_bottom;

                // Build mask in `mask_fb` (`1` = inside piece, `0` = transparent)
                mask_fb.clear(0);
                mask_fb.fill_rect(bx, by, bw, bh, 1);

                if self.curved_edges {
                    for &(active, is_vert, (var, dir), ex, ey, out_dir) in &borders {
                        if !active {
                            continue;
                        }
                        let (_, pts) = if is_vert {
                            vert_edge_curve(dim, var)
                        } else {
                            horiz_edge_curve(dim, var)
                        };
                        poly.clear();
                        poly.extend(pts.iter().map(|&(dx, dy)| {
                            if is_vert {
                                (ex + (dx as i32) * dir, ey + (dy as i32))
                            } else {
                                (ex + (dx as i32), ey + (dy as i32) * dir)
                            }
                        }));
                        let fill_val = u8::from(dir == out_dir);
                        mask_fb.fill_polygon(&poly, fill_val);
                    }
                }

                let trans = 0xF8u8;
                let mut px_buf = vec![trans; (tw * th) as usize];
                let mut opacity = vec![false; px_buf.len()];
                for dy in 0..th {
                    let py = ty + dy;
                    for dx in 0..tw {
                        let px = tx + dx;
                        if mask_fb.get_pixel(px, py) != 0 {
                            let col = self.full_picture_fb.get_pixel(src_ox + px, src_oy + py);
                            px_buf[(dy * tw + dx) as usize] = col;
                            opacity[(dy * tw + dx) as usize] = true;
                        }
                    }
                }

                let sprite = Sprite8061 {
                    stride: tw as u16,
                    height: th as u16,
                    transparent_color: trans,
                    bpp: 8,
                    sub_x: 0,
                    sub_y: 0,
                    width: tw as u16,
                    sub_h: th as u16,
                    pixels: px_buf,
                    opacity: Some(opacity),
                };

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
            }
        }

        // Shuffle pieces into the tray (`FUN_1e59_0b0a`)
        let tray_order = rng.shuffled_indices(pieces.len());

        self.pieces = pieces;
        self.tray_order = tray_order;
        self.tray_scroll = 0;
        self.held_piece = None;
        self.drag_origin = None;
        self.solved = false;
    }

    fn cut_preset_pieces(&mut self) {
        let mut pieces = Vec::new();
        let mut pixels = Framebuffer::new();
        let mut mask = Framebuffer::new();
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
                pixels.clear(0);
                pixels.set_clip(2, 2, PLAYFIELD_W, PLAYFIELD_H);
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
                mask.clear(0);
                mask.set_clip(2, 2, PLAYFIELD_W, PLAYFIELD_H);
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
                    for px in left..left + width {
                        colors.push(pixels.get_pixel(px, py));
                        opacity.push(mask.get_pixel(px, py) != 0);
                    }
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
                    sprite: Sprite8061 {
                        stride: width as u16,
                        height: height as u16,
                        transparent_color: 0,
                        bpp: 8,
                        sub_x: 0,
                        sub_y: 0,
                        width: width as u16,
                        sub_h: height as u16,
                        pixels: colors,
                        opacity: Some(opacity),
                    },
                    placed: false,
                });
            }
        }
        let mut rng = LcgRng::new(
            (self.puzzle_num as u32)
                .wrapping_mul(1103515245)
                .wrapping_add(12345),
        );
        let order = rng.shuffled_indices(pieces.len());
        self.pieces = pieces;
        self.tray_order = order;
        self.tray_scroll = 0;
        self.held_piece = None;
        self.drag_origin = None;
        self.solved = false;
        self.grid_polylines_v.clear();
        self.grid_polylines_h.clear();
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

    fn tray_slot_height(&self) -> i32 {
        (156 / self.visible_tray_slots().max(1)) as i32
    }

    fn clamp_tray_scroll(&mut self) {
        let max_scroll = self
            .tray_order
            .len()
            .saturating_sub(self.visible_tray_slots());
        self.tray_scroll = self.tray_scroll.min(max_scroll);
    }

    pub fn scroll_tray(&mut self, delta: i32) {
        if delta < 0 {
            self.tray_scroll = self
                .tray_scroll
                .saturating_sub(delta.unsigned_abs() as usize);
        } else if delta > 0 {
            self.tray_scroll = self.tray_scroll.saturating_add(delta as usize);
            self.clamp_tray_scroll();
        }
    }

    /// Piece hint.
    pub fn trigger_hint(&mut self) {
        if self.solved {
            return;
        }
        let target_idx = self.held_piece.or_else(|| self.tray_order.first().copied());
        if let Some(pidx) = target_idx {
            self.hint_piece = Some((pidx, 1.5));
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
        self.tray_order.clear();
        self.held_piece = None;
        self.hint_piece = None;
        self.on_puzzle_completed(audio);
    }

    pub fn on_puzzle_completed(&mut self, audio: &mut AudioEngine) {
        audio.stop_all();
        self.solved = true;
        self.toolbox_open = false;
        self.start_sequence(self.default_playback(), audio);
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
        if let Some(seq) = self.asset.sequences.get(seq_idx)
            && seq.start_frame < seq.end_frame
        {
            self.active_anim = Some(AnimationPlayer {
                seq_idx,
                start_frame: seq.start_frame,
                previous_frame: self.asset.static_frame_idx(),
                cur_frame: seq.start_frame,
                end_frame: seq.end_frame,
                tick_accum: 0.0,
                frame_delay_secs: default_delay_secs,
                repetitions_left: playback.repetitions.max(1),
                default_delay_secs,
                transition_mode: 0,
                bubble: None,
                wait_for_audio: false,
                looping_sound: None,
            });
            self.trigger_frame_side_effects(seq.start_frame, audio);
        }
    }

    fn trigger_frame_side_effects(&mut self, frame_idx: usize, audio: &mut AudioEngine) {
        let mut delay_ticks = None;
        let mut sound_to_play = None;
        if let Some(anim) = &mut self.active_anim {
            anim.wait_for_audio = false;
            anim.frame_delay_secs = anim.default_delay_secs;
            anim.transition_mode = 0;
        }

        if let Some(SceneRecord::Frame { commands, .. }) = self.asset.records.get(frame_idx) {
            for cmd in commands {
                match *cmd {
                    FrameCommand::Delay { ticks, raw } => {
                        delay_ticks = Some((ticks, (raw & 0x3f) as u8));
                    }
                    FrameCommand::Sound {
                        snd_idx: Some(sidx),
                    } => {
                        sound_to_play = Some(sidx);
                    }
                    _ => {}
                }
            }
        }

        if let Some((ticks, transition)) = delay_ticks
            && let Some(ref mut anim) = self.active_anim
        {
            anim.transition_mode = transition;
            anim.wait_for_audio = ticks == 998;
            if ticks != 0 && ticks != 998 {
                anim.frame_delay_secs = (ticks as f32 + 1.0) * DOS_TICK_SECS;
            }
        }

        if let Some(sidx) = sound_to_play {
            audio.stop_all();
            if let Some(anim) = &mut self.active_anim {
                anim.bubble = None;
                anim.looping_sound = None;
            }
            if let Some(entry) = self.asset.sounds.get_mut(sidx) {
                let mut voice_chosen = false;
                let looping = entry.selection_mode & 0x80 != 0;
                let music = entry.sounds.first();
                let has_music = music
                    .is_some_and(|desc| audio.iss_songs.keys().any(|(stem, _)| stem == &desc.stem));
                if (audio.sfx_enabled || !audio.music_enabled || !has_music)
                    && entry.sounds.len() > 1
                    && let Some(voice) =
                        entry.select_voice(|| macroquad::rand::gen_range(0, 2) != 0)
                {
                    let desc = &entry.sounds[voice + 1];
                    if audio.nnv_banks.contains_key(&desc.stem) {
                        voice_chosen = true;
                        audio.play_nnv(&desc.stem, desc.phrase_idx as usize);
                        if looping && let Some(anim) = &mut self.active_anim {
                            anim.looping_sound = Some((sidx, voice + 1));
                        }
                        if let Some(anim) = self.active_anim.as_mut().filter(|_| self.text_enabled)
                        {
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
                    if looping && let Some(anim) = &mut self.active_anim {
                        anim.looping_sound = Some((sidx, 0));
                    }
                }
            }
        }
        if let Some(anim) = &mut self.active_anim
            && anim.wait_for_audio
            && anim.bubble.is_some()
            && self.text_enabled
            && !audio.is_playing()
        {
            anim.frame_delay_secs = 20.0 * DOS_TICK_SECS;
        }
    }

    pub fn update(&mut self, dt: f32, audio: &mut AudioEngine) {
        let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
        if !self.solved && self.timer_enabled {
            self.elapsed_secs += dt as f64;
        }
        if let Some((pidx, mut rem)) = self.hint_piece {
            rem -= dt;
            if rem <= 0.0 {
                self.hint_piece = None;
            } else {
                self.hint_piece = Some((pidx, rem));
            }
        }
        if let Some((msg, mut rem)) = self.status_message.take() {
            rem -= dt;
            if rem > 0.0 {
                self.status_message = Some((msg, rem));
            }
        }
        if let Some(anim) = self.active_anim.as_mut() {
            anim.tick_accum += dt;
        }
        while let Some(mut anim) = self.active_anim.take() {
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
            self.active_anim = Some(anim);
            self.trigger_frame_side_effects(frame, audio);
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
        for (command_index, command) in commands.iter().enumerate() {
            if let FrameCommand::SubFrame {
                dx,
                dy,
                frame_idx,
                flags,
            } = *command
            {
                let center_hit = hotspots.get(child_index).is_some_and(|hotspot| {
                    mx == hotspot.x as i32 + hotspot.width as i32 / 2
                        && my == hotspot.y as i32 + hotspot.height as i32 / 2
                });
                child_index += 1;
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
                    selected = Some(command_index);
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
        let (x, y) = self.keyboard_pointer.unwrap_or((20.0, 40.0));
        let step = 120.0 * dt.max(0.0) / dx.hypot(dy).max(1.0);
        self.keyboard_pointer = Some((
            (x + dx * step).clamp(0.0, 319.0),
            (y + dy * step).clamp(0.0, 199.0),
        ));
    }

    pub fn handle_keyboard_activate(&mut self, audio: &mut AudioEngine) -> Option<PuzzleAction> {
        self.cancel_pointer_drag();
        let (x, y) = *self.keyboard_pointer.get_or_insert((20.0, 40.0));
        self.handle_click(x.round() as i32, y.round() as i32, audio)
    }

    pub fn toggle_toolbox(&mut self) {
        self.cancel_pointer_drag();
        self.toolbox_open = !self.toolbox_open;
    }

    pub fn handle_pointer_press(
        &mut self,
        mx: i32,
        my: i32,
        audio: &mut AudioEngine,
    ) -> Option<PuzzleAction> {
        self.keyboard_pointer = None;
        self.drag_origin = None;
        let was_holding = self.held_piece.is_some();
        let action = self.handle_click(mx, my, audio);
        if !was_holding && self.held_piece.is_some() {
            self.drag_origin = Some((mx, my));
        }
        action
    }

    pub fn handle_pointer_release(&mut self, mx: i32, my: i32, audio: &mut AudioEngine) {
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
            audio.stop_all();
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
                self.scroll_tray(-1);
                return None;
            }
            // Down scroll arrow (`y = 186..200`)
            if my >= 186 {
                self.scroll_tray(1);
                return None;
            }
            // Tray piece slots (`y = 30..186`)
            let vis = self.visible_tray_slots();
            let slot_h = self.tray_slot_height();
            let slot = (((my - 30) / slot_h.max(1)) as usize).min(vis - 1);
            let tray_pos = self.tray_scroll + slot;
            if let Some(held) = self.held_piece.take() {
                // Return held piece to tray
                let insert_at = tray_pos.min(self.tray_order.len());
                self.tray_order.insert(insert_at, held);
                audio.play_common(CommonSound::ReturnPiece);
            } else if tray_pos < self.tray_order.len() {
                let pidx = self.tray_order.remove(tray_pos);
                self.held_piece = Some(pidx);
                if self.tray_scroll > 0 && self.tray_scroll >= self.tray_order.len() {
                    self.tray_scroll = self.tray_order.len().saturating_sub(1);
                }
                audio.play_common(CommonSound::ChoosePiece);
            }
            return None;
        }

        // Click on the playfield (`mx >= playfield_x()`)
        let pfx = self.playfield_x();
        let pfy = self.playfield_y();
        if let Some(pidx) = self.held_piece {
            let piece = &self.pieces[pidx];
            let target_cx = pfx + piece.base_x + piece.base_w / 2;
            let target_cy = pfy + piece.base_y + piece.base_h / 2;
            let snap_tol_x = (piece.base_w / 2).max(12);
            let snap_tol_y = (piece.base_h / 2).max(12);

            if (mx - target_cx).abs() <= snap_tol_x && (my - target_cy).abs() <= snap_tol_y {
                self.pieces[pidx].placed = true;
                self.held_piece = None;
                self.hint_piece = None;
                if self.pieces.iter().all(|p| p.placed) {
                    self.on_puzzle_completed(audio);
                }
            } else {
                audio.play_error();
            }
        } else if mx < 16 && my < 16 {
            self.toolbox_open = true;
        }
        None
    }

    /// Exports the puzzle's `.PRT` monochrome line-art image as a portable PBM image in `prints/`.
    pub fn export_printable_picture(&mut self, game_dir: &Path) {
        match PrintAsset::load(game_dir, self.puzzle_num) {
            Ok(prt) => {
                let out_dir = game_dir.join("prints");
                let pbm_path = out_dir.join(format!("P{:02}.pbm", self.puzzle_num));
                let mut pbm = Vec::with_capacity(8032);
                pbm.extend_from_slice(b"P4\n320 200\n");
                pbm.extend_from_slice(&prt.bitmap);
                match fs::create_dir_all(&out_dir).and_then(|()| fs::write(&pbm_path, &pbm)) {
                    Ok(()) => {
                        self.status_message = Some((
                            format!(
                                "Saved printable line-art to prints/P{:02}.pbm",
                                self.puzzle_num
                            ),
                            3.0,
                        ));
                    }
                    Err(err) => self.status_message = Some((format!("Print error: {err}"), 3.0)),
                }
            }
            Err(e) => {
                self.status_message = Some((format!("Print error: {}", e), 3.0));
            }
        }
    }

    pub fn blend_picture_guide(&mut self, rgba: &mut [u8], mouse_x: i32, mouse_y: i32) {
        assert_eq!(rgba.len(), 320 * 200 * 4);
        if self.solved || self.grid_dim == 1 {
            return;
        }
        let pfx = self.playfield_x();
        let pfy = self.playfield_y();
        let bits = self.placed_bits();
        let placed_cov = self.render_cache.ensure_placed_mask(bits, &self.pieces);
        let held_bounds = self.held_piece.map(|index| {
            let piece = &self.pieces[index];
            (
                piece,
                mouse_x - piece.base_w / 2 + piece.target_x - piece.base_x - pfx,
                mouse_y - piece.base_h / 2 + piece.target_y - piece.base_y - pfy,
            )
        });

        let background = self.asset.palette[self.ui_colors.board_bg as usize];
        for y in 0..PLAYFIELD_H {
            let cov_row = (y * PLAYFIELD_W) as usize;
            let fb_row = ((2 + y) as usize) * 320 + 2;
            let rgba_row = ((pfy + y) as usize) * 320 + pfx as usize;
            let held_row = held_bounds.and_then(|(piece, hx, hy)| {
                let dy = y - hy;
                if (0..piece.sprite.sub_h as i32).contains(&dy) {
                    let row_off = (piece.sprite.sub_y as usize + dy as usize)
                        * piece.sprite.stride as usize
                        + piece.sprite.sub_x as usize;
                    Some((
                        &piece.sprite.pixels,
                        piece.sprite.opacity.as_deref(),
                        piece.sprite.transparent_color,
                        hx,
                        piece.sprite.width as i32,
                        row_off,
                    ))
                } else {
                    None
                }
            });
            for x in 0..PLAYFIELD_W as usize {
                let offset = (rgba_row + x) * 4;
                if placed_cov[cov_row + x]
                    || rgba[offset] != background[0]
                    || rgba[offset + 1] != background[1]
                    || rgba[offset + 2] != background[2]
                {
                    continue;
                }
                if let Some((pixels, opacity, trans, hx, hw, row_off)) = held_row {
                    let dx = x as i32 - hx;
                    if (0..hw).contains(&dx) {
                        let idx = row_off + dx as usize;
                        let opaque = opacity.map_or(pixels[idx] != trans, |mask| mask[idx]);
                        if opaque {
                            continue;
                        }
                    }
                }
                let blended = self.guide_lut[self.full_picture_fb.pixels[fb_row + x] as usize];
                rgba[offset] = blended[0];
                rgba[offset + 1] = blended[1];
                rgba[offset + 2] = blended[2];
            }
        }
    }

    /// Renders the static full-picture reference view (`Tab` / Reference button).
    pub fn render_reference(&self, fb: &mut Framebuffer) {
        fb.palette = self.asset.palette;
        fb.reset_clip();
        fb.clear(self.ui_colors.black);
        copy_scene_interior(fb, &self.full_picture_fb, SCENE_OFFSET_X + 2);
    }

    /// Renders the puzzle screen.
    pub fn render(
        &mut self,
        fb: &mut Framebuffer,
        tbx: &ToolboxAsset,
        font: &FontAsset,
        mouse_x: i32,
        mouse_y: i32,
    ) {
        fb.reset_clip();
        fb.palette = self.asset.palette;
        let colors = self.ui_colors;

        // If an animation sequence is currently playing (`FILM` / `F_*`), render that frame
        if let Some(ref anim) = self.active_anim {
            let cache = &mut *self.render_cache;
            if anim.transition_mode == 8 {
                let duration = (anim.frame_delay_secs * 0.5).min(4.0 * DOS_TICK_SECS);
                let progress = if duration > 0.0 {
                    anim.tick_accum / duration
                } else {
                    1.0
                };
                render_scene_dissolve_with_scratch(
                    fb,
                    &mut cache.scratch_fb,
                    &mut cache.dissolve_next_fb,
                    &self.asset,
                    anim.previous_frame,
                    anim.cur_frame,
                    progress,
                );
            } else {
                render_scene_view_with_scratch(
                    fb,
                    &mut cache.scratch_fb,
                    &self.asset,
                    anim.cur_frame,
                );
            }
            draw_playfield_border(fb, tbx, SCENE_OFFSET_X, PLAYFIELD_W + 2);
            if let Some((sound, voice)) = anim.bubble.filter(|_| self.text_enabled)
                && let Some(Some(bubble)) = self.asset.sounds[sound].bubbles.get(voice)
            {
                let (fill, ink) = if self.asset.uses_ega() {
                    (15, 1)
                } else {
                    (248, 16)
                };
                fb.draw_speech_bubble_offset(bubble, font, SCENE_OFFSET_X, 0, fill, ink);
            }
            return;
        }

        // If puzzle is solved and idle, show the static completed scene (`c7c4[3]`)
        if self.solved {
            fb.clear(colors.black);
            copy_scene_interior(fb, &self.full_picture_fb, SCENE_OFFSET_X + 2);
            draw_playfield_border(fb, tbx, SCENE_OFFSET_X, PLAYFIELD_W + 2);
            // Draw subtle menu tab icon in top-left corner (`tbx.sprites[49]` or outline)
            if let Some(icon) = tbx.sprites.get(49) {
                fb.blit_sprite(icon, 2, 2, 0);
            }
            if let Some((ref status, _)) = self.status_message {
                fb.draw_text_shadowed(font, status, 40, 6, colors.black, colors.yellow);
            }
            return;
        }

        let pfx = self.playfield_x();
        let pfy = self.playfield_y();
        let board_key = BoardCacheKey {
            toolbox_open: self.toolbox_open,
            grid_lines: self.grid_lines,
            placed_bits: self.placed_bits(),
        };
        let board = self.render_cache.ensure_board_fb(
            board_key,
            &BoardDrawParams {
                asset: &self.asset,
                tbx,
                pieces: &self.pieces,
                grid_dim: self.grid_dim,
                grid_polylines_v: &self.grid_polylines_v,
                grid_polylines_h: &self.grid_polylines_h,
                colors,
                pfx,
                pfy,
            },
        );
        fb.pixels.copy_from_slice(&board.pixels);

        // Draw F1 hint highlight if active
        if let Some((hidx, rem)) = self.hint_piece
            && let Some(piece) = self.pieces.get(hidx)
        {
            fb.set_clip(pfx, pfy, PLAYFIELD_W, PLAYFIELD_H);
            let blink = ((rem * 8.0) as i32) & 1 == 0;
            let col = if blink { colors.white } else { colors.red };
            fb.blit_sprite(
                &piece.sprite,
                pfx + piece.target_x,
                pfy + piece.target_y,
                ((col as u16) << 8) | 0x0020,
            );
            fb.reset_clip();
        }

        if self.toolbox_open {
            self.render_toolbox(fb, font, mouse_x, mouse_y);
        }

        // Draw status message if active
        if let Some((ref status, _)) = self.status_message {
            fb.draw_text_shadowed(font, status, pfx + 6, 186, colors.black, colors.yellow);
        }

        // Draw currently held piece centered on the cursor
        if let Some(pidx) = self.held_piece {
            let piece = &self.pieces[pidx];
            let px = mouse_x - piece.base_w / 2 + piece.target_x - piece.base_x;
            let py = mouse_y - piece.base_h / 2 + piece.target_y - piece.base_y;
            fb.blit_sprite(&piece.sprite, px, py, 0);
        }
    }

    fn render_toolbox(&self, fb: &mut Framebuffer, font: &FontAsset, mouse_x: i32, mouse_y: i32) {
        let colors = self.ui_colors;
        fb.fill_rect(0, 0, TOOLBOX_W - 2, SCREEN_H, colors.black);

        // Top menu button (`y = 2..16`)
        fb.fill_rect(2, 2, TOOLBOX_W - 6, 14, colors.card);
        fb.draw_text(font, "MENU", 8, 4, colors.white);

        // Up arrow (`y = 18..29`)
        fb.fill_rect(2, 18, TOOLBOX_W - 6, 11, colors.card);
        let up_color = if self.tray_scroll > 0 {
            colors.white
        } else {
            colors.black
        };
        for row in 0..4 {
            fb.draw_line(23 - row, 21 + row, 23 + row, 21 + row, up_color);
        }

        // Down arrow (`y = 187..198`)
        fb.fill_rect(2, 187, TOOLBOX_W - 6, 11, colors.card);
        let down_color = if self.tray_scroll + self.visible_tray_slots() < self.tray_order.len() {
            colors.white
        } else {
            colors.black
        };
        for row in 0..4 {
            fb.draw_line(23 - row, 194 - row, 23 + row, 194 - row, down_color);
        }

        // Visible unplaced pieces in tray (`y = 30..186`)
        let vis = self.visible_tray_slots();
        let slot_h = self.tray_slot_height();
        fb.set_clip(2, 30, TOOLBOX_W - 6, 156);
        for slot in 0..vis {
            let tray_idx = self.tray_scroll + slot;
            if let Some(&pidx) = self.tray_order.get(tray_idx) {
                let piece = &self.pieces[pidx];
                let slot_y = 30 + slot as i32 * slot_h;
                let hovered =
                    (2..44).contains(&mouse_x) && (slot_y..slot_y + slot_h).contains(&mouse_y);
                fb.fill_rect(
                    2,
                    slot_y + 1,
                    TOOLBOX_W - 6,
                    slot_h - 2,
                    if hovered { colors.hover } else { colors.card },
                );
                fb.blit_thumbnail(&piece.sprite, 3, slot_y + 2, TOOLBOX_W - 8, slot_h - 4);
            }
        }
        fb.reset_clip();
    }
}

/// Draws the original decorative playfield frame using `MJP.TBX` sprites `0x38..0x3C` (`FUN_1b43_0363`).
pub fn draw_playfield_border(fb: &mut Framebuffer, tbx: &ToolboxAsset, x_left: i32, width: i32) {
    if tbx.sprites.len() > 0x3C {
        let mut y = 0;
        while y < SCREEN_H {
            fb.blit_sprite(&tbx.sprites[0x3A], x_left, y, 0);
            fb.blit_sprite(&tbx.sprites[0x3B], x_left + width, y, 0);
            y += 20;
        }
        let mut dx = 0;
        while dx < width {
            fb.blit_sprite(&tbx.sprites[0x38], x_left + dx, 0, 0);
            fb.blit_sprite(&tbx.sprites[0x39], x_left + dx, 198, 0);
            dx += 34;
        }
        fb.blit_sprite(&tbx.sprites[0x3C], x_left - 1, -2, 0);
        fb.blit_sprite(&tbx.sprites[0x3C], x_left - 1, 198, 0);
        fb.blit_sprite(&tbx.sprites[0x3C], x_left + width, -2, 0);
        fb.blit_sprite(&tbx.sprites[0x3C], x_left + width, 198, 0);
    } else {
        fb.draw_rect_outline(x_left, 0, width + 2, SCREEN_H, 14);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PuzzleAction {
    OpenMenu,
}

#[cfg(test)]
#[path = "puzzle_tests.rs"]
mod tests;
