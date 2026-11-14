//! Jigsaw Puzzle Engine:
//! - Exact 7 difficulty grids (`2x2=4` up to `8x8=64` pieces) over the `270 x 196` playfield
//! - Parametric cubic-Bézier interlocking tab/socket geometry (`bevy_math`) + straight-edge mode
//! - Piece cutting, tray/toolbox pagination, drag-and-drop, snapping, F1 hint, F2 auto-solve
//! - Completion & interactive hotspot animations (`FILM`, `F_ALL`, `F_MICKEY`, etc.)
//! - `.PRT` monochrome line-art export

use std::fs;
use std::path::Path;

use bevy_math::Vec2;

use crate::assets::PrintAsset;
use crate::ecs::components::AnimationState;

pub const PLAYFIELD_W: i32 = 270;
pub const PLAYFIELD_H: i32 = 196;
pub const TOOLBOX_W: i32 = 48;
pub const DOS_TICK_SECS: f32 = 65536.0 / 1193182.0;

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

/// Parametric cubic-Bézier jigsaw edge profile in normalized `(along, perp)` coordinates.
///
/// `along` spans `[0.0, 1.0]` along the shared cell boundary and `perp` is normalized to
/// `[0.0, 1.0]` of the variant's peak tab amplitude. Adjacent pieces reuse the exact same
/// sampled points (reflected by `dir` in `{-1, +1}`) so boundaries fit without gaps or overlaps.
fn sample_canonical_bezier_edge(variant: usize, span: i32) -> (i32, Vec<(i16, i16)>) {
    let span_f = span.max(1) as f32;
    let (amp_px, segments): (f32, Vec<[Vec2; 4]>) = match variant & 3 {
        // Variant 0: Classic centered interlocking jigsaw tab with narrow neck & rounded bulb.
        0 => (
            (span_f * 0.27).round().clamp(5.0, 36.0),
            vec![
                [
                    Vec2::new(0.22, 0.00),
                    Vec2::new(0.32, 0.02),
                    Vec2::new(0.42, 0.14),
                    Vec2::new(0.38, 0.36),
                ],
                [
                    Vec2::new(0.38, 0.36),
                    Vec2::new(0.33, 0.62),
                    Vec2::new(0.38, 1.00),
                    Vec2::new(0.50, 1.00),
                ],
                [
                    Vec2::new(0.50, 1.00),
                    Vec2::new(0.62, 1.00),
                    Vec2::new(0.67, 0.62),
                    Vec2::new(0.62, 0.36),
                ],
                [
                    Vec2::new(0.62, 0.36),
                    Vec2::new(0.58, 0.14),
                    Vec2::new(0.68, 0.02),
                    Vec2::new(0.78, 0.00),
                ],
            ],
        ),
        // Variant 1: Slightly wider, softer-necked interlocking tab.
        1 => (
            (span_f * 0.24).round().clamp(5.0, 33.0),
            vec![
                [
                    Vec2::new(0.16, 0.00),
                    Vec2::new(0.26, 0.06),
                    Vec2::new(0.38, 0.22),
                    Vec2::new(0.36, 0.44),
                ],
                [
                    Vec2::new(0.36, 0.44),
                    Vec2::new(0.34, 0.72),
                    Vec2::new(0.40, 1.00),
                    Vec2::new(0.52, 1.00),
                ],
                [
                    Vec2::new(0.52, 1.00),
                    Vec2::new(0.64, 1.00),
                    Vec2::new(0.68, 0.70),
                    Vec2::new(0.65, 0.44),
                ],
                [
                    Vec2::new(0.65, 0.44),
                    Vec2::new(0.62, 0.20),
                    Vec2::new(0.74, 0.06),
                    Vec2::new(0.84, 0.00),
                ],
            ],
        ),
        // Variant 2: Gentle symmetric wave curve.
        2 => (
            (span_f * 0.08).round().clamp(3.0, 10.0),
            vec![
                [
                    Vec2::new(0.05, 0.00),
                    Vec2::new(0.20, 0.45),
                    Vec2::new(0.35, 1.00),
                    Vec2::new(0.50, 1.00),
                ],
                [
                    Vec2::new(0.50, 1.00),
                    Vec2::new(0.65, 1.00),
                    Vec2::new(0.80, 0.45),
                    Vec2::new(0.95, 0.00),
                ],
            ],
        ),
        // Variant 3: Skewed organic wave curve.
        _ => (
            (span_f * 0.075).round().clamp(3.0, 9.0),
            vec![
                [
                    Vec2::new(0.06, 0.00),
                    Vec2::new(0.18, 0.25),
                    Vec2::new(0.28, 1.00),
                    Vec2::new(0.44, 1.00),
                ],
                [
                    Vec2::new(0.44, 1.00),
                    Vec2::new(0.62, 1.00),
                    Vec2::new(0.76, 0.35),
                    Vec2::new(0.92, 0.00),
                ],
            ],
        ),
    };

    let samples_per_seg = 6;
    let mut pts: Vec<(i16, i16)> = Vec::with_capacity(segments.len() * samples_per_seg + 3);

    for (seg_idx, &ctrl) in segments.iter().enumerate() {
        let start = if seg_idx == 0 { 0 } else { 1 };
        for s in start..=samples_per_seg {
            let t = s as f32 / samples_per_seg as f32;
            let u_f = 1.0 - t;
            let p = ctrl[0] * (u_f * u_f * u_f)
                + ctrl[1] * (3.0 * u_f * u_f * t)
                + ctrl[2] * (3.0 * u_f * t * t)
                + ctrl[3] * (t * t * t);
            let u = (p.x * span_f).round().clamp(0.0, span_f) as i16;
            let v = (p.y * amp_px).round().clamp(0.0, amp_px) as i16;
            let pair = (u, v);
            if pts.last().copied() != Some(pair) {
                pts.push(pair);
            }
        }
    }
    // Close the polygon cleanly back along the baseline (`perp = 0`).
    if let (Some((u0, _)), Some((u1, v1))) = (pts.first().copied(), pts.last().copied()) {
        if v1 != 0 {
            pts.push((u1, 0));
        }
        pts.push((u0, 0));
    }

    let extent = (amp_px as i32) + 1;
    (extent, pts)
}

/// Returns `(extent, points)` for an interior edge of length `span` and `variant`.
///
/// When `is_vert` is `true`, points are `(dx, dy)` along `(perp, along)`; otherwise `(along, perp)`.
pub(crate) fn edge_curve(is_vert: bool, span: i32, variant: usize) -> (i32, Vec<(i16, i16)>) {
    let (extent, mut pts) = sample_canonical_bezier_edge(variant, span);
    if is_vert {
        for p in &mut pts {
            *p = (p.1, p.0);
        }
    }
    (extent, pts)
}

#[derive(Clone, Debug)]
pub struct LcgRng(u32);

impl LcgRng {
    pub fn new(seed: u32) -> Self {
        Self(seed | 1)
    }

    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> usize {
        self.0 = self.0.wrapping_mul(214013).wrapping_add(2531011);
        ((self.0 >> 16) & 0x7FFF) as usize
    }

    pub fn shuffled_indices(&mut self, count: usize) -> Vec<usize> {
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
pub enum PointerVisualState {
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
    pub session_id: u64,
    pub asset_rev: u64,
    pub visual_rev: u64,
    pub picture_guide: bool,
    pub pointer_state: PointerVisualState,
}

#[inline]
pub(crate) fn dissolve_progress(anim: &AnimationState) -> f32 {
    let duration = (anim.frame_delay_secs * 0.5).min(4.0 * DOS_TICK_SECS);
    if duration > 0.0 {
        anim.tick_accum / duration
    } else {
        1.0
    }
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

#[cfg(test)]
#[path = "puzzle_tests.rs"]
mod tests;
