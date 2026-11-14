//! Puzzle entity lifecycle, piece placement, connected-group operations, and animation helpers.

use bevy_ecs::prelude::*;

use crate::assets::{FrameCommand, PuzzleAsset, SceneRecord, SequencePlayback, Sprite8061};
use crate::audio::CommonSound;
use crate::ecs::components::{
    AnimationState, HintHighlight, PieceEdges, PieceGeometry, PieceIdentity, SceneHotspot,
};
use crate::ecs::events::AudioCommand;
use crate::ecs::resources::{AudioStatus, PuzzleMeta};
use crate::graphics::{
    Framebuffer, compute_frame_bounds, hit_test_frame, rasterize_polygon, render_scene_frame,
};
use crate::puzzle::{
    DOS_TICK_SECS, LcgRng, PLAYFIELD_H, PLAYFIELD_W, edge_curve, grid_col_widths, grid_row_heights,
};

pub struct CutPieceData {
    pub identity: PieceIdentity,
    pub geometry: PieceGeometry,
    pub edges: PieceEdges,
    pub sprite: Sprite8061,
}

pub fn render_full_picture_into(asset: &PuzzleAsset, fb: &mut Framebuffer) {
    fb.reset_clip();
    fb.palette = asset.palette;
    let static_frame_idx = asset.static_frame_idx();
    render_scene_frame(fb, asset, static_frame_idx, 0, 0, 0, true, 0, 0);
}

pub fn render_full_picture(asset: &PuzzleAsset) -> Box<Framebuffer> {
    let mut fb = Box::new(Framebuffer::new());
    render_full_picture_into(asset, &mut fb);
    fb
}

pub type GridPolyline = Vec<(i32, i32)>;
pub type CutPiecesResult = (Vec<CutPieceData>, Vec<GridPolyline>, Vec<GridPolyline>);

/// Cuts the `270 x 196` puzzle picture into `dim x dim` interlocking (or straight) pieces (`FUN_1e59_011c`).
pub fn cut_pieces_data(
    puzzle_num: usize,
    asset: &PuzzleAsset,
    full_picture_fb: &Framebuffer,
    grid_dim: usize,
    curved_edges: bool,
) -> CutPiecesResult {
    if grid_dim == 1 {
        return (cut_preset_pieces_data(asset), Vec::new(), Vec::new());
    }
    let dim = grid_dim;
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
        (puzzle_num as u32)
            .wrapping_mul(1103515245)
            .wrapping_add((dim as u32) * 12345),
    );

    let mut next_edge = || {
        let code = rng.next() & 7;
        (code & 3, if code < 4 { 1 } else { -1 })
    };

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

    let mut grid_polylines_v = Vec::new();
    for c in 1..dim {
        let bx = col_x[c];
        let mut line = Vec::new();
        line.push((bx, 0));
        for r in 0..dim {
            let by = row_y[r];
            if curved_edges {
                let (variant, dir) = v_edges[r * v_stride + c];
                let (_, pts) = crate::puzzle::edge_curve(true, row_h[r], variant);
                for &(dx, dy) in &pts {
                    line.push((bx + (dx as i32) * dir, by + (dy as i32)));
                }
            }
            line.push((bx, row_y[r + 1]));
        }
        grid_polylines_v.push(line);
    }

    let mut grid_polylines_h = Vec::new();
    for r in 1..dim {
        let by = row_y[r];
        let mut line = Vec::new();
        line.push((0, by));
        for c in 0..dim {
            let bx = col_x[c];
            if curved_edges {
                let (variant, dir) = h_edges[r * dim + c];
                let (_, pts) = crate::puzzle::edge_curve(false, col_w[c], variant);
                for &(dx, dy) in &pts {
                    line.push((bx + (dx as i32), by + (dy as i32) * dir));
                }
            }
            line.push((col_x[c + 1], by));
        }
        grid_polylines_h.push(line);
    }

    let mut pieces = Vec::with_capacity(dim * dim);
    let mut poly = Vec::with_capacity(48);
    let mut nodes = Vec::with_capacity(48);
    let src_ox = 2;
    let src_oy = 2;

    for r in 0..dim {
        for c in 0..dim {
            let idx = r * dim + c;
            let bx = col_x[c];
            let by = row_y[r];
            let bw = col_w[c];
            let bh = row_h[r];

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
            if curved_edges {
                for (i, &(active, is_vert, (var, dir), _, _, out_dir)) in borders.iter().enumerate()
                {
                    if active && dir == out_dir {
                        let span = if is_vert { bh } else { bw };
                        pads[i] = edge_curve(is_vert, span, var).0;
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

            if curved_edges {
                for &(active, is_vert, (var, dir), ex, ey, out_dir) in &borders {
                    if !active {
                        continue;
                    }
                    let span = if is_vert { bh } else { bw };
                    let (_, pts) = edge_curve(is_vert, span, var);
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
                        px_buf[i] = full_picture_fb.pixels[src_row + dx as usize];
                    }
                }
            }

            let sprite = Sprite8061::from_8bpp(tw, th, trans, px_buf, opacity);
            let edge_tag = |is_outer: bool, curved_val: (usize, i32)| {
                if is_outer {
                    None
                } else if curved_edges {
                    Some(curved_val)
                } else {
                    Some((0, 0))
                }
            };

            pieces.push(CutPieceData {
                identity: PieceIdentity {
                    index: idx,
                    home_row: r,
                    home_col: c,
                },
                geometry: PieceGeometry {
                    target_x: tx,
                    target_y: ty,
                    base_x: bx,
                    base_y: by,
                    base_w: bw,
                    base_h: bh,
                },
                edges: PieceEdges {
                    left: edge_tag(c == 0, v_edges[r * v_stride + c]),
                    right: edge_tag(c + 1 == dim, v_edges[r * v_stride + (c + 1)]),
                    top: edge_tag(r == 0, h_edges[r * dim + c]),
                    bottom: edge_tag(r + 1 == dim, h_edges[(r + 1) * dim + c]),
                },
                sprite,
            });
        }
    }

    (pieces, grid_polylines_v, grid_polylines_h)
}

fn cut_preset_pieces_data(asset: &PuzzleAsset) -> Vec<CutPieceData> {
    let mut pieces = Vec::new();
    let mut pixels = Box::new(Framebuffer::new());
    let mut mask = Box::new(Framebuffer::new());
    if let Some(SceneRecord::Frame { commands, .. }) = asset.records.get(asset.preset_frame_idx()) {
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
            let (x, y, w, h) =
                compute_frame_bounds(asset, frame_idx, i32::from(dx), i32::from(dy), flags, 0);
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
                asset,
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
                asset,
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
            let idx = pieces.len();
            pieces.push(CutPieceData {
                identity: PieceIdentity {
                    index: idx,
                    home_row: 0,
                    home_col: idx,
                },
                geometry: PieceGeometry {
                    target_x: left - 2,
                    target_y: top - 2,
                    base_x: left - 2,
                    base_y: top - 2,
                    base_w: width,
                    base_h: height,
                },
                edges: PieceEdges::default(),
                sprite: Sprite8061::from_8bpp(width, height, 0, colors, opacity),
            });
        }
    }
    pieces
}

pub fn extract_hotspots(asset: &PuzzleAsset) -> Vec<SceneHotspot> {
    let Some(film) = asset.sequences.get(1) else {
        return Vec::new();
    };
    let Some(SceneRecord::SequenceHeader { hotspots, .. }) = asset.records.get(film.header_idx)
    else {
        return Vec::new();
    };
    let Some(SceneRecord::Frame { commands, .. }) = asset.records.get(film.start_frame) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut child_index = 0;
    for command in commands {
        if let FrameCommand::SubFrame {
            dx,
            dy,
            frame_idx,
            flags,
        } = *command
        {
            if let Some(h) = hotspots.get(child_index) {
                out.push(SceneHotspot {
                    index: child_index,
                    x: h.x,
                    y: h.y,
                    width: h.width,
                    height: h.height,
                    frame_idx,
                    dx,
                    dy,
                    flags,
                    playback: h.playback,
                });
            }
            child_index += 1;
        }
    }
    out
}

pub fn default_playback(asset: &PuzzleAsset) -> SequencePlayback {
    asset
        .sequences
        .get(1)
        .and_then(|film| {
            let SceneRecord::SequenceHeader {
                default_playback, ..
            } = asset.records.get(film.header_idx)?
            else {
                return None;
            };
            *default_playback
        })
        .unwrap_or(SequencePlayback {
            seq_idx: asset.full_anim_seq_idx,
            repetitions: 0,
            frame_delay_ticks: 2,
        })
}

pub fn apply_frame_side_effects(
    asset: &mut PuzzleAsset,
    voice_rng: &mut LcgRng,
    text_enabled: bool,
    audio_status: &AudioStatus,
    anim: &mut AnimationState,
    frame_idx: usize,
    audio_out: &mut Vec<AudioCommand>,
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

    let mut started_sound = false;
    if let Some(sidx) = sound_to_play {
        audio_out.push(AudioCommand::StopAll);
        anim.bubble = None;
        anim.looping_sound = None;
        if let Some(entry) = asset.sounds.get_mut(sidx) {
            let mut voice_chosen = false;
            let looping = entry.selection_mode & 0x80 != 0;
            let has_music = entry
                .sounds
                .first()
                .is_some_and(|desc| audio_status.available_iss_stems.contains(&desc.stem));
            if (audio_status.sfx_enabled || !audio_status.music_enabled || !has_music)
                && entry.sounds.len() > 1
                && let Some(voice) = entry.select_voice(|| (voice_rng.next() & 1) != 0)
            {
                let desc = &entry.sounds[voice + 1];
                if audio_status.available_nnv.contains(&desc.stem) {
                    voice_chosen = true;
                    if audio_status.sfx_enabled {
                        started_sound = true;
                    }
                    audio_out.push(AudioCommand::PlayNnv {
                        stem: desc.stem.clone(),
                        phrase_idx: desc.phrase_idx as usize,
                    });
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
                if audio_status.music_enabled {
                    started_sound = true;
                }
                audio_out.push(AudioCommand::PlayIss {
                    stem: desc.stem.clone(),
                    song_idx: desc.phrase_idx as usize,
                });
                if looping {
                    anim.looping_sound = Some((sidx, 0));
                }
            }
        }
    }
    if anim.wait_for_audio
        && anim.bubble.is_some()
        && text_enabled
        && !audio_status.is_playing
        && !started_sound
    {
        anim.frame_delay_secs = 20.0 * DOS_TICK_SECS;
    }
}

pub fn build_animation_state(
    meta: &mut PuzzleMeta,
    playback: SequencePlayback,
    audio_status: &AudioStatus,
    audio_out: &mut Vec<AudioCommand>,
) -> Option<AnimationState> {
    let seq_idx = playback.seq_idx;
    let default_delay_secs = f32::from(playback.frame_delay_ticks) * DOS_TICK_SECS;
    let (start_frame, end_frame) = meta
        .asset
        .sequences
        .get(seq_idx)
        .filter(|seq| seq.start_frame < seq.end_frame)
        .map(|seq| (seq.start_frame, seq.end_frame))?;

    let mut anim = AnimationState {
        seq_idx,
        start_frame,
        previous_frame: meta.asset.static_frame_idx(),
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
    apply_frame_side_effects(
        &mut meta.asset,
        &mut meta.voice_rng,
        meta.text_enabled,
        audio_status,
        &mut anim,
        start_frame,
        audio_out,
    );
    Some(anim)
}

/// Computes playfield-relative draw origin `(x, y)` for a piece placed on the board.
pub fn piece_placed_origin(
    grid_dim: usize,
    geom: &PieceGeometry,
    row: usize,
    col: usize,
) -> (i32, i32) {
    if grid_dim <= 1 {
        return (geom.target_x, geom.target_y);
    }
    let col_x: i32 = grid_col_widths(grid_dim)[..col].iter().sum();
    let row_y: i32 = grid_row_heights(grid_dim)[..row].iter().sum();
    (
        geom.target_x + col_x - geom.base_x,
        geom.target_y + row_y - geom.base_y,
    )
}

/// Checks if a single piece with `edges` can be placed at `(r, c)` on a `dim x dim` board.
pub fn can_place_piece_at<'a>(
    dim: usize,
    edges: &PieceEdges,
    r: usize,
    c: usize,
    board_pieces: impl Iterator<Item = (usize, usize, &'a PieceEdges)>,
) -> bool {
    if r >= dim || c >= dim {
        return false;
    }
    let mut left_neighbor = None;
    let mut right_neighbor = None;
    let mut top_neighbor = None;
    let mut bottom_neighbor = None;
    for (pr, pc, pe) in board_pieces {
        if pr == r && pc == c {
            return false;
        }
        if pr == r && pc + 1 == c {
            left_neighbor = Some(*pe);
        } else if pr == r && c + 1 == pc {
            right_neighbor = Some(*pe);
        } else if pr + 1 == r && pc == c {
            top_neighbor = Some(*pe);
        } else if r + 1 == pr && pc == c {
            bottom_neighbor = Some(*pe);
        }
    }

    if c == 0 {
        if edges.left.is_some() {
            return false;
        }
    } else {
        if edges.left.is_none() {
            return false;
        }
        if let Some(ne) = left_neighbor
            && edges.left != ne.right
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
        if let Some(ne) = right_neighbor
            && edges.right != ne.left
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
        if let Some(ne) = top_neighbor
            && edges.top != ne.bottom
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
        if let Some(ne) = bottom_neighbor
            && edges.bottom != ne.top
        {
            return false;
        }
    }
    true
}

/// Computes screen-space pixel offsets `(piece_index, dx, dy)` for all held pieces relative to the anchor piece.
pub fn held_group_anchor_cell(
    meta: &PuzzleMeta,
    pointer_x: i32,
    pointer_y: i32,
    held_pieces: impl Iterator<Item = (usize, bool, (i32, i32))> + Clone,
) -> Option<(usize, usize)> {
    let mut count = 0usize;
    let mut has_anchor = false;
    for (_, is_anchor, _) in held_pieces.clone() {
        count += 1;
        if is_anchor {
            has_anchor = true;
        }
    }
    if !has_anchor || count <= 1 || meta.grid_dim <= 1 {
        return None;
    }
    let dim_i32 = meta.grid_dim as i32;
    let dest_anchor = meta
        .grid_cell_at(
            pointer_x - meta.playfield_x(),
            pointer_y - meta.playfield_y(),
        )
        .filter(|&(dr_cell, dc_cell)| {
            held_pieces.clone().all(|(_, _, (dr, dc))| {
                (0..dim_i32).contains(&(dr_cell as i32 + dr))
                    && (0..dim_i32).contains(&(dc_cell as i32 + dc))
            })
        });
    dest_anchor.or(meta.held_group_origin_cell)
}

pub fn held_group_offsets_at(
    meta: &PuzzleMeta,
    pointer_x: i32,
    pointer_y: i32,
    held_pieces: impl Iterator<Item = (usize, bool, (i32, i32))> + Clone,
) -> Vec<(usize, i32, i32)> {
    let mut anchor_idx = None;
    for (idx, is_anchor, _) in held_pieces.clone() {
        if is_anchor {
            anchor_idx = Some(idx);
        }
    }
    let Some(anchor_idx) = anchor_idx else {
        return Vec::new();
    };
    let Some((anchor_r, anchor_c)) =
        held_group_anchor_cell(meta, pointer_x, pointer_y, held_pieces.clone())
    else {
        return vec![(anchor_idx, 0, 0)];
    };
    let dim = meta.grid_dim;
    let col_w = grid_col_widths(dim);
    let row_h = grid_row_heights(dim);
    let mut col_x = [0i32; 9];
    let mut row_y = [0i32; 9];
    for i in 0..dim {
        col_x[i + 1] = col_x[i] + col_w[i];
        row_y[i + 1] = row_y[i] + row_h[i];
    }
    held_pieces
        .map(|(pidx, _, (dr, dc))| {
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

pub fn solved_sequence_at<'a>(
    asset: &PuzzleAsset,
    hotspots: impl Iterator<Item = &'a SceneHotspot>,
    mx: i32,
    my: i32,
) -> SequencePlayback {
    let fallback = default_playback(asset);
    let mut ordered: Vec<&SceneHotspot> = hotspots.collect();
    ordered.sort_by_key(|h| h.index);
    let mut selected = None;
    for hotspot in ordered {
        let center_hit = mx == hotspot.x as i32 + hotspot.width as i32 / 2
            && my == hotspot.y as i32 + hotspot.height as i32 / 2;
        if hotspot.flags & 0x00F0 != 0x0030
            && (center_hit
                || hit_test_frame(
                    asset,
                    hotspot.frame_idx,
                    hotspot.dx as i32,
                    hotspot.dy as i32,
                    hotspot.flags,
                    mx,
                    my,
                    0,
                ))
        {
            selected = Some(hotspot.playback);
        }
    }
    selected.unwrap_or(fallback)
}

pub fn flush_audio_commands(
    commands: Vec<AudioCommand>,
    audio_out: &mut MessageWriter<AudioCommand>,
) {
    for cmd in commands {
        audio_out.write(cmd);
    }
}

/// Completes the active puzzle: marks solved, hides toolbox, sets completion timer message,
/// and starts the `FILM` completion animation (or plays `DoneWithThisOne` immediately).
pub fn complete_puzzle(
    meta: &mut PuzzleMeta,
    audio_status: &AudioStatus,
    commands: &mut Commands,
    existing_anims: &[Entity],
    existing_hints: &[Entity],
    audio_out: &mut MessageWriter<AudioCommand>,
) {
    audio_out.write(AudioCommand::StopAll);
    for &e in existing_anims {
        commands.entity(e).despawn();
    }
    for &e in existing_hints {
        commands.entity(e).remove::<HintHighlight>();
    }
    meta.solved = true;
    meta.toolbox_open = false;
    meta.drag_origin = None;
    meta.held_group_origin_cell = None;
    if meta.timer_enabled {
        let total_secs = meta.elapsed_secs.max(0.0).floor() as u64;
        let hours = total_secs / 3600;
        let mins = (total_secs % 3600) / 60;
        let secs = total_secs % 60;
        meta.set_status_message(format!("Your time was {hours}:{mins:02}:{secs:02}"), 6.0);
    }
    let playback = default_playback(&meta.asset);
    let mut cmds = Vec::new();
    if let Some(anim) = build_animation_state(meta, playback, audio_status, &mut cmds) {
        flush_audio_commands(cmds, audio_out);
        commands.spawn(anim);
        meta.pending_completion_feedback = true;
    } else {
        flush_audio_commands(cmds, audio_out);
        meta.pending_completion_feedback = false;
        audio_out.write(AudioCommand::PlayCommon(CommonSound::DoneWithThisOne));
    }
}
