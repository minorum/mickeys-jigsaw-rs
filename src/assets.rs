//! Asset loaders for Mickey's Jigsaw Puzzles (1991, DOS):
//! - `MJP.NFT`: Proportional 1-bit font (`seg_0cd2:02fb`, `seg_07ac:01f8`)
//! - `MJP.TTL` & `MJP.CRD`: Title and Credits IFF `ILBM` screens (`seg_0701`)
//! - `MJP.TBX`: Toolbox palette & 65 `0x8061` UI sprites (`seg_0cd2:0631`)
//! - `P01..P15.PZ8` & `.PZ4`: Puzzle scene graph (`0x7ff0`, `0x7ff1`), audio/speech-bubbles (`0x8023`), and sprites (`0x8061`) (`seg_0cd2:0d15`)
//! - `P01..P15.PRT`: Monochrome 320x200 printable line-art pictures
//! - `MJP.PRE`: 28-byte preferences file (`seg_004f`)

use std::fs;
use std::path::Path;

use crate::decompress::{IlbmImage, decode_ilbm, decompress_da, split_multi_resource};

#[inline]
fn le_u16(buf: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([buf[off], buf[off + 1]])
}

#[inline]
fn le_i16(buf: &[u8], off: usize) -> i16 {
    i16::from_le_bytes([buf[off], buf[off + 1]])
}

#[inline]
fn le_u32(buf: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]])
}

fn read_file(path: &Path) -> Result<Vec<u8>, String> {
    fs::read(path).map_err(|e| format!("Failed to read {path:?}: {e}"))
}

fn read_da(path: &Path) -> Result<Vec<u8>, String> {
    decompress_da(&read_file(path)?)
}

fn copy_palette(dst: &mut [[u8; 3]; 256], src: &[u8], count: usize) {
    for (color, &rgb) in dst.iter_mut().take(count).zip(src.as_chunks::<3>().0) {
        *color = rgb;
    }
}

#[inline]
fn read_tagged_chunk<'a>(
    buf: &'a [u8],
    pos: &mut usize,
    err: &str,
) -> Result<(&'a [u8], &'a [u8]), String> {
    let header_end = pos
        .checked_add(8)
        .filter(|&end| end <= buf.len())
        .ok_or_else(|| err.to_string())?;
    let header = buf.get(*pos..header_end).ok_or_else(|| err.to_string())?;
    let tag = &header[0..4];
    let size = le_u32(header, 4) as usize;
    let end = header_end
        .checked_add(size)
        .filter(|&end| end <= buf.len())
        .ok_or_else(|| err.to_string())?;
    *pos = end;
    Ok((tag, &buf[header_end..end]))
}

/// Proportional 1-bit font loaded from `MJP.NFT` (`616 x 10` strip, ASCII `32..127`).
#[derive(Clone, Debug)]
pub struct FontAsset {
    pub name: String,
    pub strip_width: u16,
    pub height: u16,
    pub stride_bytes: usize,
    pub char_x: [u16; 96],
    pub char_w: [u16; 96],
    /// Unpacked 1-bit bitmap (`height * stride_bytes` = `10 * 77 = 770` bytes, MSB first).
    pub bitmap: Vec<u8>,
}

impl FontAsset {
    pub fn load(path: &Path) -> Result<Self, String> {
        Self::from_bytes(&read_file(path)?)
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, String> {
        if data.len() < 405 || &data[0..4] != b"FONT" {
            return Err("Invalid MJP.NFT header".to_string());
        }
        let name = String::from_utf8_lossy(&data[8..16]).trim().to_string();
        let strip_width = le_u16(data, 16);
        let height = le_u16(data, 18);
        let stride_bytes = (strip_width as usize).div_ceil(8);

        let mut char_x = [0u16; 96];
        let mut char_w = [0u16; 96];
        for i in 0..96 {
            char_x[i] = le_u16(data, 20 + i * 2);
            char_w[i] = le_u16(data, 212 + i * 2);
        }

        if strip_width == 0
            || height == 0
            || char_x
                .iter()
                .zip(&char_w)
                .any(|(&x, &w)| usize::from(x) + usize::from(w) > usize::from(strip_width))
        {
            return Err("Invalid font dimensions or glyph bounds".to_string());
        }

        let mut pos = 405usize;
        let mut bitmap = Vec::new();
        for _ in 0..height {
            let row_len = usize::from(*data.get(pos).ok_or("Truncated font row")?);
            let row_end = pos + row_len;
            if row_len < 2 || row_end > data.len() {
                return Err("Invalid font row length".to_string());
            }
            pos += 1;
            let row_start = bitmap.len();
            while pos < row_end {
                let cnt = data[pos] as usize;
                pos += 1;
                let end = pos + cnt;
                if end > row_end || bitmap.len() - row_start + cnt > stride_bytes {
                    return Err("Invalid font row packet".to_string());
                }
                bitmap.extend_from_slice(&data[pos..end]);
                pos = end;
            }
            if bitmap.len() - row_start != stride_bytes {
                return Err("Incomplete font row".to_string());
            }
        }

        Ok(Self {
            name,
            strip_width,
            height,
            stride_bytes,
            char_x,
            char_w,
            bitmap,
        })
    }

    /// Measures the pixel width of a string rendered in this font.
    pub fn measure_text(&self, text: &str) -> i32 {
        let mut max_w = 0i32;
        let mut cur_w = 0i32;
        for ch in text.chars() {
            if ch == '\r' || ch == '\n' {
                max_w = max_w.max(cur_w);
                cur_w = 0;
                continue;
            }
            let c = ch as u32;
            if (32..128).contains(&c) {
                cur_w += self.char_w[(c - 32) as usize] as i32;
            }
        }
        max_w.max(cur_w)
    }
}

/// A `0x8061` sprite bitmap used in `MJP.TBX` and `P01..P15.PZ8`/`.PZ4`.
#[derive(Clone, Debug)]
pub struct Sprite8061 {
    pub stride: u16,
    pub height: u16,
    pub transparent_color: u8,
    pub bpp: u16,
    pub sub_x: u16,
    pub sub_y: u16,
    pub width: u16,
    pub sub_h: u16,
    /// Unpacked 8-bit chunky pixel indices (`stride * height` bytes).
    pub pixels: Vec<u8>,
    pub opacity: Option<Vec<bool>>,
}

impl Sprite8061 {
    pub fn from_8bpp(
        width: i32,
        height: i32,
        transparent_color: u8,
        pixels: Vec<u8>,
        opacity: Vec<bool>,
    ) -> Self {
        Self {
            stride: width as u16,
            height: height as u16,
            transparent_color,
            bpp: 8,
            sub_x: 0,
            sub_y: 0,
            width: width as u16,
            sub_h: height as u16,
            pixels,
            opacity: Some(opacity),
        }
    }

    pub fn is_opaque(&self, index: usize) -> bool {
        self.pixels.get(index).is_some_and(|&color| {
            self.opacity
                .as_ref()
                .map_or(color != self.transparent_color, |mask| {
                    mask.get(index) == Some(&true)
                })
        })
    }

    pub fn from_record(rec: &[u8]) -> Result<Self, String> {
        if rec.len() < 26 {
            return Err("0x8061 record too short".to_string());
        }
        let magic = le_u16(rec, 0);
        if magic != 0x8061 {
            return Err(format!("Expected 0x8061 sprite, got 0x{:04x}", magic));
        }
        let stride = le_u16(rec, 10);
        let height = le_u16(rec, 12);
        let transparent_color = rec[17];
        let bpp = u16::from(rec[16]);
        let sub_x = le_u16(rec, 18);
        let sub_y = le_u16(rec, 20);
        let width = le_u16(rec, 22);
        let sub_h = le_u16(rec, 24);

        let raw = &rec[26..];
        let total_px = (stride as usize) * (height as usize);
        if ![2, 4, 8].contains(&bpp)
            || stride == 0
            || height == 0
            || sub_x as usize + width as usize > stride as usize
            || sub_y as usize + sub_h as usize > height as usize
            || raw.len() < (stride as usize * bpp as usize).div_ceil(8) * height as usize
        {
            return Err(format!(
                "Invalid sprite: stride={stride} height={height} bpp={bpp} sub={sub_x},{sub_y},{width},{sub_h} bytes={}",
                raw.len()
            ));
        }
        let mut pixels = Vec::with_capacity(total_px);
        let w = stride as usize;
        let h = height as usize;

        match bpp {
            8 => pixels.extend_from_slice(&raw[..total_px]),
            4 => {
                // 4-bit packed nibbles (`TND0` / EGA): high nibble first, then low nibble
                let row_bytes = w.div_ceil(2);
                let pairs = w / 2;
                let odd = (w & 1) != 0;
                for row in raw.chunks_exact(row_bytes).take(h) {
                    for &b in &row[..pairs] {
                        pixels.extend_from_slice(&[b >> 4, b & 0x0F]);
                    }
                    if odd {
                        pixels.push(row[pairs] >> 4);
                    }
                }
            }
            _ => {
                // 2-bit packed crumbs (CGA), MSB first
                let row_bytes = w.div_ceil(4);
                for row in raw.chunks_exact(row_bytes).take(h) {
                    for x in 0..w {
                        let shift = 6 - ((x & 3) * 2);
                        pixels.push((row[x >> 2] >> shift) & 0x03);
                    }
                }
            }
        }

        Ok(Self {
            stride,
            height,
            transparent_color,
            opacity: None,
            bpp,
            sub_x,
            sub_y,
            width,
            sub_h,
            pixels,
        })
    }
}

/// Toolbox asset loaded from `MJP.TBX` (`seg_0cd2:0631`).
#[derive(Clone, Debug)]
pub struct ToolboxAsset {
    /// 256-color RGB palette.
    pub palette: [[u8; 3]; 256],
    /// UI sprites.
    pub sprites: Vec<Sprite8061>,
}

impl ToolboxAsset {
    pub fn load(path: &Path, chunk_idx: usize) -> Result<Self, String> {
        let dec = read_da(path)?;
        let chunks = split_multi_resource(&dec)?;
        let buf = chunks
            .get(chunk_idx)
            .ok_or_else(|| format!("Toolbox variant {chunk_idx} unavailable"))?;
        Self::parse_chunk(buf)
    }

    fn parse_chunk(buf: &[u8]) -> Result<Self, String> {
        let mut pos = 0usize;
        let mut palette = [[0u8; 3]; 256];
        let mut sprites = Vec::new();
        let mut palette_payload = None;

        while pos + 8 <= buf.len() {
            let (tag, chunk) = read_tagged_chunk(buf, &mut pos, "Truncated toolbox chunk")?;

            if tag == b"PALE" {
                palette_payload = Some(chunk);
            } else if tag == b"TBOX" {
                let mut sp = 0usize;
                while sp + 6 <= chunk.len() {
                    let rtype = le_u16(chunk, sp);
                    let rsz = le_u32(chunk, sp + 2) as usize;
                    if rsz < 6 {
                        return Err("Invalid toolbox record size".to_string());
                    }
                    let record_end = sp
                        .checked_add(rsz)
                        .filter(|&end| end <= chunk.len())
                        .ok_or("Truncated toolbox record")?;
                    if rtype == 0x8061 {
                        sprites.push(Sprite8061::from_record(&chunk[sp..record_end])?);
                    }
                    sp = record_end;
                }
                if sp != chunk.len() {
                    return Err("Truncated toolbox record header".to_string());
                }
            }
        }

        if pos != buf.len() {
            return Err("Truncated toolbox chunk header".to_string());
        }
        if sprites.is_empty() {
            return Err("Toolbox contains no sprites".to_string());
        }
        if let Some(colors) = palette_payload {
            let count = sprites
                .iter()
                .map(|sprite| 1usize << sprite.bpp)
                .max()
                .unwrap_or(0);
            if colors.len() < count * 3 {
                return Err(format!("Truncated toolbox palette for {count} colors"));
            }
            copy_palette(&mut palette, colors, count);
        }
        Ok(Self { palette, sprites })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SequencePlayback {
    pub seq_idx: usize,
    pub repetitions: u16,
    pub frame_delay_ticks: u16,
}

#[derive(Clone, Debug)]
pub struct Hotspot {
    pub playback: SequencePlayback,
    pub x: i16,
    pub y: i16,
    pub width: i16,
    pub height: i16,
}

/// Sub-command inside a `0x7ff1` scene/animation frame (`seg_096c:14f0`, `17a3`).
#[derive(Clone, Debug)]
pub enum FrameCommand {
    /// `0x0020`: Trigger `0x8023` sound/speech-bubble (`c774` 0-based index, or `None` if 0/0x7fff).
    Sound { snd_idx: Option<usize> },
    /// `0x0030`: Frame delay word (`delay_raw`; tick count is `((delay_raw >> 6) & 0x3FF)`).
    Delay { raw: u16, ticks: u16 },
    /// `0x0040`: Recursive `0x7ff1` sub-frame (`c7c4` 0-based index).
    SubFrame {
        dx: i16,
        dy: i16,
        frame_idx: usize,
        flags: u16,
    },
    /// `0x0050`: Filled rectangle.
    Rect {
        x: i16,
        y: i16,
        color: u16,
        flags: u16,
        width: i16,
        height: i16,
    },
    /// `0x0060`: Draw `0x8061` sprite (`aeb6` 0-based index).
    Sprite {
        x: i16,
        y: i16,
        sprite_idx: usize,
        flags: u16,
    },
    /// `0x0080`: Draw text resource (`aeb6` 0-based index).
    Text {
        x: i16,
        y: i16,
        res_idx: usize,
        flags: u16,
        font_and_color: u16,
    },
    /// `0x0090`: Draw polygon resource (`aeb6` 0-based index).
    Polygon {
        x: i16,
        y: i16,
        res_idx: usize,
        flags: u16,
        params: [i16; 8],
    },
}

/// Entry in `c7c4` (`0x7ff0` sequence header or `0x7ff1` scene/animation frame).
#[derive(Clone, Debug)]
pub enum SceneRecord {
    SequenceHeader {
        name: String,
        default_playback: Option<SequencePlayback>,
        hotspots: Vec<Hotspot>,
    },
    Frame {
        bg_frame_idx: usize,
        bg_color: u16,
        commands: Vec<FrameCommand>,
    },
}

/// Audio descriptor inside a `0x8023` (`c774`) sound/speech-bubble entry.
#[derive(Clone, Debug)]
pub struct SoundDescriptor {
    pub stem: String,
    pub phrase_idx: u16,
}

/// `0x8023` sound & speech-bubble entry in `c774`.
#[derive(Clone, Debug)]
pub struct SoundBubbleEntry {
    pub tag: String,
    pub sounds: Vec<SoundDescriptor>,
    pub selection_mode: u16,
    pub bubbles: Vec<Option<SpeechBubble>>,
}

impl SoundBubbleEntry {
    pub fn select_voice(&mut self, mut coin: impl FnMut() -> bool) -> Option<usize> {
        let voices = self.sounds.len().saturating_sub(1);
        if voices == 0 {
            return None;
        }
        let mode = self.selection_mode & 0x7f;
        if mode == 1 {
            self.selection_mode = (self.selection_mode & 0xff00) | 2;
            return Some(0);
        }
        let first = usize::from(mode >= 2).min(voices - 1);
        let mut selected = first;
        for _ in first + 1..voices {
            selected += usize::from(coin());
        }
        Some(selected)
    }
}

#[derive(Clone, Debug)]
pub struct SpeechBubble {
    pub points: Vec<(i32, i32)>,
    pub text_x: i32,
    pub text_y: i32,
    pub text: Vec<u8>,
}

impl SpeechBubble {
    fn parse_all(mut rest: &[u8]) -> Result<Vec<Option<Self>>, String> {
        let mut bubbles = Vec::new();
        while !rest.is_empty() {
            let word = |bytes: &[u8], offset: usize| -> Result<u16, String> {
                let pair = bytes
                    .get(offset..offset + 2)
                    .ok_or("Truncated speech bubble")?;
                Ok(u16::from_le_bytes([pair[0], pair[1]]))
            };
            let count = word(rest, 0)? as usize;
            if count == 0 {
                bubbles.push(None);
                rest = &rest[2..];
                continue;
            }
            let header = 2 + count * 4;
            let text_x = word(rest, header + 8)? as i16 as i32;
            let text_y = word(rest, header + 10)? as i16 as i32;
            let length = word(rest, header + 12)? as usize;
            let end = header + 14 + length;
            let text = rest
                .get(header + 14..end)
                .ok_or("Truncated speech bubble text")?
                .to_vec();
            let points = rest[2..header]
                .as_chunks::<4>()
                .0
                .iter()
                .map(|c| (le_i16(c, 0) as i32, le_i16(c, 2) as i32))
                .collect();
            bubbles.push(Some(SpeechBubble {
                points,
                text_x,
                text_y,
                text,
            }));
            rest = &rest[end..];
        }
        Ok(bubbles)
    }
}

/// Named animation sequence range in `c7c4` (`0xc558[seq_idx] + 1 .. 0xc558[seq_idx + 1]`).
#[derive(Clone, Debug)]
pub struct NamedSequence {
    pub name: String,
    pub header_idx: usize,
    pub start_frame: usize,
    pub end_frame: usize,
}

/// Complete puzzle asset loaded from `P01..P15.PZ8` (or `.PZ4`).
#[derive(Clone, Debug)]
pub struct PuzzleAsset {
    pub id: usize,
    pub palette: [[u8; 3]; 256],
    pub records: Vec<SceneRecord>,
    pub sequences: Vec<NamedSequence>,
    pub hotspots: Vec<Hotspot>,
    pub full_anim_seq_idx: usize,
    pub sounds: Vec<SoundBubbleEntry>,
    pub sprites: Vec<Sprite8061>,
}

impl PuzzleAsset {
    pub fn uses_ega(&self) -> bool {
        self.sprites.first().is_some_and(|sprite| sprite.bpp == 4)
    }

    /// Static completed picture frame index (`c7c4[3]`, first frame of `FILM`).
    pub fn static_frame_idx(&self) -> usize {
        self.sequences.get(1).map_or(3, |s| s.start_frame)
    }

    /// Original preset piece decomposition frame index.
    pub fn preset_frame_idx(&self) -> usize {
        self.sequences.get(1).map_or(4, |s| s.start_frame + 1)
    }

    /// Number of pieces in the original preset decomposition.
    pub fn preset_piece_count(&self) -> usize {
        match self.records.get(self.preset_frame_idx()) {
            Some(SceneRecord::Frame { commands, .. }) => commands
                .iter()
                .filter(|c| matches!(c, FrameCommand::SubFrame { .. }))
                .count(),
            _ => 0,
        }
    }

    pub fn load(dir: &Path, puzzle_num: usize, use_ega: bool) -> Result<Self, String> {
        let ext = if use_ega { "PZ4" } else { "PZ8" };
        let filename = format!("P{:02}.{}", puzzle_num, ext);
        let path = dir.join(&filename);
        let raw = match fs::read(&path) {
            Err(e) if use_ega && e.kind() == std::io::ErrorKind::NotFound => {
                return Self::load(dir, puzzle_num, false);
            }
            result => result.map_err(|e| format!("Failed to read {:?}: {}", path, e))?,
        };
        match decompress_da(&raw).and_then(|dec| Self::from_decompressed(puzzle_num, &dec)) {
            Err(error) if use_ega => {
                eprintln!("Cannot load {filename}: {error}; using original VGA asset");
                Self::load(dir, puzzle_num, false)
            }
            result => result,
        }
    }

    fn parse_top_chunks(buf: &[u8]) -> Result<([[u8; 3]; 256], &[u8]), String> {
        let mut pos = 0usize;
        let mut palette = [[0u8; 3]; 256];
        let mut main_chunk: Option<&[u8]> = None;

        while pos + 8 <= buf.len() {
            let tag = &buf[pos..pos + 4];
            if tag != b"PALE"
                && tag != b"MUSI"
                && tag != b"MCG0"
                && tag != b"TND0"
                && tag != b"EGA0"
                && tag != b"CGA0"
            {
                break;
            }
            let (tag, chunk) = read_tagged_chunk(buf, &mut pos, "Truncated puzzle chunk")?;

            if tag == b"PALE" {
                if chunk.len() < 48 {
                    return Err("Truncated puzzle palette".to_string());
                }
                copy_palette(
                    &mut palette,
                    chunk,
                    if chunk.len() >= 768 { 256 } else { 16 },
                );
            } else if tag == b"MCG0" || tag == b"TND0" || tag == b"EGA0" || tag == b"CGA0" {
                main_chunk = Some(chunk);
                break;
            }
        }

        let chunk = main_chunk.ok_or_else(|| "Missing MCG0/TND0 chunk in puzzle".to_string())?;
        Ok((palette, chunk))
    }

    fn next_sub_command<'a>(
        rec: &'a [u8],
        sp: &mut usize,
        kind: &str,
    ) -> Result<(u16, &'a [u8]), String> {
        if *sp + 4 > rec.len() {
            return Err(format!("Truncated {kind} command table"));
        }
        let ctype = le_u16(rec, *sp);
        let csz = le_u16(rec, *sp + 2) as usize;
        let end = sp
            .checked_add(csz)
            .filter(|&end| csz >= 4 && end <= rec.len())
            .ok_or_else(|| format!("Invalid {kind} command size"))?;
        let sub = &rec[*sp..end];
        *sp = end;
        Ok((ctype, sub))
    }

    fn parse_sequence_header(
        rec: &[u8],
        full_anim_seq_idx: &mut usize,
        all_hotspots: &mut Vec<Hotspot>,
    ) -> Result<(String, SceneRecord), String> {
        let sub_cnt = le_u16(rec, 6) as usize;
        let name = String::from_utf8_lossy(&rec[8..16]).trim().to_string();
        let mut hotspots = Vec::new();
        let mut default_playback = None;
        let mut sp = 16usize;
        for _ in 0..sub_cnt {
            let (ctype, sub) = Self::next_sub_command(rec, &mut sp, "sequence")?;
            if (ctype == 0x0010 && sub.len() < 12) || (ctype == 0x0011 && sub.len() < 22) {
                return Err(format!("Truncated sequence command {ctype:#06x}"));
            }
            if ctype == 0x0010 {
                let sidx = le_u16(sub, 4) as usize;
                default_playback = Some(SequencePlayback {
                    seq_idx: sidx,
                    repetitions: le_u16(sub, 6),
                    frame_delay_ticks: le_u16(sub, 8),
                });
                *full_anim_seq_idx = sidx;
            } else if ctype == 0x0011 {
                let sidx = le_u16(sub, 4) as usize;
                let hs = Hotspot {
                    playback: SequencePlayback {
                        seq_idx: sidx,
                        repetitions: le_u16(sub, 6),
                        frame_delay_ticks: le_u16(sub, 8),
                    },
                    x: le_i16(sub, 14),
                    y: le_i16(sub, 16),
                    width: le_i16(sub, 18),
                    height: le_i16(sub, 20),
                };
                hotspots.push(hs.clone());
                all_hotspots.push(hs);
            }
        }
        if sp != rec.len() {
            return Err("Invalid sequence command count".to_string());
        }
        Ok((
            name.clone(),
            SceneRecord::SequenceHeader {
                name,
                default_playback,
                hotspots,
            },
        ))
    }

    fn parse_frame_record(rec: &[u8]) -> Result<SceneRecord, String> {
        let bg_frame_idx = le_u16(rec, 6) as usize;
        let bg_color = le_u16(rec, 8);
        let sub_cnt = le_u16(rec, 10) as usize;
        let mut commands = Vec::with_capacity(sub_cnt);
        let mut sp = 12usize;
        for _ in 0..sub_cnt {
            let (ctype, sub) = Self::next_sub_command(rec, &mut sp, "frame")?;
            let need = |min: usize| {
                (sub.len() >= min)
                    .then_some(sub)
                    .ok_or_else(|| format!("Truncated frame command {ctype:#06x}"))
            };
            match ctype {
                0x0020 => {
                    let sub = need(6)?;
                    let raw_idx = le_u16(sub, 4);
                    let snd_idx =
                        (raw_idx != 0 && raw_idx != 0x7FFF).then(|| (raw_idx - 1) as usize);
                    commands.push(FrameCommand::Sound { snd_idx });
                }
                0x0030 => {
                    let raw = le_u16(need(6)?, 4);
                    commands.push(FrameCommand::Delay {
                        raw,
                        ticks: (raw >> 6) & 0x03FF,
                    });
                }
                0x0040 => {
                    let sub = need(12)?;
                    commands.push(FrameCommand::SubFrame {
                        dx: le_i16(sub, 4),
                        dy: le_i16(sub, 6),
                        frame_idx: le_u16(sub, 8) as usize,
                        flags: le_u16(sub, 10),
                    });
                }
                0x0050 => {
                    let sub = need(16)?;
                    commands.push(FrameCommand::Rect {
                        x: le_i16(sub, 4),
                        y: le_i16(sub, 6),
                        color: le_u16(sub, 8),
                        flags: le_u16(sub, 10),
                        width: le_i16(sub, 12),
                        height: le_i16(sub, 14),
                    });
                }
                0x0060 => {
                    let sub = need(12)?;
                    commands.push(FrameCommand::Sprite {
                        x: le_i16(sub, 4),
                        y: le_i16(sub, 6),
                        sprite_idx: (le_u16(sub, 8) as usize).saturating_sub(1),
                        flags: le_u16(sub, 10),
                    });
                }
                0x0080 => {
                    let sub = need(14)?;
                    commands.push(FrameCommand::Text {
                        x: le_i16(sub, 4),
                        y: le_i16(sub, 6),
                        res_idx: (le_u16(sub, 8) as usize).saturating_sub(1),
                        flags: le_u16(sub, 10),
                        font_and_color: le_u16(sub, 12),
                    });
                }
                0x0090 => {
                    let sub = need(28)?;
                    let mut params = [0i16; 8];
                    for (pi, param) in params.iter_mut().enumerate() {
                        *param = le_i16(sub, 12 + pi * 2);
                    }
                    commands.push(FrameCommand::Polygon {
                        x: le_i16(sub, 4),
                        y: le_i16(sub, 6),
                        res_idx: (le_u16(sub, 8) as usize).saturating_sub(1),
                        flags: le_u16(sub, 10),
                        params,
                    });
                }
                _ => {}
            }
        }
        if sp != rec.len() {
            return Err("Invalid frame command count".to_string());
        }
        Ok(SceneRecord::Frame {
            bg_frame_idx,
            bg_color,
            commands,
        })
    }

    fn parse_sound_bubble_record(rec: &[u8]) -> Result<SoundBubbleEntry, String> {
        let tag = String::from_utf8_lossy(&rec[6..14]).trim().to_string();
        let num_snds = le_u16(rec, 24) as usize;
        if 28 + num_snds * 14 > rec.len() {
            return Err("Truncated scene sound descriptors".to_string());
        }
        let mut snd_list = Vec::with_capacity(num_snds);
        let mut sp = 28usize;
        for _ in 0..num_snds {
            let stem = String::from_utf8_lossy(&rec[sp..sp + 8]).trim().to_string();
            let phrase_idx = le_u16(rec, sp + 8);
            snd_list.push(SoundDescriptor { stem, phrase_idx });
            sp += 14;
        }
        let bubbles = SpeechBubble::parse_all(&rec[sp..])?;
        if bubbles.len() != num_snds.saturating_sub(1) {
            return Err("Invalid speech bubble count".to_string());
        }
        Ok(SoundBubbleEntry {
            tag,
            sounds: snd_list,
            selection_mode: le_u16(rec, 26),
            bubbles,
        })
    }

    pub fn from_decompressed(puzzle_num: usize, buf: &[u8]) -> Result<Self, String> {
        let (palette, chunk) = Self::parse_top_chunks(buf)?;
        let mut rpos = 0usize;
        let mut records = Vec::new();
        let mut seq_headers: Vec<(String, usize)> = Vec::new();
        let mut all_hotspots = Vec::new();
        let mut full_anim_seq_idx = 2usize;
        let mut sounds = Vec::new();
        let mut sprites = Vec::new();

        while rpos + 4 <= chunk.len() {
            let rtype = le_u16(chunk, rpos);
            if (rtype & 0x8000) == 0 {
                let rsz = le_u16(chunk, rpos + 2) as usize;
                if rsz < 4 || rpos + rsz > chunk.len() {
                    return Err(format!("Invalid scene record size at {rpos:#x}"));
                }
                let rec = &chunk[rpos..rpos + rsz];
                if (rtype == 0x7FF0 && rec.len() < 16) || (rtype == 0x7FF1 && rec.len() < 12) {
                    return Err(format!("Truncated scene record at {rpos:#x}"));
                }
                if rtype == 0x7FF0 {
                    let (name, record) = Self::parse_sequence_header(
                        rec,
                        &mut full_anim_seq_idx,
                        &mut all_hotspots,
                    )?;
                    seq_headers.push((name, records.len()));
                    records.push(record);
                } else if rtype == 0x7FF1 {
                    records.push(Self::parse_frame_record(rec)?);
                }
                rpos += rsz;
            } else {
                if rpos + 6 > chunk.len() {
                    return Err("Truncated scene resource header".to_string());
                }
                let rsz = le_u32(chunk, rpos + 2) as usize;
                let end = rpos
                    .checked_add(rsz)
                    .filter(|&end| rsz >= 6 && end <= chunk.len())
                    .ok_or_else(|| format!("Invalid scene resource size at {rpos:#x}"))?;
                let rec = &chunk[rpos..end];
                if rtype == 0x8023 {
                    if rec.len() < 28 {
                        return Err("Truncated scene sound header".to_string());
                    }
                    sounds.push(Self::parse_sound_bubble_record(rec)?);
                } else if rtype == 0x8061 {
                    sprites.push(Sprite8061::from_record(rec)?);
                }
                rpos = end;
            }
        }
        if rpos != chunk.len() {
            return Err("Truncated scene record header".to_string());
        }

        let mut sequences = Vec::with_capacity(seq_headers.len());
        let mut hdr_iter = seq_headers.into_iter().peekable();
        while let Some((name, hdr_idx)) = hdr_iter.next() {
            let end_frame = hdr_iter.peek().map_or(records.len(), |&(_, idx)| idx);
            sequences.push(NamedSequence {
                name,
                header_idx: hdr_idx,
                start_frame: hdr_idx + 1,
                end_frame,
            });
        }

        Ok(Self {
            id: puzzle_num,
            palette,
            records,
            sequences,
            hotspots: all_hotspots,
            full_anim_seq_idx,
            sounds,
            sprites,
        })
    }
}

/// Monochrome 320x200 printable line-art image loaded from `P01..P15.PRT`.
#[derive(Clone, Debug)]
pub struct PrintAsset {
    pub width: u16,
    pub height: u16,
    /// 8,000 bytes (320x200 1-bit monochrome, 40 bytes per row, MSB = leftmost pixel).
    pub bitmap: Vec<u8>,
}

impl PrintAsset {
    pub fn load(dir: &Path, puzzle_num: usize) -> Result<Self, String> {
        let dec = read_da(&dir.join(format!("P{:02}.PRT", puzzle_num)))?;
        if dec.len() < 8000 {
            return Err(format!("PRT decompressed size too small: {}", dec.len()));
        }
        Ok(Self {
            width: 320,
            height: 200,
            bitmap: dec[..8000].to_vec(),
        })
    }
}

/// Load Title (`MJP.TTL`) or Credits (`MJP.CRD`) screens for the selected video mode.
pub fn load_ilbm_container(path: &Path) -> Result<Vec<IlbmImage>, String> {
    let dec = read_da(path)?;
    let chunks = split_multi_resource(&dec)?;
    let mut images = Vec::new();
    for (idx, ch) in chunks.into_iter().enumerate() {
        match decode_ilbm(ch) {
            Ok(img) => images.push(img),
            Err(err) => eprintln!("{} resource {idx}: {err}", path.display()),
        }
    }
    if images.is_empty() {
        return Err(format!("No decodable images in {}", path.display()));
    }
    Ok(images)
}

/// 28-byte (14 `u16`) Preferences structure matching `MJP.PRE` (`seg_004f`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Preferences {
    pub video_mode: u16, // 0=MCGA/VGA 256, 1=VGA, 2=EGA, 3=Tandy, 4=CGA RGB, 5=CGA Composite
    pub control_mode: u16, // 0=Keyboard, 1=Mouse, 2=Joystick
    pub music_on: u16,   // 0=Off, 1=On
    pub sfx_on: u16,     // 0=Off, 1=On
    pub sound_category: u16, // 0=Standard, 1=Sound Source / Sound Blaster / AdLib / PS/1 / Tandy
    pub sound_device: u16, // 0=None, 1=PC Speaker, 2=Disney SS LPT1, 3=Disney SS LPT2, 4=PS/1, 5=Tandy, 6=AdLib/SB
    pub sb_port: u16,      // 0=210h, 1=220h, 2=230h, 3=240h, 4=250h, 5=260h
    pub sb_irq: u16,       // 0=IRQ2, 1=IRQ3, 2=IRQ5, 3=IRQ7
    pub timer_on: u16,     // 0=Off, 1=On
    pub grid_lines_on: u16, // 0=Off, 1=On
    pub text_msg_on: u16,  // 0=Off, 1=On
    pub printer_model: u16, // 0..28
    pub printer_class: i16, // -1..6
    pub printer_port: u16, // 0=LPT1, 1=LPT2, 2=COM1, 3=COM2
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            video_mode: 1,   // VGA 256 Color
            control_mode: 1, // Mouse (plus Keyboard always active)
            music_on: 1,
            sfx_on: 1,
            sound_category: 1,
            sound_device: 6, // Sound Blaster / AdLib + Disney Sound Source
            sb_port: 1,      // 220h
            sb_irq: 3,       // IRQ 7
            timer_on: 1,
            grid_lines_on: 0,
            text_msg_on: 1,
            printer_model: 8, // Epson FX-80
            printer_class: 1,
            printer_port: 0, // LPT1
        }
    }
}

impl Preferences {
    pub fn load(path: &Path) -> Result<Self, String> {
        let b = read_file(path)?;
        if b.len() < 28 {
            return Err("MJP.PRE shorter than 28 bytes".to_string());
        }
        let u = |i: usize| le_u16(&b, i * 2);
        Ok(Self {
            video_mode: u(0),
            control_mode: u(1),
            music_on: u(2),
            sfx_on: u(3),
            sound_category: u(4),
            sound_device: u(5),
            sb_port: u(6),
            sb_irq: u(7),
            timer_on: u(8),
            grid_lines_on: u(9),
            text_msg_on: u(10),
            printer_model: u(11),
            printer_class: u(12) as i16,
            printer_port: u(13),
        })
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let words = [
            self.video_mode,
            self.control_mode,
            self.music_on,
            self.sfx_on,
            self.sound_category,
            self.sound_device,
            self.sb_port,
            self.sb_irq,
            self.timer_on,
            self.grid_lines_on,
            self.text_msg_on,
            self.printer_model,
            self.printer_class as u16,
            self.printer_port,
        ];
        let mut bytes = [0u8; 28];
        for (dst, w) in bytes.as_chunks_mut::<2>().0.iter_mut().zip(words) {
            *dst = w.to_le_bytes();
        }
        fs::write(path, bytes).map_err(|e| format!("Failed to write {:?}: {}", path, e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(
        not(feature = "asset-tests"),
        ignore = "requires original DOS game assets"
    )]
    #[test]
    fn vga_sprite_pixels_match_original_driver() {
        check_sprite_pixels(include_str!("../validation/sprite-pixels.tsv"), false);
    }

    #[cfg_attr(
        not(feature = "asset-tests"),
        ignore = "requires original DOS game assets"
    )]
    #[test]
    fn packed_sprite_pixels_match_original_driver() {
        check_sprite_pixels(include_str!("../validation/sprite-pixels-tandy.tsv"), true);
    }

    #[cfg_attr(
        not(feature = "asset-tests"),
        ignore = "requires original DOS game assets"
    )]
    #[test]
    fn ega_sprite_pixels_match_original_conversion_and_driver() {
        let mut loaded = None;
        let mut checked = 0;
        let mut pixels = 0;
        for row in include_str!("../validation/ega-sprite-pixels.tsv")
            .lines()
            .skip(1)
        {
            let columns: Vec<_> = row.split('\t').collect();
            let id: usize = columns[0][1..3].parse().unwrap();
            if loaded.as_ref().is_none_or(|(previous, _)| *previous != id) {
                loaded = Some((
                    id,
                    PuzzleAsset::load(Path::new(env!("CARGO_MANIFEST_DIR")), id, true).unwrap(),
                ));
            }
            let sprite = &loaded.as_ref().unwrap().1.sprites[columns[1].parse::<usize>().unwrap()];
            assert_eq!(sprite.stride, columns[2].parse::<u16>().unwrap(), "{row}");
            assert_eq!(sprite.height, columns[3].parse::<u16>().unwrap(), "{row}");
            let mut checksum = 0xcbf29ce484222325u64;
            for (position, &color) in sprite.pixels.iter().enumerate() {
                let result = if sprite.is_opaque(position) {
                    color as u16
                } else {
                    u16::MAX
                };
                for byte in result.to_le_bytes() {
                    checksum = (checksum ^ byte as u64).wrapping_mul(0x100000001b3);
                }
            }
            assert_eq!(
                checksum,
                u64::from_str_radix(columns[4], 16).unwrap(),
                "{row}"
            );
            checked += 1;
            pixels += sprite.pixels.len();
        }
        assert_eq!(checked, 722);
        assert_eq!(pixels, 749756);
    }

    fn check_sprite_pixels(fixture: &str, use_ega: bool) {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut loaded = None;
        let mut checked = 0;
        for row in fixture.lines().skip(1) {
            let columns: Vec<_> = row.split('\t').collect();
            let id: usize = columns[0][1..3].parse().unwrap();
            if loaded.as_ref().is_none_or(|(previous, _)| *previous != id) {
                loaded = Some((id, PuzzleAsset::load(root, id, use_ega).unwrap()));
            }
            let sprite = &loaded.as_ref().unwrap().1.sprites[columns[1].parse::<usize>().unwrap()];
            let mut seen = [false; 256];
            let mut count = 0;
            let mut checksum = 0xcbf29ce484222325u64;
            for (position, &color) in sprite.pixels.iter().enumerate() {
                if std::mem::replace(&mut seen[color as usize], true) {
                    continue;
                }
                count += 1;
                let result = if sprite.is_opaque(position) {
                    color as u16
                } else {
                    u16::MAX
                };
                for byte in (position as u32)
                    .to_le_bytes()
                    .into_iter()
                    .chain(result.to_le_bytes())
                {
                    checksum = (checksum ^ byte as u64).wrapping_mul(0x100000001b3);
                }
            }
            assert_eq!(count, columns[2].parse::<usize>().unwrap(), "{row}");
            assert_eq!(
                checksum,
                u64::from_str_radix(columns[3], 16).unwrap(),
                "{row}"
            );
            checked += 1;
        }
        assert_eq!(checked, if use_ega { 722 } else { 885 });
    }

    fn toolbox_fixture() -> Vec<u8> {
        let mut sprite = vec![0; 27];
        sprite[..2].copy_from_slice(&0x8061u16.to_le_bytes());
        sprite[2..6].copy_from_slice(&27u32.to_le_bytes());
        for offset in [10, 12, 22, 24] {
            sprite[offset] = 1;
        }
        sprite[16] = 4;
        sprite[26] = 0x70;
        let mut data = b"PALE".to_vec();
        data.extend_from_slice(&48u32.to_le_bytes());
        data.extend_from_slice(&[0; 48]);
        data[8..11].copy_from_slice(&[2, 3, 4]);
        data.extend_from_slice(b"TBOX");
        data.extend_from_slice(&27u32.to_le_bytes());
        data.extend_from_slice(&sprite);
        data
    }

    #[test]
    fn toolbox_complete_palette_and_unknown_chunks_remain_valid() {
        let mut data = toolbox_fixture();
        data.extend_from_slice(b"JUNK\0\0\0\0");
        let toolbox = ToolboxAsset::parse_chunk(&data).unwrap();
        assert_eq!(toolbox.palette[0], [2, 3, 4]);
        assert_eq!(toolbox.sprites.len(), 1);
        assert_eq!(toolbox.sprites[0].pixels, [7]);
    }

    #[test]
    fn toolbox_rejects_partial_chunks_records_and_palettes() {
        let original = toolbox_fixture();
        let mut oversized = original.clone();
        oversized[60..64].copy_from_slice(&28u32.to_le_bytes());
        assert!(ToolboxAsset::parse_chunk(&oversized).is_err());
        for size in [0u32, 5, 28, u32::MAX] {
            let mut damaged = original.clone();
            damaged[66..70].copy_from_slice(&size.to_le_bytes());
            assert!(
                ToolboxAsset::parse_chunk(&damaged).is_err(),
                "record size={size}"
            );
        }
        for tail in 1..8 {
            let mut damaged = original.clone();
            damaged.extend_from_slice(&vec![0; tail]);
            assert!(
                ToolboxAsset::parse_chunk(&damaged).is_err(),
                "chunk tail={tail}"
            );
        }
        for tail in 1..6 {
            let mut damaged = original.clone();
            damaged.extend_from_slice(&vec![0; tail]);
            damaged[60..64].copy_from_slice(&(27u32 + tail as u32).to_le_bytes());
            assert!(
                ToolboxAsset::parse_chunk(&damaged).is_err(),
                "record tail={tail}"
            );
        }
        for size in 0..48u32 {
            let mut damaged = b"PALE".to_vec();
            damaged.extend_from_slice(&size.to_le_bytes());
            damaged.extend_from_slice(&vec![0; size as usize]);
            damaged.extend_from_slice(&original[56..]);
            assert!(
                ToolboxAsset::parse_chunk(&damaged).is_err(),
                "palette size={size}"
            );
        }
        assert!(ToolboxAsset::parse_chunk(&[]).is_err());
    }

    fn scene_command_fixture(sequence: bool, command: u16, size: usize) -> Vec<u8> {
        let prefix = if sequence { 16 } else { 12 };
        let mut record = vec![0; prefix + size];
        record[..2].copy_from_slice(&if sequence { 0x7ff0u16 } else { 0x7ff1u16 }.to_le_bytes());
        record[2..4].copy_from_slice(&((prefix + size) as u16).to_le_bytes());
        record[if sequence { 6 } else { 10 }] = 1;
        record[prefix..prefix + 2].copy_from_slice(&command.to_le_bytes());
        record[prefix + 2..prefix + 4].copy_from_slice(&(size as u16).to_le_bytes());
        let mut container = b"MCG0".to_vec();
        container.extend_from_slice(&(record.len() as u32).to_le_bytes());
        container.extend_from_slice(&record);
        container
    }

    #[test]
    fn recognized_scene_commands_require_their_payload() {
        for (sequence, command, minimum) in [
            (true, 0x10, 12),
            (true, 0x11, 22),
            (false, 0x20, 6),
            (false, 0x30, 6),
            (false, 0x40, 12),
            (false, 0x50, 16),
            (false, 0x60, 12),
            (false, 0x80, 14),
            (false, 0x90, 28),
        ] {
            for size in 4..minimum {
                let data = scene_command_fixture(sequence, command, size);
                assert!(
                    PuzzleAsset::from_decompressed(1, &data).is_err(),
                    "{command:#x} size={size}"
                );
            }
            assert!(
                PuzzleAsset::from_decompressed(
                    1,
                    &scene_command_fixture(sequence, command, minimum)
                )
                .is_ok()
            );
        }
        let unknown =
            PuzzleAsset::from_decompressed(1, &scene_command_fixture(false, 0xa0, 6)).unwrap();
        assert!(
            matches!(&unknown.records[0], SceneRecord::Frame { commands, .. } if commands.is_empty())
        );
    }

    #[test]
    fn sequence_commands_preserve_playback_settings() {
        for (command, size) in [(0x10, 12), (0x11, 22)] {
            let mut data = scene_command_fixture(true, command, size);
            for (offset, value) in [(28, 3u16), (30, 6), (32, 11)] {
                data[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
            }
            let asset = PuzzleAsset::from_decompressed(1, &data).unwrap();
            let SceneRecord::SequenceHeader {
                default_playback,
                hotspots,
                ..
            } = &asset.records[0]
            else {
                panic!("Missing sequence")
            };
            if command == 0x10 {
                assert_eq!(
                    *default_playback,
                    Some(SequencePlayback {
                        seq_idx: 3,
                        repetitions: 6,
                        frame_delay_ticks: 11,
                    })
                );
            } else {
                assert_eq!(hotspots[0].playback.seq_idx, 3);
                assert_eq!(
                    (
                        hotspots[0].playback.repetitions,
                        hotspots[0].playback.frame_delay_ticks
                    ),
                    (6, 11)
                );
            }
        }
    }

    #[test]
    fn puzzle_chunks_reject_truncated_declared_sizes_and_palettes() {
        let mut data = scene_command_fixture(false, 0xa0, 6);
        let size = u32::from_le_bytes(data[4..8].try_into().unwrap());
        data[4..8].copy_from_slice(&(size + 1).to_le_bytes());
        assert!(PuzzleAsset::from_decompressed(1, &data).is_err());
        for size in 0..=48u32 {
            let mut palette = b"PALE".to_vec();
            palette.extend_from_slice(&size.to_le_bytes());
            palette.extend_from_slice(&vec![7; size as usize]);
            palette.extend_from_slice(&scene_command_fixture(false, 0xa0, 6));
            let parsed = PuzzleAsset::from_decompressed(1, &palette);
            if size < 48 {
                assert!(parsed.is_err(), "palette size={size}");
            } else {
                assert_eq!(parsed.unwrap().palette[0], [7; 3]);
            }
        }
    }

    #[cfg_attr(
        not(feature = "asset-tests"),
        ignore = "requires original DOS game assets"
    )]
    #[test]
    fn font_rejects_truncated_rows_and_invalid_bounds() {
        let original = fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("MJP.NFT")).unwrap();
        for end in 0..1205 {
            assert!(FontAsset::from_bytes(&original[..end]).is_err(), "{end}");
        }
        for (offset, value) in [
            (405, 0),
            (405, 79),
            (405, 81),
            (406, 255),
            (16, 0),
            (18, 0),
            (21, 255),
            (213, 255),
        ] {
            let mut damaged = original.to_vec();
            damaged[offset] = value;
            assert!(FontAsset::from_bytes(&damaged).is_err(), "{offset}:{value}");
        }
    }

    #[cfg_attr(
        not(feature = "asset-tests"),
        ignore = "requires original DOS game assets"
    )]
    #[test]
    fn supplied_font_keeps_all_literal_pixels() {
        let original = fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("MJP.NFT")).unwrap();
        let font = FontAsset::from_bytes(&original).unwrap();
        assert_eq!((font.strip_width, font.height), (616, 10));
        for row in 0..10 {
            let offset = 405 + row * 80;
            assert_eq!(
                &font.bitmap[row * 77..row * 77 + 63],
                &original[offset + 2..offset + 65]
            );
            assert_eq!(
                &font.bitmap[row * 77 + 63..(row + 1) * 77],
                &original[offset + 66..offset + 80]
            );
        }
    }

    #[cfg_attr(
        not(feature = "asset-tests"),
        ignore = "requires original DOS game assets"
    )]
    #[test]
    fn supplied_dialogue_preserves_polygons_and_multiple_bubbles() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let p01 = PuzzleAsset::load(dir, 1, false).unwrap();
        let oh_boy = p01.sounds.iter().find(|s| s.tag == "OHBOY-01").unwrap();
        assert_eq!(oh_boy.bubbles.len(), 1);
        let bubble = oh_boy.bubbles[0].as_ref().unwrap();
        assert_eq!(bubble.text, b"Oh, boy!");
        assert_eq!((bubble.text_x, bubble.text_y), (19, 80));
        assert_eq!(bubble.points.len(), 20);
        assert_eq!(bubble.points[0], (33, 125));
        assert_eq!(bubble.points.last(), bubble.points.first());
        let p02 = PuzzleAsset::load(dir, 2, false).unwrap();
        let mickey = p02.sounds.iter().find(|s| s.tag == "MICK-02").unwrap();
        assert_eq!(mickey.bubbles.len(), 2);
        assert_eq!(
            mickey.bubbles[0].as_ref().unwrap().text,
            b"Did you see that\rshooting star?"
        );
        assert_eq!(
            mickey.bubbles[1].as_ref().unwrap().text,
            b"Those spacemen\rare silly."
        );
    }

    #[cfg_attr(
        not(feature = "asset-tests"),
        ignore = "requires original DOS game assets"
    )]
    #[test]
    fn truncated_dialogue_is_rejected() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let asset = PuzzleAsset::load(dir, 1, false).unwrap();
        let bubble = asset
            .sounds
            .iter()
            .find(|s| s.tag == "OHBOY-01")
            .unwrap()
            .bubbles[0]
            .as_ref()
            .unwrap();
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&(bubble.points.len() as u16).to_le_bytes());
        for &(x, y) in &bubble.points {
            bytes.extend_from_slice(&(x as i16).to_le_bytes());
            bytes.extend_from_slice(&(y as i16).to_le_bytes());
        }
        for value in [24u16, 24, 248, 248, 19, 80, 8] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(b"Oh, boy!");
        for length in 1..bytes.len() {
            assert!(
                SpeechBubble::parse_all(&bytes[..length]).is_err(),
                "{length}"
            );
        }
        assert!(SpeechBubble::parse_all(&bytes).is_ok());
        assert_eq!(SpeechBubble::parse_all(&[0, 0]).unwrap().len(), 1);
        assert!(SpeechBubble::parse_all(&[0, 0]).unwrap()[0].is_none());
    }

    #[cfg_attr(
        not(feature = "asset-tests"),
        ignore = "requires original DOS game assets"
    )]
    #[test]
    fn alternative_dialogue_selects_one_voice_and_keeps_intro_once() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut asset = PuzzleAsset::load(dir, 11, false).unwrap();
        let entry = asset
            .sounds
            .iter_mut()
            .find(|entry| entry.tag == "THINK-11")
            .unwrap();
        assert_eq!(entry.selection_mode, 1);
        assert_eq!(
            entry.select_voice(|| panic!("Intro uses no random choice")),
            Some(0)
        );
        assert_eq!(entry.select_voice(|| false), Some(1));
        assert_eq!(entry.select_voice(|| true), Some(1));
        let mut asset = PuzzleAsset::load(dir, 2, false).unwrap();
        let entry = asset
            .sounds
            .iter_mut()
            .find(|entry| entry.tag == "MICK-02")
            .unwrap();
        assert_eq!(entry.select_voice(|| false), Some(0));
        assert_eq!(entry.select_voice(|| true), Some(1));
    }

    #[cfg_attr(
        not(feature = "asset-tests"),
        ignore = "requires original DOS game assets"
    )]
    #[test]
    fn supplied_scene_resources_resolve() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut checked = 0;
        let mut bubble_count = 0;
        for ega in [false, true] {
            for id in 1..=15 {
                let path = dir.join(format!("P{id:02}.{}", if ega { "PZ4" } else { "PZ8" }));
                if !path.exists() {
                    continue;
                }
                let decoded = decompress_da(&fs::read(&path).unwrap()).unwrap();
                let parsed = PuzzleAsset::from_decompressed(id, &decoded);
                if ega && id == 6 {
                    assert!(parsed.unwrap_err().contains("0x3efc"));
                    continue;
                }
                let asset = parsed.unwrap_or_else(|error| panic!("{}: {error}", path.display()));
                for sound in &asset.sounds {
                    assert_eq!(sound.bubbles.len(), sound.sounds.len().saturating_sub(1));
                    bubble_count += sound.bubbles.iter().flatten().count();
                }
                for record in &asset.records {
                    if let SceneRecord::Frame {
                        bg_frame_idx,
                        commands,
                        ..
                    } = record
                    {
                        if *bg_frame_idx != 0 {
                            assert!(matches!(
                                asset.records.get(*bg_frame_idx),
                                Some(SceneRecord::Frame { .. })
                            ));
                        }
                        for command in commands {
                            match *command {
                                FrameCommand::SubFrame { frame_idx, .. } => assert!(matches!(
                                    asset.records.get(frame_idx),
                                    Some(SceneRecord::Frame { .. })
                                )),
                                FrameCommand::Sprite { sprite_idx, .. } => {
                                    assert!(sprite_idx < asset.sprites.len());
                                }
                                FrameCommand::Sound {
                                    snd_idx: Some(index),
                                } => {
                                    assert!(index < asset.sounds.len());
                                }
                                _ => {}
                            }
                        }
                    }
                }
                assert!(
                    asset
                        .hotspots
                        .iter()
                        .all(|h| h.playback.seq_idx < asset.sequences.len())
                );
                checked += 1;
            }
        }
        assert_eq!(checked, 27);
        assert_eq!(bubble_count, 93);
    }

    #[cfg_attr(
        not(feature = "asset-tests"),
        ignore = "requires original DOS game assets"
    )]
    #[test]
    fn malformed_ega_scene_loads_original_vga() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let selected = PuzzleAsset::load(dir, 6, true).unwrap();
        let vga = PuzzleAsset::load(dir, 6, false).unwrap();
        assert_eq!(selected.palette, vga.palette);
        assert_eq!(selected.records.len(), vga.records.len());
        assert_eq!(selected.sounds.len(), 2);
        assert_eq!(selected.sprites.len(), 53);
        for (selected, original) in selected.sprites.iter().zip(&vga.sprites) {
            assert_eq!(selected.pixels, original.pixels);
        }
    }

    #[test]
    fn read_tagged_chunk_rejects_truncated_headers_and_oversized_lengths() {
        for len in 0..8 {
            let short = vec![0u8; len];
            let mut pos = 0usize;
            assert!(read_tagged_chunk(&short, &mut pos, "truncated").is_err());
            assert_eq!(pos, 0);
        }

        let mut past_end_pos = 4usize;
        let buf = b"PALE\x00\x00\x00\x00";
        assert!(read_tagged_chunk(buf, &mut past_end_pos, "truncated").is_err());
        assert_eq!(past_end_pos, 4);

        let mut oversized = b"PALE".to_vec();
        oversized.extend_from_slice(&u32::MAX.to_le_bytes());
        let mut pos = 0usize;
        assert!(read_tagged_chunk(&oversized, &mut pos, "oversized").is_err());
        assert_eq!(pos, 0);

        let mut valid = b"PALE".to_vec();
        valid.extend_from_slice(&3u32.to_le_bytes());
        valid.extend_from_slice(&[1, 2, 3]);
        let mut pos = 0usize;
        let (tag, chunk) = read_tagged_chunk(&valid, &mut pos, "valid").unwrap();
        assert_eq!(tag, b"PALE");
        assert_eq!(chunk, &[1, 2, 3]);
        assert_eq!(pos, valid.len());
    }

    #[test]
    fn packed_4bpp_and_2bpp_odd_stride_sprites_unpack_correctly() {
        let make_header = |stride: u16, height: u16, bpp: u8| {
            let mut rec = vec![0u8; 26];
            rec[0..2].copy_from_slice(&0x8061u16.to_le_bytes());
            rec[10..12].copy_from_slice(&stride.to_le_bytes());
            rec[12..14].copy_from_slice(&height.to_le_bytes());
            rec[16] = bpp;
            rec[22..24].copy_from_slice(&stride.to_le_bytes());
            rec[24..26].copy_from_slice(&height.to_le_bytes());
            rec
        };

        let mut rec4 = make_header(3, 2, 4);
        rec4.extend_from_slice(&[0x12, 0x30, 0xAB, 0xC0]);
        let spr4 = Sprite8061::from_record(&rec4).unwrap();
        assert_eq!(spr4.pixels, vec![1, 2, 3, 0xA, 0xB, 0xC]);
        rec4.pop();
        assert!(Sprite8061::from_record(&rec4).is_err());

        let mut rec2 = make_header(3, 2, 2);
        rec2.extend_from_slice(&[0b01_10_11_00, 0b11_01_00_00]);
        let spr2 = Sprite8061::from_record(&rec2).unwrap();
        assert_eq!(spr2.pixels, vec![1, 2, 3, 3, 1, 0]);
        rec2.pop();
        assert!(Sprite8061::from_record(&rec2).is_err());
    }

    #[test]
    fn scene_resource_rejects_u32_max_size_without_overflow() {
        let mut container = b"MCG0".to_vec();
        container.extend_from_slice(&6u32.to_le_bytes());
        container.extend_from_slice(&0x8061u16.to_le_bytes());
        container.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(PuzzleAsset::from_decompressed(1, &container).is_err());
    }

    #[test]
    fn font_rejects_extreme_dimensions_without_eager_allocation() {
        let mut data = vec![0u8; 405];
        data[0..4].copy_from_slice(b"FONT");
        data[16..18].copy_from_slice(&u16::MAX.to_le_bytes());
        data[18..20].copy_from_slice(&u16::MAX.to_le_bytes());
        assert!(FontAsset::from_bytes(&data).is_err());
    }

    #[test]
    fn preferences_roundtrip_and_rejects_short_file() {
        struct TempDirGuard(std::path::PathBuf);
        impl Drop for TempDirGuard {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }

        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("mjp_test_prefs_{}_{unique}", std::process::id()));
        fs::create_dir(&dir).unwrap();
        let _guard = TempDirGuard(dir.clone());
        let tmp = dir.join("MJP.PRE");

        let prefs = Preferences {
            video_mode: 2,
            control_mode: 0,
            printer_class: -1,
            ..Preferences::default()
        };
        prefs.save(&tmp).unwrap();
        assert_eq!(Preferences::load(&tmp).unwrap(), prefs);
        fs::write(&tmp, [0u8; 27]).unwrap();
        assert!(Preferences::load(&tmp).is_err());
    }
}
