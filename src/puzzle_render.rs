//! Software framebuffer renderer and render-cache for [`PuzzleSession`].

use crate::assets::{FontAsset, FrameCommand, PuzzleAsset, SceneRecord, ToolboxAsset};
use crate::graphics::{
    Framebuffer, SCENE_OFFSET_X, SCREEN_H, SCREEN_W, apply_dissolve_step, copy_scene_interior,
    render_scene_frame, render_scene_view_with_scratch,
};
use crate::puzzle::{
    AnimationPlayer, PLAYFIELD_H, PLAYFIELD_W, PuzzleSession, TOOLBOX_W, ToolboxTarget,
    dissolve_progress,
};

#[derive(Clone, Copy, Debug, Default)]
struct UiColors {
    white: u8,
    black: u8,
    gray: u8,
    card: u8,
    hover: u8,
    hi: u8,
    lo: u8,
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
            hi: fb.color_index([220, 220, 228]),
            lo: fb.color_index([40, 40, 48]),
            yellow: fb.color_index([255, 255, 85]),
            red: fb.color_index([255, 0, 0]),
            board_bg: fb.color_index([204, 204, 187]),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct BoardStateKey {
    session_id: u64,
    asset_rev: u64,
    board_rev: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct ToolboxCacheKey {
    session_id: u64,
    asset_rev: u64,
    tray_rev: u64,
    hovered_target: Option<ToolboxTarget>,
    pressed_target: Option<ToolboxTarget>,
}

const TOOLBOX_STRIP_W: usize = (TOOLBOX_W - 2) as usize;

#[derive(Clone, Copy, PartialEq, Eq)]
struct AnimFrameCacheKey {
    session_id: u64,
    asset_rev: u64,
    cur_frame: usize,
    bubble: Option<(usize, usize)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct DissolveCacheKey {
    session_id: u64,
    asset_rev: u64,
    previous_frame: usize,
    cur_frame: usize,
}

fn build_guide_playfield(full_picture_fb: &Framebuffer, board_bg: u8, out: &mut Vec<[u8; 4]>) {
    let bg = full_picture_fb.palette[board_bg as usize];
    let mut lut = [[0u8; 4]; 256];
    for (dst, src) in lut.iter_mut().zip(full_picture_fb.palette.iter()) {
        *dst = [
            ((u16::from(bg[0]) * 4 + u16::from(src[0])) / 5) as u8,
            ((u16::from(bg[1]) * 4 + u16::from(src[1])) / 5) as u8,
            ((u16::from(bg[2]) * 4 + u16::from(src[2])) / 5) as u8,
            255,
        ];
    }
    out.clear();
    out.reserve((PLAYFIELD_W * PLAYFIELD_H) as usize);
    for y in 0..PLAYFIELD_H {
        let row = ((2 + y) as usize) * 320 + 2;
        for x in 0..PLAYFIELD_W as usize {
            out.push(lut[full_picture_fb.pixels[row + x] as usize]);
        }
    }
}

/// Cached software renderer for [`PuzzleSession`].
///
/// All derived palette colors, picture-guide LUTs, board layers, and animation/dissolve
/// framebuffers are keyed off `PuzzleSession`'s unique `session_id` and monotonic
/// `asset_rev` / `board_rev` counters, preventing stale caches across session
/// replacements or video-mode switches.
pub struct PuzzleRenderer {
    synced_asset: Option<(u64, u64)>,
    synced_tbx: Option<(u64, u64)>,
    ui_colors: UiColors,
    tbx_remap: [u8; 256],
    guide_playfield: Vec<[u8; 4]>,
    scratch_fb: Box<Framebuffer>,
    dissolve_prev_fb: Box<Framebuffer>,
    dissolve_next_fb: Box<Framebuffer>,
    dissolve_key: Option<DissolveCacheKey>,
    anim_fb: Option<(AnimFrameCacheKey, Box<Framebuffer>)>,
    placed_mask: Option<(BoardStateKey, Vec<bool>)>,
    board_fb: Option<(BoardStateKey, Box<Framebuffer>)>,
    toolbox_strip: Option<(ToolboxCacheKey, Vec<u8>)>,
}

impl Default for PuzzleRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl PuzzleRenderer {
    pub fn new() -> Self {
        Self {
            synced_asset: None,
            synced_tbx: None,
            ui_colors: UiColors::default(),
            tbx_remap: [0u8; 256],
            guide_playfield: Vec::new(),
            scratch_fb: Box::new(Framebuffer::new()),
            dissolve_prev_fb: Box::new(Framebuffer::new()),
            dissolve_next_fb: Box::new(Framebuffer::new()),
            dissolve_key: None,
            anim_fb: None,
            placed_mask: None,
            board_fb: None,
            toolbox_strip: None,
        }
    }

    fn ensure_placed_mask(&mut self, key: BoardStateKey, session: &PuzzleSession) {
        let rebuild = self
            .placed_mask
            .as_ref()
            .is_none_or(|(cached_key, _)| *cached_key != key);
        if rebuild {
            let mut placed_cov = self
                .placed_mask
                .take()
                .map(|(_, mut buf)| {
                    buf.fill(false);
                    buf
                })
                .unwrap_or_else(|| vec![false; (PLAYFIELD_W * PLAYFIELD_H) as usize]);

            for piece in session.pieces.iter().filter(|piece| piece.placed) {
                let spr = &piece.sprite;
                let (ox, oy) = session
                    .piece_placed_origin(piece.index)
                    .unwrap_or((piece.target_x, piece.target_y));
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
            self.placed_mask = Some((key, placed_cov));
        }
    }

    pub(crate) fn tbx_remap(&self) -> &[u8; 256] {
        &self.tbx_remap
    }

    fn ensure_board_fb(
        &mut self,
        key: BoardStateKey,
        session: &PuzzleSession,
        tbx: &ToolboxAsset,
        colors: UiColors,
    ) -> &Framebuffer {
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
            let asset = &session.asset;
            let pfx = session.playfield_x();
            let pfy = session.playfield_y();
            board.palette = asset.palette;
            board.reset_clip();
            board.clear(colors.black);

            // Draw playfield background and decorative border (`FUN_1b43_0363`)
            board.fill_rect(pfx, pfy, PLAYFIELD_W, PLAYFIELD_H, colors.board_bg);
            draw_playfield_border_remapped(
                &mut board,
                tbx,
                pfx - 2,
                PLAYFIELD_W + 2,
                &self.tbx_remap,
            );

            // Draw Grid Lines if enabled (`FUN_1b43_0448`)
            if session.grid_lines {
                board.set_clip(pfx, pfy, PLAYFIELD_W, PLAYFIELD_H);
                for line in session
                    .grid_polylines_v
                    .iter()
                    .chain(&session.grid_polylines_h)
                {
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
            if session.grid_dim == 1 {
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
                    for (piece, command) in session.pieces.iter().zip(
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
                for piece in &session.pieces {
                    if let Some((ox, oy)) = session.piece_placed_origin(piece.index) {
                        board.blit_sprite(&piece.sprite, pfx + ox, pfy + oy, 0);
                    }
                }
            }
            board.reset_clip();
            self.board_fb = Some((key, board));
        }
        &self.board_fb.as_ref().unwrap().1
    }

    fn sync_asset(&mut self, session: &PuzzleSession, tbx: Option<&ToolboxAsset>) {
        let key = (session.session_id(), session.asset_rev());
        if self.synced_asset != Some(key) {
            let full_picture_fb = &session.full_picture_fb;
            self.ui_colors = UiColors::from_framebuffer(full_picture_fb);
            build_guide_playfield(
                full_picture_fb,
                self.ui_colors.board_bg,
                &mut self.guide_playfield,
            );
            self.synced_asset = Some(key);
            self.synced_tbx = None;
        }
        if let Some(tbx) = tbx
            && self.synced_tbx != Some(key)
        {
            let full_picture_fb = &session.full_picture_fb;
            for (i, &rgb) in tbx.palette.iter().enumerate() {
                self.tbx_remap[i] = full_picture_fb.color_index(rgb);
            }
            self.synced_tbx = Some(key);
        }
    }

    /// Renders the static full-picture reference view (`Tab` / Reference button).
    pub fn render_reference(&mut self, session: &PuzzleSession, fb: &mut Framebuffer) {
        self.sync_asset(session, None);
        fb.palette = session.asset.palette;
        fb.reset_clip();
        fb.clear(self.ui_colors.black);
        copy_scene_interior(fb, &session.full_picture_fb, SCENE_OFFSET_X + 2);
    }

    /// Blends the translucent picture guide onto unplaced board background pixels in `rgba`.
    pub fn blend_picture_guide(
        &mut self,
        session: &PuzzleSession,
        rgba: &mut [u8],
        pointer_x: i32,
        pointer_y: i32,
    ) {
        assert_eq!(rgba.len(), 320 * 200 * 4);
        if session.solved || session.grid_dim == 1 || session.active_anim.is_some() {
            return;
        }
        self.sync_asset(session, None);
        let mask_key = BoardStateKey {
            session_id: session.session_id(),
            asset_rev: session.asset_rev(),
            board_rev: session.board_rev(),
        };
        let pfx = session.playfield_x();
        let pfy = session.playfield_y();
        let bg_rgb = session.asset.palette[self.ui_colors.board_bg as usize];
        let bg_rgba = [bg_rgb[0], bg_rgb[1], bg_rgb[2], 255];
        let (rgba_chunks, _) = rgba.as_chunks_mut::<4>();
        let pw = PLAYFIELD_W as usize;
        let held_infos: Vec<_> = session
            .held_piece
            .map(|anchor_idx| {
                let anchor = &session.pieces[anchor_idx];
                let ax = pointer_x - anchor.base_w / 2 - pfx;
                let ay = pointer_y - anchor.base_h / 2 - pfy;
                session
                    .held_group_offsets_at(pointer_x, pointer_y)
                    .into_iter()
                    .map(|(gp_idx, cell_dx, cell_dy)| {
                        let piece = &session.pieces[gp_idx];
                        let hx = ax + cell_dx + piece.target_x - piece.base_x;
                        let hy = ay + cell_dy + piece.target_y - piece.base_y;
                        (piece, hx, hy)
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.ensure_placed_mask(mask_key, session);
        let placed_cov = &self.placed_mask.as_ref().unwrap().1;

        for y in 0..PLAYFIELD_H {
            let cov_row = (y as usize) * pw;
            let rgba_row = ((pfy + y) as usize) * 320 + pfx as usize;
            let dst_row = &mut rgba_chunks[rgba_row..rgba_row + pw];
            let cov_slice = &placed_cov[cov_row..cov_row + pw];
            let guide_row = &self.guide_playfield[cov_row..cov_row + pw];

            let mut has_held_on_row = false;
            let mut held_row = [false; PLAYFIELD_W as usize];
            for &(piece, hx, hy) in &held_infos {
                let dy = y - hy;
                if !(0..piece.sprite.sub_h as i32).contains(&dy) {
                    continue;
                }
                let x0 = hx.clamp(0, PLAYFIELD_W) as usize;
                let x1 = (hx + piece.sprite.width as i32).clamp(0, PLAYFIELD_W) as usize;
                if x0 >= x1 {
                    continue;
                }
                has_held_on_row = true;
                let row_off = (piece.sprite.sub_y as usize + dy as usize)
                    * piece.sprite.stride as usize
                    + piece.sprite.sub_x as usize;
                let src_start = row_off + (x0 as i32 - hx) as usize;
                let src_len = x1 - x0;
                if let Some(mask) = piece.sprite.opacity.as_deref()
                    && src_start + src_len <= mask.len()
                {
                    for (h, &op) in held_row[x0..x1]
                        .iter_mut()
                        .zip(&mask[src_start..src_start + src_len])
                    {
                        if op {
                            *h = true;
                        }
                    }
                } else if src_start + src_len <= piece.sprite.pixels.len() {
                    let trans = piece.sprite.transparent_color;
                    for (h, &px) in held_row[x0..x1]
                        .iter_mut()
                        .zip(&piece.sprite.pixels[src_start..src_start + src_len])
                    {
                        if px != trans {
                            *h = true;
                        }
                    }
                }
            }

            if has_held_on_row {
                for ((d, &g), (&c, &h)) in dst_row
                    .iter_mut()
                    .zip(guide_row)
                    .zip(cov_slice.iter().zip(&held_row))
                {
                    if !c && !h && *d == bg_rgba {
                        *d = g;
                    }
                }
            } else {
                apply_guide_span(dst_row, guide_row, cov_slice, bg_rgba);
            }
        }
    }

    /// Renders the puzzle screen into `fb`.
    pub fn render(
        &mut self,
        session: &PuzzleSession,
        fb: &mut Framebuffer,
        tbx: &ToolboxAsset,
        font: &FontAsset,
        pointer_x: i32,
        pointer_y: i32,
    ) {
        self.sync_asset(session, Some(tbx));
        let asset = &session.asset;
        fb.reset_clip();
        fb.palette = asset.palette;
        let colors = self.ui_colors;

        // If an animation sequence is currently playing (`FILM` / `F_*`), render that frame
        if let Some(anim) = &session.active_anim {
            self.render_animation(session, anim, asset, fb, tbx, font);
            return;
        }

        // If puzzle is solved and idle, show the static completed scene (`c7c4[3]`)
        if session.solved {
            fb.clear(colors.black);
            copy_scene_interior(fb, &session.full_picture_fb, SCENE_OFFSET_X + 2);
            draw_playfield_border_remapped(
                fb,
                tbx,
                SCENE_OFFSET_X,
                PLAYFIELD_W + 2,
                &self.tbx_remap,
            );
            // Draw subtle menu tab icon in top-left corner (`tbx.sprites[49]` or outline)
            if let Some(icon) = tbx.sprites.get(49) {
                fb.blit_sprite_remapped(icon, 2, 2, 0, &self.tbx_remap);
            }
            if let Some(status) = session.status_message() {
                fb.draw_text_shadowed(font, status, 40, 6, colors.black, colors.yellow);
            }
            return;
        }

        let pfx = session.playfield_x();
        let pfy = session.playfield_y();
        let board_key = BoardStateKey {
            session_id: session.session_id(),
            asset_rev: session.asset_rev(),
            board_rev: session.board_rev(),
        };
        let board = self.ensure_board_fb(board_key, session, tbx, colors);
        fb.pixels.copy_from_slice(&board.pixels);

        // Draw F1 hint highlight if active
        if let Some((hidx, rem)) = session.hint_piece
            && let Some(piece) = session.pieces.get(hidx)
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

        if session.toolbox_open {
            self.ensure_toolbox(session, fb, tbx, font, pointer_x, pointer_y);
        } else {
            let hovered =
                session.toolbox_target_at(pointer_x, pointer_y) == Some(ToolboxTarget::ClosedTab);
            let pressed = session.pressed_toolbox_target == Some(ToolboxTarget::ClosedTab);
            if let Some(icon) = tbx.sprites.get(49) {
                fb.blit_sprite_remapped(icon, 2, 2, 0, &self.tbx_remap);
            } else {
                draw_beveled_box(
                    fb,
                    2,
                    2,
                    16,
                    13,
                    colors.card,
                    colors.hi,
                    colors.lo,
                    pressed,
                    hovered,
                    colors.yellow,
                    colors.white,
                );
            }
            if pressed {
                fb.draw_rect_outline(2, 2, 16, 13, colors.white);
            } else if hovered {
                fb.draw_rect_outline(2, 2, 16, 13, colors.yellow);
            }
        }

        // Draw status message if active
        if let Some(status) = session.status_message() {
            fb.draw_text_shadowed(font, status, pfx + 6, 186, colors.black, colors.yellow);
        }

        // Draw currently held piece (or connected group) centered on the cursor
        if let Some(pidx) = session.held_piece {
            let anchor = &session.pieces[pidx];
            let ax = pointer_x - anchor.base_w / 2;
            let ay = pointer_y - anchor.base_h / 2;
            for (gp_idx, cell_dx, cell_dy) in session.held_group_offsets_at(pointer_x, pointer_y) {
                let piece = &session.pieces[gp_idx];
                let px = ax + cell_dx + piece.target_x - piece.base_x;
                let py = ay + cell_dy + piece.target_y - piece.base_y;
                fb.blit_sprite(&piece.sprite, px, py, 0);
            }
        }
    }

    fn render_animation(
        &mut self,
        session: &PuzzleSession,
        anim: &AnimationPlayer,
        asset: &PuzzleAsset,
        fb: &mut Framebuffer,
        tbx: &ToolboxAsset,
        font: &FontAsset,
    ) {
        let bubble = anim.bubble.filter(|_| session.text_enabled);
        if anim.transition_mode == 8 {
            let progress = dissolve_progress(anim);
            let key = DissolveCacheKey {
                session_id: session.session_id(),
                asset_rev: session.asset_rev(),
                previous_frame: anim.previous_frame,
                cur_frame: anim.cur_frame,
            };
            if self.dissolve_key != Some(key) {
                render_scene_view_with_scratch(
                    &mut self.dissolve_prev_fb,
                    &mut self.scratch_fb,
                    asset,
                    anim.previous_frame,
                );
                render_scene_view_with_scratch(
                    &mut self.dissolve_next_fb,
                    &mut self.scratch_fb,
                    asset,
                    anim.cur_frame,
                );
                self.dissolve_key = Some(key);
            }
            apply_dissolve_step(
                fb,
                &self.dissolve_prev_fb,
                &self.dissolve_next_fb,
                asset.uses_ega(),
                progress,
            );
            draw_playfield_border_remapped(
                fb,
                tbx,
                SCENE_OFFSET_X,
                PLAYFIELD_W + 2,
                &self.tbx_remap,
            );
            draw_anim_bubble(fb, asset, font, bubble);
        } else {
            let key = AnimFrameCacheKey {
                session_id: session.session_id(),
                asset_rev: session.asset_rev(),
                cur_frame: anim.cur_frame,
                bubble,
            };
            let rebuild = self
                .anim_fb
                .as_ref()
                .is_none_or(|(cached_key, _)| *cached_key != key);
            if rebuild {
                let mut cached = self
                    .anim_fb
                    .take()
                    .map(|(_, buf)| buf)
                    .unwrap_or_else(|| Box::new(Framebuffer::new()));
                cached.reset_clip();
                cached.palette = asset.palette;
                render_scene_view_with_scratch(
                    &mut cached,
                    &mut self.scratch_fb,
                    asset,
                    anim.cur_frame,
                );
                draw_playfield_border_remapped(
                    &mut cached,
                    tbx,
                    SCENE_OFFSET_X,
                    PLAYFIELD_W + 2,
                    &self.tbx_remap,
                );
                draw_anim_bubble(&mut cached, asset, font, key.bubble);
                self.anim_fb = Some((key, cached));
            }
            fb.pixels
                .copy_from_slice(&self.anim_fb.as_ref().unwrap().1.pixels);
        }
        if let Some(status) = session.status_message() {
            fb.draw_text_shadowed(
                font,
                status,
                session.playfield_x() + 6,
                186,
                self.ui_colors.black,
                self.ui_colors.yellow,
            );
        }
    }

    fn ensure_toolbox(
        &mut self,
        session: &PuzzleSession,
        fb: &mut Framebuffer,
        tbx: &ToolboxAsset,
        font: &FontAsset,
        pointer_x: i32,
        pointer_y: i32,
    ) {
        let hovered_target = session.toolbox_target_at(pointer_x, pointer_y);
        let pressed_target = session.pressed_toolbox_target;
        let key = ToolboxCacheKey {
            session_id: session.session_id(),
            asset_rev: session.asset_rev(),
            tray_rev: session.tray_rev(),
            hovered_target,
            pressed_target,
        };
        if let Some((cached_key, strip)) = &self.toolbox_strip
            && *cached_key == key
        {
            for y in 0..SCREEN_H as usize {
                let dst_off = y * SCREEN_W as usize;
                let src_off = y * TOOLBOX_STRIP_W;
                fb.pixels[dst_off..dst_off + TOOLBOX_STRIP_W]
                    .copy_from_slice(&strip[src_off..src_off + TOOLBOX_STRIP_W]);
            }
            return;
        }
        self.render_toolbox(session, fb, tbx, font, hovered_target, pressed_target);
        let mut strip = self
            .toolbox_strip
            .take()
            .map(|(_, buf)| buf)
            .unwrap_or_else(|| vec![0u8; TOOLBOX_STRIP_W * SCREEN_H as usize]);
        for y in 0..SCREEN_H as usize {
            let src_off = y * SCREEN_W as usize;
            let dst_off = y * TOOLBOX_STRIP_W;
            strip[dst_off..dst_off + TOOLBOX_STRIP_W]
                .copy_from_slice(&fb.pixels[src_off..src_off + TOOLBOX_STRIP_W]);
        }
        self.toolbox_strip = Some((key, strip));
    }

    fn render_toolbox(
        &self,
        session: &PuzzleSession,
        fb: &mut Framebuffer,
        tbx: &ToolboxAsset,
        font: &FontAsset,
        hovered_target: Option<ToolboxTarget>,
        pressed_target: Option<ToolboxTarget>,
    ) {
        let colors = self.ui_colors;
        fb.fill_rect(0, 0, TOOLBOX_W - 2, SCREEN_H, colors.black);
        if let Some(bg) = tbx.sprites.get(4) {
            let bw = bg.width.max(1) as i32;
            let bh = bg.height.max(1) as i32;
            fb.set_clip(2, 0, TOOLBOX_W - 4, SCREEN_H);
            let mut ty = 0;
            while ty < SCREEN_H {
                let mut tx = 2;
                while tx < TOOLBOX_W - 2 {
                    fb.blit_sprite_remapped(bg, tx, ty, 0, &self.tbx_remap);
                    tx += bw;
                }
                ty += bh;
            }
            fb.reset_clip();
        }

        // Top menu button (`y = 2..16`)
        let menu_hovered = hovered_target == Some(ToolboxTarget::Menu);
        let menu_pressed = pressed_target == Some(ToolboxTarget::Menu);
        draw_beveled_box(
            fb,
            2,
            2,
            TOOLBOX_W - 6,
            14,
            if menu_hovered && !menu_pressed {
                colors.hover
            } else {
                colors.card
            },
            colors.hi,
            colors.lo,
            menu_pressed,
            menu_hovered,
            colors.yellow,
            colors.white,
        );
        if let Some(icon) = tbx.sprites.get(49) {
            fb.blit_sprite_remapped(icon, 3, 2, 0, &self.tbx_remap);
            fb.draw_text(font, "MENU", 20, 4, colors.white);
        } else {
            fb.draw_text(font, "MENU", 8, 4, colors.white);
        }

        // Up arrow (`y = 18..29`)
        let up_enabled = session.tray_scroll > 0;
        let up_hovered = up_enabled && hovered_target == Some(ToolboxTarget::ScrollUp);
        let up_pressed = up_enabled && pressed_target == Some(ToolboxTarget::ScrollUp);
        draw_beveled_box(
            fb,
            2,
            18,
            TOOLBOX_W - 6,
            11,
            if up_hovered && !up_pressed {
                colors.hover
            } else {
                colors.card
            },
            colors.hi,
            colors.lo,
            up_pressed,
            up_hovered,
            colors.yellow,
            colors.white,
        );
        let up_color = if up_enabled {
            colors.white
        } else {
            colors.black
        };
        for row in 0..4 {
            fb.draw_line(23 - row, 21 + row, 23 + row, 21 + row, up_color);
        }

        // Down arrow (`y = 187..198`)
        let down_enabled =
            session.tray_scroll + session.visible_tray_slots() < session.tray_order.len();
        let down_hovered = down_enabled && hovered_target == Some(ToolboxTarget::ScrollDown);
        let down_pressed = down_enabled && pressed_target == Some(ToolboxTarget::ScrollDown);
        draw_beveled_box(
            fb,
            2,
            187,
            TOOLBOX_W - 6,
            11,
            if down_hovered && !down_pressed {
                colors.hover
            } else {
                colors.card
            },
            colors.hi,
            colors.lo,
            down_pressed,
            down_hovered,
            colors.yellow,
            colors.white,
        );
        let down_color = if down_enabled {
            colors.white
        } else {
            colors.black
        };
        for row in 0..4 {
            fb.draw_line(23 - row, 194 - row, 23 + row, 194 - row, down_color);
        }

        // Visible unplaced pieces in tray (`y = 30..186`)
        let vis = session.visible_tray_slots();
        let slot_h = session.tray_slot_height();
        fb.set_clip(2, 30, TOOLBOX_W - 6, 156);
        for slot in 0..vis {
            let tray_idx = session.tray_scroll + slot;
            if let Some(&pidx) = session.tray_order.get(tray_idx) {
                let piece = &session.pieces[pidx];
                let slot_y = 30 + slot as i32 * slot_h;
                let hovered = hovered_target == Some(ToolboxTarget::TraySlot(slot));
                let pressed = pressed_target == Some(ToolboxTarget::TraySlot(slot));
                draw_beveled_box(
                    fb,
                    2,
                    slot_y + 1,
                    TOOLBOX_W - 6,
                    slot_h - 2,
                    if hovered && !pressed {
                        colors.hover
                    } else {
                        colors.card
                    },
                    colors.hi,
                    colors.lo,
                    pressed,
                    hovered,
                    colors.yellow,
                    colors.white,
                );
                fb.blit_thumbnail(&piece.sprite, 3, slot_y + 2, TOOLBOX_W - 8, slot_h - 4);
            }
        }
        fb.reset_clip();
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_beveled_box(
    fb: &mut Framebuffer,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    fill: u8,
    hi: u8,
    lo: u8,
    pressed: bool,
    hovered: bool,
    hover_border: u8,
    press_border: u8,
) {
    fb.fill_rect(x, y, w, h, fill);
    if pressed {
        fb.draw_rect_outline(x, y, w, h, press_border);
        if w > 2 && h > 2 {
            fb.draw_line(x + 1, y + 1, x + w - 2, y + 1, lo);
            fb.draw_line(x + 1, y + 1, x + 1, y + h - 2, lo);
            fb.draw_line(x + 1, y + h - 2, x + w - 2, y + h - 2, hi);
            fb.draw_line(x + w - 2, y + 1, x + w - 2, y + h - 2, hi);
        }
    } else {
        fb.draw_line(x, y, x + w - 1, y, hi);
        fb.draw_line(x, y, x, y + h - 1, hi);
        fb.draw_line(x, y + h - 1, x + w - 1, y + h - 1, lo);
        fb.draw_line(x + w - 1, y, x + w - 1, y + h - 1, lo);
        if hovered {
            fb.draw_rect_outline(x, y, w, h, hover_border);
        }
    }
}

#[inline(always)]
fn apply_guide_span(dst: &mut [[u8; 4]], guide: &[[u8; 4]], cov: &[bool], bg_rgba: [u8; 4]) {
    let len = dst.len().min(guide.len()).min(cov.len());
    for x in 0..len {
        if !cov[x] && dst[x] == bg_rgba {
            dst[x] = guide[x];
        }
    }
}

fn draw_anim_bubble(
    fb: &mut Framebuffer,
    asset: &PuzzleAsset,
    font: &FontAsset,
    bubble: Option<(usize, usize)>,
) {
    if let Some((sound, voice)) = bubble
        && let Some(Some(bubble_data)) = asset.sounds[sound].bubbles.get(voice)
    {
        let (fill, ink) = if asset.uses_ega() { (15, 1) } else { (248, 16) };
        fb.draw_speech_bubble_offset(bubble_data, font, SCENE_OFFSET_X, 0, fill, ink);
    }
}

/// Draws the original decorative playfield frame using `MJP.TBX` sprites `0x38..0x3C` (`FUN_1b43_0363`).
pub fn draw_playfield_border(fb: &mut Framebuffer, tbx: &ToolboxAsset, x_left: i32, width: i32) {
    draw_playfield_border_inner(fb, tbx, x_left, width, None);
}

/// Draws the decorative playfield frame remapping `MJP.TBX` palette indices via `remap`.
pub fn draw_playfield_border_remapped(
    fb: &mut Framebuffer,
    tbx: &ToolboxAsset,
    x_left: i32,
    width: i32,
    remap: &[u8; 256],
) {
    draw_playfield_border_inner(fb, tbx, x_left, width, Some(remap));
}

fn draw_playfield_border_inner(
    fb: &mut Framebuffer,
    tbx: &ToolboxAsset,
    x_left: i32,
    width: i32,
    remap: Option<&[u8; 256]>,
) {
    let blit = |fb: &mut Framebuffer, spr, x, y| {
        if let Some(table) = remap {
            fb.blit_sprite_remapped(spr, x, y, 0, table);
        } else {
            fb.blit_sprite(spr, x, y, 0);
        }
    };
    if tbx.sprites.len() > 0x3C {
        let mut y = 0;
        while y < SCREEN_H {
            blit(fb, &tbx.sprites[0x3A], x_left, y);
            blit(fb, &tbx.sprites[0x3B], x_left + width, y);
            y += 20;
        }
        let mut dx = 0;
        while dx < width {
            blit(fb, &tbx.sprites[0x38], x_left + dx, 0);
            blit(fb, &tbx.sprites[0x39], x_left + dx, 198);
            dx += 34;
        }
        blit(fb, &tbx.sprites[0x3C], x_left - 1, -2);
        blit(fb, &tbx.sprites[0x3C], x_left - 1, 198);
        blit(fb, &tbx.sprites[0x3C], x_left + width, -2);
        blit(fb, &tbx.sprites[0x3C], x_left + width, 198);
    } else {
        fb.draw_rect_outline(x_left, 0, width + 2, SCREEN_H, 14);
    }
}
