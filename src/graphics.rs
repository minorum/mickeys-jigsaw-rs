//! 320x200 Indexed Framebuffer, Sprite Blitter, Proportional Font Renderer,
//! Polygon Rasterizer, and `0x7ff1` Hierarchical Scene-Graph Renderer (`seg_096c`, `seg_1202`, `VGA.BIN`).

use crate::assets::{FontAsset, FrameCommand, PuzzleAsset, SceneRecord, Sprite8061};
use crate::decompress::IlbmImage;
use std::sync::OnceLock;

pub const SCREEN_W: i32 = 320;
pub const SCREEN_H: i32 = 200;
pub const SCENE_OFFSET_X: i32 = (SCREEN_W - 273) / 2;

pub const DEFAULT_EGA16_PALETTE: [[u8; 3]; 16] = [
    [0, 0, 0],
    [0, 0, 170],
    [0, 170, 0],
    [0, 170, 170],
    [170, 0, 0],
    [170, 0, 170],
    [170, 85, 0],
    [170, 170, 170],
    [85, 85, 85],
    [85, 85, 255],
    [85, 255, 85],
    [85, 255, 255],
    [255, 85, 85],
    [255, 85, 255],
    [255, 255, 85],
    [255, 255, 255],
];

/// 320x200 indexed-color framebuffer with 256-entry RGB palette and clipping rectangle.
#[derive(Clone)]
pub struct Framebuffer {
    pub pixels: [u8; (SCREEN_W * SCREEN_H) as usize],
    pub palette: [[u8; 3]; 256],
    pub clip_x: i32,
    pub clip_y: i32,
    pub clip_w: i32,
    pub clip_h: i32,
}

impl Default for Framebuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl Framebuffer {
    pub fn new() -> Self {
        let mut fb = Self {
            pixels: [0u8; (SCREEN_W * SCREEN_H) as usize],
            palette: [[0u8; 3]; 256],
            clip_x: 0,
            clip_y: 0,
            clip_w: SCREEN_W,
            clip_h: SCREEN_H,
        };
        fb.reset_default_palette();
        fb
    }

    pub fn reset_default_palette(&mut self) {
        self.palette = [[0u8; 3]; 256];
        self.palette[..16].copy_from_slice(&DEFAULT_EGA16_PALETTE);
    }

    pub fn reset_clip(&mut self) {
        self.clip_x = 0;
        self.clip_y = 0;
        self.clip_w = SCREEN_W;
        self.clip_h = SCREEN_H;
    }

    pub fn set_clip(&mut self, x: i32, y: i32, w: i32, h: i32) {
        let x0 = x.clamp(0, SCREEN_W);
        let y0 = y.clamp(0, SCREEN_H);
        let x1 = (x + w).clamp(0, SCREEN_W);
        let y1 = (y + h).clamp(0, SCREEN_H);
        self.clip_x = x0;
        self.clip_y = y0;
        self.clip_w = (x1 - x0).max(0);
        self.clip_h = (y1 - y0).max(0);
    }

    pub fn clear(&mut self, color: u8) {
        self.pixels.fill(color);
    }

    pub fn color_index(&self, rgb: [u8; 3]) -> u8 {
        self.palette
            .iter()
            .enumerate()
            .min_by_key(|(_, color)| {
                color
                    .iter()
                    .zip(rgb)
                    .map(|(&a, b)| (a as i32 - b as i32).pow(2))
                    .sum::<i32>()
            })
            .map(|(index, _)| index as u8)
            .unwrap_or(0)
    }

    #[inline]
    pub fn put_pixel(&mut self, x: i32, y: i32, color: u8) {
        if x >= self.clip_x
            && x < self.clip_x + self.clip_w
            && y >= self.clip_y
            && y < self.clip_y + self.clip_h
        {
            self.pixels[(y * SCREEN_W + x) as usize] = color;
        }
    }

    #[inline]
    pub fn get_pixel(&self, x: i32, y: i32) -> u8 {
        if (0..SCREEN_W).contains(&x) && (0..SCREEN_H).contains(&y) {
            self.pixels[(y * SCREEN_W + x) as usize]
        } else {
            0
        }
    }

    pub fn draw_ilbm(&mut self, img: &IlbmImage) {
        self.palette = img.palette;
        let w = (img.width as i32).min(SCREEN_W);
        let h = (img.height as i32).min(SCREEN_H);
        for y in 0..h {
            let src_row = (y as usize) * (img.width as usize);
            let dst_row = (y * SCREEN_W) as usize;
            let count = w as usize;
            if src_row + count <= img.pixels.len() {
                self.pixels[dst_row..dst_row + count]
                    .copy_from_slice(&img.pixels[src_row..src_row + count]);
            }
        }
    }

    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: u8) {
        let x0 = x.max(self.clip_x);
        let y0 = y.max(self.clip_y);
        let x1 = (x + w).min(self.clip_x + self.clip_w);
        let y1 = (y + h).min(self.clip_y + self.clip_h);
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        for py in y0..y1 {
            let row = (py * SCREEN_W) as usize;
            self.pixels[row + (x0 as usize)..row + (x1 as usize)].fill(color);
        }
    }

    pub fn draw_rect_outline(&mut self, x: i32, y: i32, w: i32, h: i32, color: u8) {
        if w <= 0 || h <= 0 {
            return;
        }
        self.fill_rect(x, y, w, 1, color);
        self.fill_rect(x, y + h - 1, w, 1, color);
        self.fill_rect(x, y, 1, h, color);
        self.fill_rect(x + w - 1, y, 1, h, color);
    }

    fn fill_pattern_rect(&mut self, x: i32, y: i32, w: i32, h: i32, colors: u16) {
        let low = colors as u8;
        let high = (colors >> 8) as u8;
        if low == high {
            self.fill_rect(x, y, w, h, low);
            return;
        }
        let x0 = x.max(self.clip_x);
        let y0 = y.max(self.clip_y);
        let x1 = (x + w).min(self.clip_x + self.clip_w);
        let y1 = (y + h).min(self.clip_y + self.clip_h);
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        for py in y0..y1 {
            let row = &mut self.pixels[(py * SCREEN_W) as usize..];
            for px in x0..x1 {
                row[px as usize] = if (px + py) & 1 == 0 { low } else { high };
            }
        }
    }

    pub fn draw_line(&mut self, mut x0: i32, mut y0: i32, x1: i32, y1: i32, color: u8) {
        let clip_x1 = self.clip_x + self.clip_w;
        let clip_y1 = self.clip_y + self.clip_h;
        if y0 == y1 {
            if y0 >= self.clip_y && y0 < clip_y1 {
                let lx0 = x0.min(x1).max(self.clip_x);
                let lx1 = (x0.max(x1) + 1).min(clip_x1);
                if lx0 < lx1 {
                    let row = (y0 * SCREEN_W) as usize;
                    self.pixels[row + lx0 as usize..row + lx1 as usize].fill(color);
                }
            }
            return;
        }
        let unclipped = x0 >= self.clip_x
            && x0 < clip_x1
            && x1 >= self.clip_x
            && x1 < clip_x1
            && y0 >= self.clip_y
            && y0 < clip_y1
            && y1 >= self.clip_y
            && y1 < clip_y1;
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            if unclipped {
                self.pixels[(y0 * SCREEN_W + x0) as usize] = color;
            } else {
                self.put_pixel(x0, y0, color);
            }
            if x0 == x1 && y0 == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x0 += sx;
            }
            if e2 <= dx {
                err += dx;
                y0 += sy;
            }
        }
    }

    /// Blits a `0x8061` sprite at `(x, y)` with `flags`:
    /// - `flags & 1`: horizontal flip
    /// - `flags & 2`: vertical flip
    /// - `(flags >> 4) & 0xF`: mode (`0` = transparent blit, `1` = opaque copy, `2` = solid color silhouette `(flags >> 8)`)
    pub fn blit_sprite(&mut self, spr: &Sprite8061, x: i32, y: i32, flags: u16) {
        let flip_x = (flags & 1) != 0;
        let flip_y = (flags & 2) != 0;
        let mode = (flags >> 4) & 0x0F;
        let solid_color = (flags >> 8) as u8;

        let w = spr.width as i32;
        let h = spr.sub_h as i32;
        let stride = spr.stride as usize;
        let sub_x = spr.sub_x as i32;
        let sub_y = spr.sub_y as i32;

        let mut dy_start = (self.clip_y - y).max(0);
        let mut dy_end = (self.clip_y + self.clip_h - y).min(h);
        let mut dx_start = (self.clip_x - x).max(0);
        let mut dx_end = (self.clip_x + self.clip_w - x).min(w);
        if flip_y {
            dy_start = dy_start.max(sub_y + h - spr.height as i32);
            dy_end = dy_end.min(sub_y + h);
        } else {
            dy_start = dy_start.max(-sub_y);
            dy_end = dy_end.min(spr.height as i32 - sub_y);
        }
        if flip_x {
            dx_start = dx_start.max(sub_x + w - spr.stride as i32);
            dx_end = dx_end.min(sub_x + w);
        } else {
            dx_start = dx_start.max(-sub_x);
            dx_end = dx_end.min(spr.stride as i32 - sub_x);
        }
        if dy_start >= dy_end || dx_start >= dx_end {
            return;
        }

        let len = (dx_end - dx_start) as usize;
        let dst_x0 = (x + dx_start) as usize;
        let required_len = (spr.height as usize) * stride;
        if spr.pixels.len() < required_len
            || spr.opacity.as_ref().is_some_and(|m| m.len() < required_len)
        {
            return;
        }
        let opacity = spr.opacity.as_deref();

        for dy in dy_start..dy_end {
            let py = y + dy;
            let sy = if flip_y {
                sub_y + (h - 1 - dy)
            } else {
                sub_y + dy
            };
            let row_off = (sy as usize) * stride;
            let dst_row = (py * SCREEN_W) as usize;
            let dst_slice = &mut self.pixels[dst_row + dst_x0..dst_row + dst_x0 + len];
            let src_start = if flip_x {
                row_off + (sub_x + w - dx_start) as usize - len
            } else {
                row_off + (sub_x + dx_start) as usize
            };
            let src_row = &spr.pixels[src_start..src_start + len];
            let mask_row = opacity.map(|m| &m[src_start..src_start + len]);
            let solid = (mode == 2).then_some(solid_color);
            match (mode == 1, flip_x) {
                (true, false) => dst_slice.copy_from_slice(src_row),
                (true, true) => {
                    for (dst, &src) in dst_slice.iter_mut().zip(src_row.iter().rev()) {
                        *dst = src;
                    }
                }
                (false, false) => blit_masked_row(
                    dst_slice,
                    src_row.iter(),
                    mask_row.map(|m| m.iter()),
                    spr.transparent_color,
                    solid,
                ),
                (false, true) => blit_masked_row(
                    dst_slice,
                    src_row.iter().rev(),
                    mask_row.map(|m| m.iter().rev()),
                    spr.transparent_color,
                    solid,
                ),
            }
        }
    }

    pub fn blit_thumbnail(&mut self, spr: &Sprite8061, x: i32, y: i32, w: i32, h: i32) {
        let scale = (w as f32 / spr.width as f32)
            .min(h as f32 / spr.sub_h as f32)
            .min(1.0);
        let dw = (spr.width as f32 * scale).max(1.0) as i32;
        let dh = (spr.sub_h as f32 * scale).max(1.0) as i32;
        let ox = x + (w - dw) / 2;
        let oy = y + (h - dh) / 2;
        let dy_start = (self.clip_y - oy).max(0);
        let dy_end = (self.clip_y + self.clip_h - oy).min(dh);
        let dx_start = (self.clip_x - ox).max(0);
        let dx_end = (self.clip_x + self.clip_w - ox).min(dw);
        if dy_start >= dy_end || dx_start >= dx_end {
            return;
        }
        let stride = spr.stride as usize;
        let sub_x = spr.sub_x as usize;
        let sub_y = spr.sub_y as usize;
        let spr_w = spr.width as usize;
        let spr_h = spr.sub_h as usize;
        let required_len = (spr.height as usize) * stride;
        if spr.pixels.len() < required_len
            || sub_x + spr_w > stride
            || sub_y + spr_h > spr.height as usize
            || spr.opacity.as_ref().is_some_and(|m| m.len() < required_len)
        {
            return;
        }
        let opacity = spr.opacity.as_deref();
        let dw_u = dw as usize;
        let dh_u = dh as usize;
        let trans = spr.transparent_color;
        for dy in dy_start..dy_end {
            let sy = sub_y + (dy as usize) * spr_h / dh_u;
            let row_off = sy * stride + sub_x;
            let dst_row = ((oy + dy) * SCREEN_W + ox) as usize;
            for dx in dx_start..dx_end {
                let idx = row_off + (dx as usize) * spr_w / dw_u;
                let opaque = opacity.map_or_else(|| spr.pixels[idx] != trans, |mask| mask[idx]);
                if opaque {
                    self.pixels[dst_row + dx as usize] = spr.pixels[idx];
                }
            }
        }
    }

    /// Renders proportional text using `MJP.NFT` (`FUN_196c_023a`).
    pub fn draw_text(&mut self, font: &FontAsset, text: &str, mut x: i32, mut y: i32, color: u8) {
        let start_x = x;
        let h = font.height as i32;
        let clip_x0 = self.clip_x;
        let clip_y0 = self.clip_y;
        let clip_x1 = self.clip_x + self.clip_w;
        let clip_y1 = self.clip_y + self.clip_h;
        for ch in text.chars() {
            if ch == '\r' || ch == '\n' {
                x = start_x;
                y += h + 1;
                continue;
            }
            let code = ch as u32;
            if !(32..128).contains(&code) {
                continue;
            }
            let idx = (code - 32) as usize;
            let cw = font.char_w[idx] as i32;
            let cx = font.char_x[idx] as i32;
            if code != 32 && cw > 0 {
                let row_start = (clip_y0 - y).max(0);
                let row_end = (clip_y1 - y).min(h);
                let col_start = (clip_x0 - x).max(0);
                let col_end = (clip_x1 - x).min(cw);
                if row_start < row_end && col_start < col_end {
                    for row in row_start..row_end {
                        let row_base = (row as usize) * font.stride_bytes;
                        let dst_row = ((y + row) * SCREEN_W) as usize;
                        for col in col_start..col_end {
                            let bit_x = (cx + col) as usize;
                            let byte_idx = row_base + (bit_x >> 3);
                            let bit_mask = 0x80u8 >> (bit_x & 7);
                            if byte_idx < font.bitmap.len()
                                && (font.bitmap[byte_idx] & bit_mask) != 0
                            {
                                self.pixels[dst_row + (x + col) as usize] = color;
                            }
                        }
                    }
                }
            }
            x += cw;
        }
    }

    /// Renders shadowed proportional text (`FUN_196c_030c`).
    pub fn draw_text_shadowed(
        &mut self,
        font: &FontAsset,
        text: &str,
        x: i32,
        y: i32,
        shadow_color: u8,
        fg_color: u8,
    ) {
        self.draw_text(font, text, x + 1, y + 1, shadow_color);
        self.draw_text(font, text, x, y, fg_color);
    }

    /// Scanline polygon fill (`VGA.BIN` `1000:3006`).
    pub fn fill_polygon(&mut self, pts: &[(i32, i32)], color: u8) {
        self.fill_polygon_offset(pts, 0, 0, color);
    }

    pub fn fill_polygon_offset(&mut self, pts: &[(i32, i32)], ox: i32, oy: i32, color: u8) {
        let mut nodes = Vec::with_capacity(pts.len());
        rasterize_polygon(
            pts,
            ox,
            oy,
            self.clip_x,
            self.clip_y,
            self.clip_w,
            self.clip_h,
            &mut nodes,
            |y, x0, x1| {
                let row = (y * SCREEN_W) as usize;
                self.pixels[row + (x0 as usize)..=row + (x1 as usize)].fill(color);
            },
        );
    }

    pub fn draw_polygon_outline_offset(&mut self, pts: &[(i32, i32)], ox: i32, oy: i32, color: u8) {
        if pts.len() < 2 {
            return;
        }
        for i in 0..pts.len() {
            let (x0, y0) = pts[i];
            let (x1, y1) = pts[(i + 1) % pts.len()];
            self.draw_line(x0 + ox, y0 + oy, x1 + ox, y1 + oy, color);
        }
    }

    pub fn draw_speech_bubble(
        &mut self,
        bubble: &crate::assets::SpeechBubble,
        font: &FontAsset,
        fill: u8,
        ink: u8,
    ) {
        self.draw_speech_bubble_offset(bubble, font, 0, 0, fill, ink);
    }

    pub fn draw_speech_bubble_offset(
        &mut self,
        bubble: &crate::assets::SpeechBubble,
        font: &FontAsset,
        ox: i32,
        oy: i32,
        fill: u8,
        ink: u8,
    ) {
        self.fill_polygon_offset(&bubble.points, ox, oy, fill);
        self.draw_polygon_outline_offset(&bubble.points, ox, oy, ink);
        let mut y = bubble.text_y + oy;
        for line in bubble.text.split(|byte| !(32..128).contains(byte)) {
            if let Ok(line) = std::str::from_utf8(line) {
                self.draw_text(font, line, bubble.text_x + ox, y, ink);
            }
            y += font.height as i32;
        }
    }

    /// Converts the indexed 320x200 framebuffer into an RGBA8 byte slice (`320 * 200 * 4`).
    pub fn to_rgba8(&self, out: &mut [u8]) {
        let mut lut = [[0u8; 4]; 256];
        for (dst, &[r, g, b]) in lut.iter_mut().zip(&self.palette) {
            *dst = [r, g, b, 255];
        }
        let (chunks, _) = out.as_chunks_mut::<4>();
        for (&px, dst) in self.pixels.iter().zip(chunks) {
            *dst = lut[px as usize];
        }
    }
}

/// Scanline polygon rasterizer using the `VGA.BIN` half-open Y rule `(yi < y && yj >= y)`.
///
/// Calls `fill_span(y, x0, x1)` with inclusive clipped bounds `x0 <= x1` for each scanline.
#[expect(clippy::too_many_arguments)]
pub(crate) fn rasterize_polygon(
    pts: &[(i32, i32)],
    ox: i32,
    oy: i32,
    clip_x: i32,
    clip_y: i32,
    clip_w: i32,
    clip_h: i32,
    nodes: &mut Vec<i32>,
    mut fill_span: impl FnMut(i32, i32, i32),
) {
    if pts.len() < 3 || clip_w <= 0 || clip_h <= 0 {
        return;
    }
    let mut min_y = i32::MAX;
    let mut max_y = i32::MIN;
    for &(_, y) in pts {
        let py = (y as i64 + oy as i64).clamp(i32::MIN as i64, i32::MAX as i64) as i32;
        min_y = min_y.min(py);
        max_y = max_y.max(py);
    }
    min_y = min_y.max(clip_y);
    max_y = max_y.min(clip_y + clip_h - 1);
    let clip_x1 = clip_x + clip_w - 1;

    for y in min_y..=max_y {
        nodes.clear();
        let y128 = y as i128;
        let mut j = pts.len() - 1;
        for i in 0..pts.len() {
            let xi = pts[i].0 as i128 + ox as i128;
            let yi = pts[i].1 as i128 + oy as i128;
            let xj = pts[j].0 as i128 + ox as i128;
            let yj = pts[j].1 as i128 + oy as i128;
            if (yi < y128 && yj >= y128) || (yj < y128 && yi >= y128) {
                let x = xi + ((y128 - yi) * (xj - xi)) / (yj - yi);
                nodes.push(x.clamp(i32::MIN as i128, i32::MAX as i128) as i32);
            }
            j = i;
        }
        nodes.sort_unstable();
        for pair in nodes.as_chunks::<2>().0 {
            let x0 = pair[0].max(clip_x);
            let x1 = pair[1].min(clip_x1);
            if x0 <= x1 {
                fill_span(y, x0, x1);
            }
        }
    }
}

#[inline(always)]
fn blit_masked_row<'a, P, M>(
    dst_slice: &mut [u8],
    src_row: P,
    mask_row: Option<M>,
    trans: u8,
    solid: Option<u8>,
) where
    P: Iterator<Item = &'a u8>,
    M: Iterator<Item = &'a bool>,
{
    match (mask_row, solid) {
        (Some(mask), Some(c)) => {
            for (dst, &op) in dst_slice.iter_mut().zip(mask) {
                if op {
                    *dst = c;
                }
            }
        }
        (Some(mask), None) => {
            for ((dst, &px), &op) in dst_slice.iter_mut().zip(src_row).zip(mask) {
                if op {
                    *dst = px;
                }
            }
        }
        (None, Some(c)) => {
            for (dst, &px) in dst_slice.iter_mut().zip(src_row) {
                if px != trans {
                    *dst = c;
                }
            }
        }
        (None, None) => {
            for (dst, &px) in dst_slice.iter_mut().zip(src_row) {
                if px != trans {
                    *dst = px;
                }
            }
        }
    }
}

/// Computes the bounding box `(min_x, min_y, width, height)` of a `0x7ff1` frame (`FUN_196c_0aea` & `0ba3`).
pub fn compute_frame_bounds(
    puzzle: &PuzzleAsset,
    frame_idx: usize,
    origin_x: i32,
    origin_y: i32,
    flags: u16,
    depth: usize,
) -> (i32, i32, i32, i32) {
    let (min_x, min_y, max_x, max_y) =
        accumulate_frame_bounds(puzzle, frame_idx, origin_x, origin_y, flags, depth);

    if min_x == i32::MAX || max_x < min_x {
        (
            origin_x + SCREEN_W / 4,
            origin_y + SCREEN_H / 4,
            SCREEN_W / 2,
            SCREEN_H / 2,
        )
    } else {
        (min_x, min_y, max_x - min_x, max_y - min_y)
    }
}

#[inline]
fn reflect_subframe_origin(
    puzzle: &PuzzleAsset,
    child_idx: usize,
    dx: i16,
    dy: i16,
    flags: u16,
    depth: usize,
) -> (i32, i32) {
    let mut lx = dx as i32;
    let mut ly = dy as i32;
    if (flags & 3) != 0 {
        let (bx, by, bw, bh) = compute_frame_bounds(puzzle, child_idx, lx, ly, 0, depth + 1);
        if (flags & 1) != 0 {
            lx += SCREEN_W - bx * 2 - bw;
        }
        if (flags & 2) != 0 {
            ly += SCREEN_H - by * 2 - bh;
        }
    }
    (lx, ly)
}

#[inline]
fn combine_frame_flags(parent_flags: u16, cmd_flags: u16) -> u16 {
    let local_c = if (parent_flags & 0x00F0) != 0 {
        (parent_flags & 0xFFF0) | (cmd_flags & 0x000F)
    } else {
        cmd_flags
    };
    ((parent_flags ^ local_c) & 3) | (local_c & 0xFFF0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FrameTraversalPolicy {
    /// Used by bounds calculation and hit-testing: visits all sub-frames and propagates only flip bits.
    Geometry,
    /// Used by scene rendering: skips top-level `0x0030` sub-frames and propagates high command flags.
    Render,
}

#[derive(Clone, Copy)]
struct FrameWalkState {
    frame_idx: usize,
    ox: i32,
    oy: i32,
    flags: u16,
    depth: usize,
}

fn visit_frame_commands<F>(
    puzzle: &PuzzleAsset,
    state: FrameWalkState,
    policy: FrameTraversalPolicy,
    visitor: &mut F,
) -> std::ops::ControlFlow<()>
where
    F: FnMut(&FrameCommand, i32, i32, u16) -> std::ops::ControlFlow<()>,
{
    if state.depth > 16 {
        return std::ops::ControlFlow::Continue(());
    }
    let Some(SceneRecord::Frame { commands, .. }) = puzzle.records.get(state.frame_idx) else {
        return std::ops::ControlFlow::Continue(());
    };

    for cmd in commands {
        match *cmd {
            FrameCommand::SubFrame {
                dx,
                dy,
                frame_idx: child_idx,
                flags: child_flags,
            } => {
                // In top-level frame (`FUN_196c_17a3` line 1301), skip sub-frames with `(flags & 0xF0) == 0x30`
                if policy == FrameTraversalPolicy::Render
                    && state.depth == 0
                    && (child_flags & 0x00F0) == 0x0030
                {
                    continue;
                }
                let (lx, ly) =
                    reflect_subframe_origin(puzzle, child_idx, dx, dy, state.flags, state.depth);
                let combined_flags = match policy {
                    FrameTraversalPolicy::Render => combine_frame_flags(state.flags, child_flags),
                    FrameTraversalPolicy::Geometry => (state.flags ^ child_flags) & 3,
                };
                visit_frame_commands(
                    puzzle,
                    FrameWalkState {
                        frame_idx: child_idx,
                        ox: state.ox + lx,
                        oy: state.oy + ly,
                        flags: combined_flags,
                        depth: state.depth + 1,
                    },
                    policy,
                    visitor,
                )?;
            }
            _ => visitor(cmd, state.ox, state.oy, state.flags)?,
        }
    }
    std::ops::ControlFlow::Continue(())
}

fn accumulate_frame_bounds(
    puzzle: &PuzzleAsset,
    frame_idx: usize,
    ox: i32,
    oy: i32,
    flags: u16,
    depth: usize,
) -> (i32, i32, i32, i32) {
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    let _ = visit_frame_commands(
        puzzle,
        FrameWalkState {
            frame_idx,
            ox,
            oy,
            flags,
            depth,
        },
        FrameTraversalPolicy::Geometry,
        &mut |cmd, cur_ox, cur_oy, _| {
            let rect = match *cmd {
                FrameCommand::Rect {
                    x,
                    y,
                    width,
                    height,
                    ..
                } => Some((
                    cur_ox + x as i32,
                    cur_oy + y as i32,
                    width as i32,
                    height as i32,
                )),
                FrameCommand::Sprite {
                    x, y, sprite_idx, ..
                } => puzzle.sprites.get(sprite_idx).map(|spr| {
                    (
                        cur_ox + x as i32,
                        cur_oy + y as i32,
                        spr.width as i32,
                        spr.sub_h as i32,
                    )
                }),
                _ => None,
            };
            if let Some((px, py, pw, ph)) = rect {
                min_x = min_x.min(px);
                min_y = min_y.min(py);
                max_x = max_x.max(px + pw);
                max_y = max_y.max(py + ph);
            }
            std::ops::ControlFlow::Continue(())
        },
    );
    (min_x, min_y, max_x, max_y)
}

/// Renders a `0x7ff1` scene/animation frame (`FUN_196c_14f0` & `FUN_196c_17a3`).
/// `x_shift` corresponds to `*(int *)0x9a0` (playfield horizontal shift when toolbox is open, or `0`).
#[expect(clippy::too_many_arguments)]
pub fn render_scene_frame(
    fb: &mut Framebuffer,
    puzzle: &PuzzleAsset,
    frame_idx: usize,
    ox: i32,
    oy: i32,
    flags: u16,
    draw_bg: bool,
    x_shift: i32,
    depth: usize,
) {
    if depth > 16 {
        return;
    }
    let Some(SceneRecord::Frame {
        bg_frame_idx,
        bg_color,
        ..
    }) = puzzle.records.get(frame_idx)
    else {
        return;
    };

    if draw_bg {
        if *bg_frame_idx == 0 {
            fb.fill_pattern_rect(0, 0, SCREEN_W, SCREEN_H, *bg_color);
        } else {
            render_scene_frame(fb, puzzle, *bg_frame_idx, 0, 0, 0, true, x_shift, depth + 1);
        }
    }

    let _ = visit_frame_commands(
        puzzle,
        FrameWalkState {
            frame_idx,
            ox,
            oy,
            flags,
            depth,
        },
        FrameTraversalPolicy::Render,
        &mut |cmd, cur_ox, cur_oy, cur_flags| {
            match *cmd {
                FrameCommand::Rect {
                    x,
                    y,
                    color,
                    flags: cmd_flags,
                    width,
                    height,
                } => {
                    let u6 = combine_frame_flags(cur_flags, cmd_flags);
                    let pattern = if ((u6 >> 4) & 0x0F) == 2 {
                        let solid = (u6 >> 8) & if puzzle.uses_ega() { 15 } else { 255 };
                        if color & 0xff00 == 0 {
                            solid
                        } else if color & 255 == 0 {
                            solid << 8
                        } else {
                            solid | (solid << 8)
                        }
                    } else {
                        color
                    };
                    let (px, py) = (cur_ox + x as i32 + x_shift, cur_oy + y as i32);
                    if puzzle.uses_ega() {
                        fb.fill_pattern_rect(px, py, width as i32, height as i32, pattern);
                    } else {
                        fb.fill_rect(px, py, width as i32, height as i32, pattern as u8);
                    }
                }
                FrameCommand::Sprite {
                    x,
                    y,
                    sprite_idx,
                    flags: cmd_flags,
                } => {
                    if let Some(spr) = puzzle.sprites.get(sprite_idx) {
                        let u6 = combine_frame_flags(cur_flags, cmd_flags);
                        fb.blit_sprite(spr, cur_ox + (x as i32) + x_shift, cur_oy + (y as i32), u6);
                    }
                }
                _ => {}
            }
            std::ops::ControlFlow::Continue(())
        },
    );
}

/// Checks if a point `(test_x, test_y)` hits any non-transparent pixel in `frame_idx` (`FUN_2202_03fc`).
#[expect(clippy::too_many_arguments)]
pub fn hit_test_frame(
    puzzle: &PuzzleAsset,
    frame_idx: usize,
    ox: i32,
    oy: i32,
    flags: u16,
    test_x: i32,
    test_y: i32,
    depth: usize,
) -> bool {
    visit_frame_commands(
        puzzle,
        FrameWalkState {
            frame_idx,
            ox,
            oy,
            flags,
            depth,
        },
        FrameTraversalPolicy::Geometry,
        &mut |cmd, cur_ox, cur_oy, cur_flags| {
            let hit = match *cmd {
                FrameCommand::Rect {
                    x,
                    y,
                    width,
                    height,
                    ..
                } => {
                    let rx = cur_ox + (x as i32);
                    let ry = cur_oy + (y as i32);
                    test_x >= rx
                        && test_x < rx + (width as i32)
                        && test_y >= ry
                        && test_y < ry + (height as i32)
                }
                FrameCommand::Sprite {
                    x,
                    y,
                    sprite_idx,
                    flags: cmd_flags,
                } => puzzle.sprites.get(sprite_idx).is_some_and(|spr| {
                    let sx0 = cur_ox + (x as i32);
                    let sy0 = cur_oy + (y as i32);
                    let w = spr.width as i32;
                    let h = spr.sub_h as i32;
                    if test_x < sx0 || test_x >= sx0 + w || test_y < sy0 || test_y >= sy0 + h {
                        return false;
                    }
                    let u5 = (cur_flags ^ cmd_flags) & 3;
                    let dx = test_x - sx0;
                    let dy = test_y - sy0;
                    let px = if (u5 & 1) != 0 {
                        (spr.sub_x as i32) + (w - 1 - dx)
                    } else {
                        (spr.sub_x as i32) + dx
                    };
                    let py = if (u5 & 2) != 0 {
                        (spr.sub_y as i32) + (h - 1 - dy)
                    } else {
                        (spr.sub_y as i32) + dy
                    };
                    let idx = (py as usize) * (spr.stride as usize) + (px as usize);
                    spr.is_opaque(idx)
                }),
                _ => false,
            };
            if hit {
                std::ops::ControlFlow::Break(())
            } else {
                std::ops::ControlFlow::Continue(())
            }
        },
    )
    .is_break()
}

pub fn dissolve_order(ega: bool) -> &'static [u16] {
    static VGA: OnceLock<Vec<u16>> = OnceLock::new();
    static EGA: OnceLock<Vec<u16>> = OnceLock::new();
    let cache = if ega { &EGA } else { &VGA };
    cache.get_or_init(|| {
        let mut order = Vec::with_capacity(64000);
        let (mask, shift, width, height, feedback) = if ega {
            (255, 8, 160, 100, 0x6000)
        } else {
            (511, 9, 320, 200, 0x12000)
        };
        let mut append = |state: u32| {
            let (x, y) = (state & mask, state >> shift);
            if x < width && y < height {
                let pixel = (y * 320 + x) as u16;
                order.push(pixel);
                if ega {
                    order.extend([pixel + 32000, pixel + 160, pixel + 32160]);
                }
            }
        };
        let mut state = 1;
        loop {
            append(state);
            state = (state >> 1) ^ if state & 1 != 0 { feedback } else { 0 };
            if state == 1 {
                break;
            }
        }
        append(0);
        order
    })
}

fn dissolve_playfield_order(ega: bool) -> &'static [(u16, u16)] {
    static VGA: OnceLock<Vec<(u16, u16)>> = OnceLock::new();
    static EGA: OnceLock<Vec<(u16, u16)>> = OnceLock::new();
    let cache = if ega { &EGA } else { &VGA };
    cache.get_or_init(|| {
        dissolve_order(ega)
            .iter()
            .enumerate()
            .filter_map(|(step, &pixel)| {
                let index = pixel as usize;
                if (2..272).contains(&(index % 320)) && (2..198).contains(&(index / 320)) {
                    Some((step as u16, pixel + SCENE_OFFSET_X as u16))
                } else {
                    None
                }
            })
            .collect()
    })
}

pub fn dissolve_visible_step(ega: bool, progress: f32) -> usize {
    let playfield = dissolve_playfield_order(ega);
    if !progress.is_finite() || progress <= 0.0 {
        return 0;
    }
    if progress >= 1.0 {
        return playfield.len();
    }
    let total = dissolve_order(ega).len();
    let limit = (progress * total as f32) as usize;
    playfield.partition_point(|&(step, _)| (step as usize) < limit)
}

fn overlay_dissolve_pixels(fb: &mut Framebuffer, next: &Framebuffer, ega: bool, progress: f32) {
    let end = dissolve_visible_step(ega, progress);
    if end == 0 {
        return;
    }
    for &(_, target) in &dissolve_playfield_order(ega)[..end] {
        let idx = target as usize;
        fb.pixels[idx] = next.pixels[idx];
    }
}

pub fn apply_dissolve_step(
    fb: &mut Framebuffer,
    prev: &Framebuffer,
    next: &Framebuffer,
    ega: bool,
    progress: f32,
) {
    if progress >= 1.0 {
        fb.pixels.copy_from_slice(&next.pixels);
        return;
    }
    fb.pixels.copy_from_slice(&prev.pixels);
    overlay_dissolve_pixels(fb, next, ega, progress);
}

#[cfg(test)]
pub fn render_scene_dissolve_with_scratch(
    fb: &mut Framebuffer,
    scratch: &mut Framebuffer,
    next: &mut Framebuffer,
    puzzle: &PuzzleAsset,
    previous_frame: usize,
    frame: usize,
    progress: f32,
) {
    if progress >= 1.0 {
        render_scene_view_with_scratch(fb, scratch, puzzle, frame);
        return;
    }
    render_scene_view_with_scratch(fb, scratch, puzzle, previous_frame);
    if progress <= 0.0 {
        return;
    }
    render_scene_view_with_scratch(next, scratch, puzzle, frame);
    overlay_dissolve_pixels(fb, next, puzzle.uses_ega(), progress);
}

pub fn render_scene_view(fb: &mut Framebuffer, puzzle: &PuzzleAsset, frame_idx: usize) {
    let mut scratch = Framebuffer::new();
    render_scene_view_with_scratch(fb, &mut scratch, puzzle, frame_idx);
}

pub fn copy_scene_interior(dst: &mut Framebuffer, src: &Framebuffer, dst_x: i32) {
    for y in 2..198usize {
        let src_off = y * SCREEN_W as usize + 2;
        let dst_off = y * SCREEN_W as usize + dst_x as usize;
        dst.pixels[dst_off..dst_off + 270].copy_from_slice(&src.pixels[src_off..src_off + 270]);
    }
}

pub fn render_scene_view_with_scratch(
    fb: &mut Framebuffer,
    source: &mut Framebuffer,
    puzzle: &PuzzleAsset,
    frame_idx: usize,
) {
    source.clear(0);
    source.set_clip(2, 2, 270, 196);
    render_scene_frame(source, puzzle, frame_idx, 0, 0, 0, true, 0, 0);
    fb.palette = puzzle.palette;
    fb.reset_clip();
    fb.clear(fb.color_index([0; 3]));
    copy_scene_interior(fb, source, SCENE_OFFSET_X + 2);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[cfg_attr(
        not(feature = "asset-tests"),
        ignore = "requires original DOS game assets"
    )]
    #[test]
    fn vga_sprite_blits_match_original_driver() {
        check_sprite_blits(include_str!("../validation/sprite-blits.tsv"), false);
    }

    #[cfg_attr(
        not(feature = "asset-tests"),
        ignore = "requires original DOS game assets"
    )]
    #[test]
    fn ega_sprite_blits_match_original_driver() {
        check_sprite_blits(include_str!("../validation/ega-sprite-blits.tsv"), true);
    }

    #[cfg_attr(
        not(feature = "asset-tests"),
        ignore = "requires original DOS game assets"
    )]
    #[test]
    fn vga_scene_rasters_match_original_traversal_and_driver() {
        check_scene_rasters(include_str!("../validation/scene-raster-vga.tsv"), false);
    }

    #[cfg_attr(
        not(feature = "asset-tests"),
        ignore = "requires original DOS game assets"
    )]
    #[test]
    fn ega_scene_rasters_match_original_traversal_and_driver() {
        check_scene_rasters(include_str!("../validation/scene-raster-ega.tsv"), true);
    }

    fn check_scene_rasters(fixture: &str, use_ega: bool) {
        let mut loaded = None;
        let mut checked = 0;
        for row in fixture.lines().skip(1) {
            let columns: Vec<_> = row.split('\t').collect();
            let id: usize = columns[0][1..3].parse().unwrap();
            if loaded.as_ref().is_none_or(|(previous, _)| *previous != id) {
                loaded = Some((
                    id,
                    PuzzleAsset::load(Path::new(env!("CARGO_MANIFEST_DIR")), id, use_ega).unwrap(),
                ));
            }
            let mut fb = Framebuffer::new();
            fb.clear(0);
            fb.set_clip(2, 2, 270, 196);
            render_scene_frame(
                &mut fb,
                &loaded.as_ref().unwrap().1,
                columns[1].parse().unwrap(),
                0,
                0,
                0,
                true,
                0,
                0,
            );
            let checksum = fb.pixels.iter().fold(0xcbf29ce484222325u64, |hash, &byte| {
                (hash ^ byte as u64).wrapping_mul(0x100000001b3)
            });
            assert_eq!(
                checksum,
                u64::from_str_radix(columns[3], 16).unwrap(),
                "{row}"
            );
            checked += 1;
        }
        assert_eq!(checked, if use_ega { 2507 } else { 3079 });
    }

    fn check_sprite_blits(fixture: &str, use_ega: bool) {
        let mut loaded = None;
        let mut checked = 0;
        for row in fixture.lines().skip(1) {
            let columns: Vec<_> = row.split('\t').collect();
            let id: usize = columns[0][1..3].parse().unwrap();
            if loaded.as_ref().is_none_or(|(previous, _)| *previous != id) {
                loaded = Some((
                    id,
                    PuzzleAsset::load(Path::new(env!("CARGO_MANIFEST_DIR")), id, use_ega).unwrap(),
                ));
            }
            let values: Vec<i32> = columns[1..9].iter().map(|v| v.parse().unwrap()).collect();
            let sprite = &loaded.as_ref().unwrap().1.sprites[values[0] as usize];
            let mut fb = Framebuffer::new();
            fb.clear(if use_ega { 15 } else { 255 });
            fb.set_clip(values[4], values[5], values[6], values[7]);
            fb.blit_sprite(sprite, values[1], values[2], values[3] as u16);
            let checksum = fb.pixels.iter().fold(0xcbf29ce484222325u64, |hash, &byte| {
                (hash ^ byte as u64).wrapping_mul(0x100000001b3)
            });
            assert_eq!(
                checksum,
                u64::from_str_radix(columns[9], 16).unwrap(),
                "{row}"
            );
            checked += 1;
        }
        assert_eq!(checked, if use_ega { 2984 } else { 3684 });
    }

    #[cfg_attr(
        not(feature = "asset-tests"),
        ignore = "requires original DOS game assets"
    )]
    #[test]
    fn nonzero_sprite_keys_preserve_rendering_and_hit_testing() {
        for (id, index, key) in [(7, 38, 2), (10, 24, 246), (10, 37, 2)] {
            let mut asset =
                PuzzleAsset::load(std::path::Path::new(env!("CARGO_MANIFEST_DIR")), id, false)
                    .unwrap();
            let sprite = asset.sprites[index].clone();
            assert_eq!(sprite.transparent_color, key);
            for flags in 0..4 {
                asset.records = vec![SceneRecord::Frame {
                    bg_frame_idx: 0,
                    bg_color: 0,
                    commands: vec![FrameCommand::Sprite {
                        x: 0,
                        y: 0,
                        sprite_idx: index,
                        flags,
                    }],
                }];
                let mut fb = Framebuffer::new();
                fb.clear(255);
                fb.blit_sprite(&sprite, 0, 0, flags);
                for y in 0..sprite.sub_h as i32 {
                    for x in 0..sprite.width as i32 {
                        let sx = sprite.sub_x as i32
                            + if flags & 1 != 0 {
                                sprite.width as i32 - 1 - x
                            } else {
                                x
                            };
                        let sy = sprite.sub_y as i32
                            + if flags & 2 != 0 {
                                sprite.sub_h as i32 - 1 - y
                            } else {
                                y
                            };
                        let color = sprite.pixels[(sy * sprite.stride as i32 + sx) as usize];
                        assert_eq!(fb.get_pixel(x, y), if color == key { 255 } else { color });
                        assert_eq!(hit_test_frame(&asset, 0, 0, 0, 0, x, y, 0), color != key);
                    }
                }
            }
        }
    }

    #[cfg_attr(
        not(feature = "asset-tests"),
        ignore = "requires original DOS game assets"
    )]
    #[test]
    fn dissolve_orders_match_original_graphics_drivers() {
        for row in include_str!("../validation/dissolve-order.tsv")
            .lines()
            .skip(1)
        {
            let fields: Vec<_> = row.split('\t').collect();
            let order = dissolve_order(fields[0] == "EGA");
            assert_eq!(order.len(), fields[1].parse::<usize>().unwrap());
            let mut seen = vec![false; 64000];
            let mut checksum = 0xcbf29ce484222325u64;
            for &pixel in order {
                assert!(!seen[pixel as usize]);
                seen[pixel as usize] = true;
                for byte in pixel.to_le_bytes() {
                    checksum = (checksum ^ u64::from(byte)).wrapping_mul(0x100000001b3);
                }
            }
            assert!(seen.into_iter().all(|v| v));
            assert_eq!(
                checksum,
                u64::from_str_radix(fields[2], 16).unwrap(),
                "{row}"
            );
            let first: Vec<u16> = fields[3].split(',').map(|v| v.parse().unwrap()).collect();
            let last: Vec<u16> = fields[4].split(',').map(|v| v.parse().unwrap()).collect();
            assert_eq!(&order[..first.len()], first);
            assert_eq!(&order[order.len() - last.len()..], last);
        }
    }

    #[cfg_attr(
        not(feature = "asset-tests"),
        ignore = "requires original DOS game assets"
    )]
    #[test]
    fn scene_view_preserves_native_pixels_and_pattern_phase() {
        let row = include_str!("../validation/scene-viewport.tsv")
            .lines()
            .nth(1)
            .unwrap();
        let v: Vec<i32> = row.split('\t').map(|word| word.parse().unwrap()).collect();
        assert_eq!(&v[1..], &[2, 2, 270, 196]);
        for ega in [false, true] {
            let asset = PuzzleAsset::load(Path::new(env!("CARGO_MANIFEST_DIR")), 11, ega).unwrap();
            let mut native = Framebuffer::new();
            render_scene_frame(&mut native, &asset, 168, 0, 0, 0, true, 0, 0);
            let mut view = Framebuffer::new();
            render_scene_view(&mut view, &asset, 168);
            let black = view.color_index([0; 3]);
            for y in 0..SCREEN_H {
                for x in 0..SCREEN_W {
                    let sx = x - SCENE_OFFSET_X;
                    let expected =
                        if (v[1]..v[1] + v[3]).contains(&sx) && (v[2]..v[2] + v[4]).contains(&y) {
                            native.get_pixel(sx, y)
                        } else {
                            black
                        };
                    assert_eq!(view.get_pixel(x, y), expected, "ega={ega} {x},{y}");
                }
            }
        }
    }

    #[cfg_attr(
        not(feature = "asset-tests"),
        ignore = "requires original DOS game assets"
    )]
    #[test]
    fn scene_background_preserves_both_pattern_colors() {
        for ega in [false, true] {
            let mut asset =
                PuzzleAsset::load(Path::new(env!("CARGO_MANIFEST_DIR")), 1, ega).unwrap();
            asset.records = vec![SceneRecord::Frame {
                bg_frame_idx: 0,
                bg_color: 0x0204,
                commands: Vec::new(),
            }];
            let mut fb = Framebuffer::new();
            fb.clear(99);
            fb.set_clip(51, 41, 31, 21);
            render_scene_frame(&mut fb, &asset, 0, 0, 0, 0, true, 0, 0);
            for y in 0..SCREEN_H {
                for x in 0..SCREEN_W {
                    let expected = if (51..82).contains(&x) && (41..62).contains(&y) {
                        if (x + y) & 1 == 0 { 4 } else { 2 }
                    } else {
                        99
                    };
                    assert_eq!(fb.get_pixel(x, y), expected, "ega={ega} {x},{y}");
                }
            }
        }
    }

    #[cfg_attr(
        not(feature = "asset-tests"),
        ignore = "requires original DOS game assets"
    )]
    #[test]
    fn scene_background_respects_the_viewport() {
        let mut asset = PuzzleAsset::load(Path::new(env!("CARGO_MANIFEST_DIR")), 1, false).unwrap();
        asset.records = vec![SceneRecord::Frame {
            bg_frame_idx: 0,
            bg_color: 0x0505,
            commands: Vec::new(),
        }];
        let mut fb = Framebuffer::new();
        fb.clear(99);
        fb.set_clip(50, 40, 30, 20);
        render_scene_frame(&mut fb, &asset, 0, 0, 0, 0, true, 0, 0);
        for y in 0..SCREEN_H {
            for x in 0..SCREEN_W {
                let inside = (50..80).contains(&x) && (40..60).contains(&y);
                assert_eq!(fb.get_pixel(x, y), if inside { 5 } else { 99 }, "{x},{y}");
            }
        }
    }

    #[cfg_attr(
        not(feature = "asset-tests"),
        ignore = "requires original DOS game assets"
    )]
    #[test]
    fn ega_rectangle_pattern_matches_original_driver() {
        let mut colors = std::collections::BTreeSet::new();
        for ega in [false, true] {
            for id in 1..=15 {
                let asset =
                    PuzzleAsset::load(Path::new(env!("CARGO_MANIFEST_DIR")), id, ega).unwrap();
                for (index, record) in asset.records.iter().enumerate() {
                    if let SceneRecord::Frame { commands, .. } = record {
                        for command in commands {
                            if let FrameCommand::Rect { color, flags, .. } = command {
                                if color >> 8 != color & 255 {
                                    colors.insert(*color);
                                    println!(
                                        "P{id:02} ega={ega} record={index} pattern={color:#06x}"
                                    );
                                }
                                assert!(*flags <= 1);
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(colors.into_iter().collect::<Vec<_>>(), vec![0x090e]);
        let mut supplied =
            PuzzleAsset::load(Path::new(env!("CARGO_MANIFEST_DIR")), 11, true).unwrap();
        let mut patterned = Framebuffer::new();
        render_scene_frame(&mut patterned, &supplied, 168, 0, 0, 0, true, 0, 0);
        let SceneRecord::Frame { commands, .. } = &mut supplied.records[168] else {
            panic!("P11 frame");
        };
        for command in commands {
            if let FrameCommand::Rect { color, .. } = command
                && *color == 0x090e
            {
                *color = 0x0e0e;
            }
        }
        let mut flattened = Framebuffer::new();
        render_scene_frame(&mut flattened, &supplied, 168, 0, 0, 0, true, 0, 0);
        assert_ne!(patterned.pixels, flattened.pixels);
        let mut asset = PuzzleAsset::load(Path::new(env!("CARGO_MANIFEST_DIR")), 1, true).unwrap();
        asset.records = vec![SceneRecord::Frame {
            bg_frame_idx: 0,
            bg_color: 0,
            commands: vec![FrameCommand::Rect {
                x: 3,
                y: 5,
                color: 0x090e,
                flags: 0,
                width: 9,
                height: 4,
            }],
        }];
        let mut fb = Framebuffer::new();
        fb.clear(255);
        render_scene_frame(&mut fb, &asset, 0, 0, 0, 0, false, 0, 0);
        for row in include_str!("../validation/ega-rectangle.tsv")
            .lines()
            .skip(1)
        {
            let values: Vec<i32> = row.split('\t').map(|v| v.parse().unwrap()).collect();
            assert_eq!(fb.get_pixel(values[0], values[1]), values[2] as u8, "{row}");
        }
        let mut clipped = Framebuffer::new();
        clipped.clear(255);
        clipped.set_clip(5, 6, 4, 2);
        render_scene_frame(&mut clipped, &asset, 0, 0, 0, 0, false, 0, 0);
        for y in 0..10 {
            for x in 0..16 {
                let expected = if (5..9).contains(&x) && (6..8).contains(&y) {
                    fb.get_pixel(x, y)
                } else {
                    255
                };
                assert_eq!(clipped.get_pixel(x, y), expected);
            }
        }
    }

    #[cfg_attr(
        not(feature = "asset-tests"),
        ignore = "requires original DOS game assets"
    )]
    #[test]
    fn preset_piece_bounds_match_original_machine_code() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut assets = std::collections::HashMap::new();
        let mut checked = 0;
        for row in include_str!("../validation/preset-bounds.tsv")
            .lines()
            .skip(1)
        {
            let fields: Vec<_> = row.split('\t').collect();
            let asset = assets.entry(fields[0]).or_insert_with(|| {
                let id = fields[0][1..3].parse().unwrap();
                PuzzleAsset::load(dir, id, fields[0].ends_with("PZ4")).unwrap()
            });
            let values: Vec<i32> = fields[1..].iter().map(|v| v.parse().unwrap()).collect();
            let SceneRecord::Frame { commands, .. } = &asset.records[4] else {
                panic!("Missing preset frame: {row}");
            };
            let command = commands
                .iter()
                .filter(|command| matches!(command, FrameCommand::SubFrame { .. }))
                .nth(values[0] as usize)
                .unwrap();
            assert!(
                matches!(command, FrameCommand::SubFrame { dx, dy, frame_idx, flags }
                if *frame_idx == values[1] as usize && i32::from(*dx) == values[2]
                    && i32::from(*dy) == values[3] && i32::from(*flags) == values[4]),
                "{row}"
            );
            let bounds = compute_frame_bounds(
                asset,
                values[1] as usize,
                values[2],
                values[3],
                values[4] as u16 & 3,
                0,
            );
            assert_eq!(
                bounds,
                (values[5], values[6], values[7], values[8]),
                "{row}"
            );
            checked += 1;
        }
        assert_eq!(assets.len(), 27);
        assert_eq!(checked, 349);
    }

    #[cfg_attr(
        not(feature = "asset-tests"),
        ignore = "requires original DOS game assets"
    )]
    #[test]
    fn nested_reflection_bounds_match_original_machine_code() {
        let mut asset = PuzzleAsset::load(Path::new(env!("CARGO_MANIFEST_DIR")), 1, false).unwrap();
        let frame = |commands| SceneRecord::Frame {
            bg_frame_idx: 0,
            bg_color: 0,
            commands,
        };
        let mut checked = 0;
        for row in include_str!("../validation/frame-bounds.tsv")
            .lines()
            .skip(1)
        {
            let values: Vec<i32> = row.split('\t').map(|v| v.parse().unwrap()).collect();
            asset.records = vec![
                frame(vec![FrameCommand::SubFrame {
                    dx: 5,
                    dy: 9,
                    frame_idx: 1,
                    flags: values[1] as u16,
                }]),
                frame(vec![FrameCommand::SubFrame {
                    dx: -3,
                    dy: 4,
                    frame_idx: 2,
                    flags: values[2] as u16,
                }]),
                frame(vec![FrameCommand::Rect {
                    x: 7,
                    y: 11,
                    color: 5,
                    flags: 0,
                    width: 13,
                    height: 17,
                }]),
            ];
            let bounds = compute_frame_bounds(&asset, 0, 13, 19, values[0] as u16, 0);
            assert_eq!(
                bounds,
                (values[3], values[4], values[5], values[6]),
                "{row}"
            );
            let mut fb = Framebuffer::new();
            fb.clear(0);
            render_scene_frame(&mut fb, &asset, 0, 13, 19, values[0] as u16, false, 0, 0);
            for y in 0..SCREEN_H {
                for x in 0..SCREEN_W {
                    let inside = x >= values[3]
                        && x < values[3] + values[5]
                        && y >= values[4]
                        && y < values[4] + values[6];
                    assert_eq!(
                        fb.get_pixel(x, y),
                        if inside { 5 } else { 0 },
                        "{row}: {x},{y}"
                    );
                }
            }
            for (x, y, expected) in [
                (values[3], values[4], true),
                (values[3] + values[5] - 1, values[4] + values[6] - 1, true),
                (values[3] - 1, values[4], false),
                (values[3] + values[5], values[4], false),
            ] {
                assert_eq!(
                    hit_test_frame(&asset, 0, 13, 19, values[0] as u16, x, y, 0),
                    expected,
                    "{row}: {x},{y}"
                );
            }
            checked += 1;
        }
        assert_eq!(checked, 64);
    }

    #[test]
    fn blit_sprite_clips_all_edges_and_flips() {
        let sprite = Sprite8061 {
            stride: 3,
            height: 2,
            transparent_color: 0,
            bpp: 8,
            sub_x: 0,
            sub_y: 0,
            width: 3,
            sub_h: 2,
            pixels: vec![1, 2, 3, 4, 0, 6],
            opacity: None,
        };
        let mut fb = Framebuffer::new();
        fb.set_clip(10, 10, 4, 4);
        // Clip left/top edge with horizontal flip: row 0 becomes [3, 2, 1], x=9 clips col 0 (3).
        fb.blit_sprite(&sprite, 9, 9, 1);
        assert_eq!(fb.get_pixel(10, 10), 0); // (dx=1, dy=1) -> pixel (1,1) = 0 (transparent)
        assert_eq!(fb.get_pixel(11, 10), 4); // (dx=2, dy=1) -> flipped x=0, y=1 = 4
        assert_eq!(fb.get_pixel(9, 10), 0); // outside clip_x

        // Reset palette restores EGA16 after overwrite.
        fb.palette[1] = [99, 99, 99];
        fb.reset_default_palette();
        assert_eq!(fb.palette[1], DEFAULT_EGA16_PALETTE[1]);
    }

    #[cfg_attr(
        not(feature = "asset-tests"),
        ignore = "requires original DOS game assets"
    )]
    #[test]
    fn blit_sprite_modes_and_offset_speech_bubble_match() {
        let sprite = Sprite8061 {
            stride: 3,
            height: 2,
            transparent_color: 0,
            bpp: 8,
            sub_x: 0,
            sub_y: 0,
            width: 3,
            sub_h: 2,
            pixels: vec![1, 0, 3, 4, 5, 0],
            opacity: None,
        };
        let mut fb = Framebuffer::new();
        fb.clear(9);
        // Mode 1 opaque copy (unflipped) writes transparent pixels too.
        fb.blit_sprite(&sprite, 5, 5, 0x0010);
        assert_eq!(fb.get_pixel(6, 5), 0);
        assert_eq!(fb.get_pixel(7, 5), 3);
        // Mode 1 opaque copy (flipped X).
        fb.blit_sprite(&sprite, 15, 5, 0x0011);
        assert_eq!(fb.get_pixel(15, 5), 3);
        assert_eq!(fb.get_pixel(16, 5), 0);
        assert_eq!(fb.get_pixel(17, 5), 1);
        // Mode 2 solid silhouette (color 7).
        fb.blit_sprite(&sprite, 25, 5, 0x0720);
        assert_eq!(fb.get_pixel(25, 5), 7);
        assert_eq!(fb.get_pixel(26, 5), 9);

        let font = FontAsset::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("MJP.NFT")).unwrap();
        let bubble = crate::assets::SpeechBubble {
            points: vec![(10, 10), (70, 10), (70, 35), (10, 35)],
            text_x: 14,
            text_y: 14,
            text: b"HI!".to_vec(),
        };
        let mut shifted = bubble.clone();
        for (x, _) in &mut shifted.points {
            *x += SCENE_OFFSET_X;
        }
        shifted.text_x += SCENE_OFFSET_X;

        let mut expected = Framebuffer::new();
        expected.draw_speech_bubble(&shifted, &font, 15, 1);
        let mut actual = Framebuffer::new();
        actual.draw_speech_bubble_offset(&bubble, &font, SCENE_OFFSET_X, 0, 15, 1);
        assert_eq!(actual.pixels, expected.pixels);
    }

    #[test]
    fn fill_polygon_offset_handles_negative_and_degenerate_polys() {
        let mut fb = Framebuffer::new();
        fb.clear(0);
        fb.fill_polygon_offset(&[(0, 0), (10, 10)], 0, 0, 5);
        assert!(fb.pixels.iter().all(|&p| p == 0));

        fb.fill_polygon_offset(&[(-20, -30), (20, -30), (0, -5)], 0, 0, 5);
        assert!(fb.pixels.iter().all(|&p| p == 0));
    }

    #[test]
    fn traversal_policy_distinguishes_render_and_geometry_flags() {
        use crate::assets::{FrameCommand, PuzzleAsset, SceneRecord};

        let asset = PuzzleAsset {
            id: 0,
            palette: [[0; 3]; 256],
            records: vec![
                // Record 0: top-level frame with a `0x0030` sub-frame and a `0x0720` sub-frame
                SceneRecord::Frame {
                    bg_frame_idx: 0,
                    bg_color: 0,
                    commands: vec![
                        FrameCommand::SubFrame {
                            dx: 0,
                            dy: 0,
                            frame_idx: 1,
                            flags: 0x0030,
                        },
                        FrameCommand::SubFrame {
                            dx: 0,
                            dy: 0,
                            frame_idx: 2,
                            flags: 0x0720,
                        },
                    ],
                },
                // Record 1: skipped by Render at depth 0, but included by Geometry
                SceneRecord::Frame {
                    bg_frame_idx: 0,
                    bg_color: 0,
                    commands: vec![FrameCommand::Rect {
                        x: 10,
                        y: 10,
                        color: 9,
                        flags: 0,
                        width: 4,
                        height: 4,
                    }],
                },
                // Record 2: high flags `0x0720` override rect color to `7` during Render
                SceneRecord::Frame {
                    bg_frame_idx: 0,
                    bg_color: 0,
                    commands: vec![FrameCommand::Rect {
                        x: 20,
                        y: 20,
                        color: 0x0303,
                        flags: 0,
                        width: 4,
                        height: 4,
                    }],
                },
            ],
            sequences: Vec::new(),
            hotspots: Vec::new(),
            full_anim_seq_idx: 0,
            sounds: Vec::new(),
            sprites: Vec::new(),
        };

        // Geometry includes both the `0x0030` sub-frame at (10,10) and the `0x0720` sub-frame at (20,20)
        assert_eq!(
            compute_frame_bounds(&asset, 0, 0, 0, 0, 0),
            (10, 10, 14, 14)
        );
        assert!(hit_test_frame(&asset, 0, 0, 0, 0, 11, 11, 0));
        assert!(hit_test_frame(&asset, 0, 0, 0, 0, 21, 21, 0));

        // Render skips the `0x0030` sub-frame at depth 0 and propagates `0x0720` color override (`7`)
        let mut fb = Framebuffer::new();
        render_scene_frame(&mut fb, &asset, 0, 0, 0, 0, false, 0, 0);
        assert_eq!(fb.get_pixel(11, 11), 0);
        assert_eq!(fb.get_pixel(21, 21), 7);
    }

    #[cfg_attr(
        not(feature = "asset-tests"),
        ignore = "requires original DOS game assets"
    )]
    #[test]
    fn apply_dissolve_step_matches_render_scene_dissolve() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        for ega in [false, true] {
            let asset = PuzzleAsset::load(dir, 1, ega).unwrap();
            let seq = &asset.sequences[1];
            let prev_frame = seq.start_frame;
            let next_frame = (seq.start_frame + 1).min(seq.end_frame - 1);

            let mut prev_fb = Framebuffer::new();
            let mut next_fb = Framebuffer::new();
            let mut scratch = Framebuffer::new();
            render_scene_view_with_scratch(&mut prev_fb, &mut scratch, &asset, prev_frame);
            render_scene_view_with_scratch(&mut next_fb, &mut scratch, &asset, next_frame);

            let mut expected = Framebuffer::new();
            let mut actual = Framebuffer::new();
            let mut scratch_next = Framebuffer::new();
            for progress in [0.0, 0.25, 0.5, 0.75, 1.0] {
                render_scene_dissolve_with_scratch(
                    &mut expected,
                    &mut scratch,
                    &mut scratch_next,
                    &asset,
                    prev_frame,
                    next_frame,
                    progress,
                );
                apply_dissolve_step(&mut actual, &prev_fb, &next_fb, ega, progress);
                assert_eq!(
                    actual.pixels, expected.pixels,
                    "dissolve mismatch for ega={ega} progress={progress}"
                );
            }
        }
    }

    #[test]
    fn fill_polygon_offset_handles_extreme_signed_offsets() {
        let mut fb = Framebuffer::new();
        fb.clear(0);
        fb.fill_polygon_offset(
            &[(i32::MIN, i32::MIN), (i32::MAX, i32::MIN), (0, i32::MAX)],
            -10,
            -10,
            7,
        );
        assert_eq!(fb.get_pixel(10, 10), 7);

        fb.clear(0);
        fb.fill_polygon_offset(
            &[
                (i32::MIN, i32::MIN),
                (i32::MAX, i32::MAX),
                (i32::MIN, i32::MAX),
            ],
            0,
            -1,
            9,
        );
        assert_eq!(fb.get_pixel(0, 0), 9);
    }

    #[test]
    fn blit_thumbnail_ignores_truncated_or_out_of_bounds_sprites() {
        let mut fb = Framebuffer::new();
        fb.clear(3);
        let truncated = Sprite8061 {
            stride: 4,
            height: 4,
            transparent_color: 0,
            bpp: 8,
            sub_x: 0,
            sub_y: 0,
            width: 4,
            sub_h: 4,
            pixels: vec![1; 8],
            opacity: None,
        };
        fb.blit_thumbnail(&truncated, 0, 0, 4, 4);
        assert_eq!(fb.get_pixel(0, 0), 3);

        let oob_sub = Sprite8061 {
            stride: 4,
            height: 4,
            transparent_color: 0,
            bpp: 8,
            sub_x: 2,
            sub_y: 0,
            width: 4,
            sub_h: 4,
            pixels: vec![1; 16],
            opacity: None,
        };
        fb.blit_thumbnail(&oob_sub, 0, 0, 4, 4);
        assert_eq!(fb.get_pixel(0, 0), 3);

        let short_opacity = Sprite8061 {
            stride: 4,
            height: 4,
            transparent_color: 0,
            bpp: 8,
            sub_x: 0,
            sub_y: 0,
            width: 4,
            sub_h: 4,
            pixels: vec![1; 16],
            opacity: Some(vec![true; 8]),
        };
        fb.blit_thumbnail(&short_opacity, 0, 0, 4, 4);
        fb.blit_sprite(&short_opacity, 0, 0, 0);
        assert_eq!(fb.get_pixel(0, 0), 3);
    }
}
