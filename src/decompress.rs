//! Decompression and container parsers for Mickey's Jigsaw Puzzles (1991, DOS):
//! - `DA\0\0` Shannon-Fano / Canonical Prefix Code + LZ77 decompressor (`seg_165f`)
//! - Multi-resource container parser (`seg_1747`)
//! - Electronic Arts IFF `ILBM` planar image decoder (`seg_0701`, `VGA.BIN` `1000:172f`)

use std::borrow::Cow;

/// Read RLE-encoded symbol bit-lengths from a `DA\0\0` header.
fn read_tree_lengths(src: &[u8], pos: &mut usize, n_expected: usize) -> Result<Vec<u8>, String> {
    if *pos >= src.len() {
        return Err("Unexpected EOF reading tree length count".to_string());
    }
    let count = (src[*pos] as usize) + 1;
    *pos += 1;
    let mut lengths = Vec::with_capacity(n_expected);
    for _ in 0..count {
        if *pos >= src.len() {
            return Err("Unexpected EOF reading tree lengths".to_string());
        }
        let b = src[*pos];
        *pos += 1;
        let run_len = (((b >> 4) & 0x0F) as usize) + 1;
        let bit_len = (b & 0x0F) + 1;
        lengths.extend(std::iter::repeat_n(bit_len, run_len));
    }
    if lengths.len() != n_expected {
        return Err(format!(
            "Expected {} tree lengths, got {}",
            n_expected,
            lengths.len()
        ));
    }
    if lengths.iter().map(|&len| 1u32 << (16 - len)).sum::<u32>() > 65536 {
        return Err("Oversubscribed prefix tree".to_string());
    }
    Ok(lengths)
}

struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    buf: u32,
    cnt: u32,
}

impl<'a> BitReader<'a> {
    #[inline]
    fn new(data: &'a [u8], pos: usize) -> Self {
        Self {
            data,
            pos,
            buf: 0,
            cnt: 0,
        }
    }

    #[inline]
    fn ensure(&mut self, needed: u32) {
        while self.cnt < needed {
            let b = self.data.get(self.pos).copied().unwrap_or(0) as u32;
            self.pos += 1;
            self.buf |= b << self.cnt;
            self.cnt += 8;
        }
    }

    #[inline]
    fn read_bits(&mut self, n: u32) -> u32 {
        self.ensure(n);
        let v = self.buf & ((1u32 << n) - 1);
        self.buf >>= n;
        self.cnt -= n;
        v
    }

    #[inline]
    fn consume(&mut self, n: u8) {
        self.buf >>= n;
        self.cnt -= n as u32;
    }

    #[inline]
    fn consumed_bytes_exceed(&self, len: usize) -> bool {
        self.pos * 8 - self.cnt as usize > len * 8
    }
}

struct PrefixDecoder {
    /// Direct 8-bit lookup table: `fast[byte] = (symbol, bit_len)` if `bit_len <= 8`, else `(0, 0)`.
    fast: [(u16, u8); 256],
    /// Fallback list of `(bit_len, mask, code, symbol)` for codes with `bit_len > 8`.
    slow: Vec<(u8, u16, u16, u16)>,
}

impl PrefixDecoder {
    fn build(lengths: &[u8]) -> Self {
        let n = lengths.len();
        let mut bl_count = [0usize; 17];
        for &l in lengths {
            if (l as usize) < 17 {
                bl_count[l as usize] += 1;
            }
        }
        let mut cum = [0usize; 17];
        let mut total = 0usize;
        for i in 1..=16 {
            total += bl_count[i];
            cum[i] = total;
        }
        let mut order = vec![0u8; n];
        for sym in (0..n).rev() {
            let l = lengths[sym] as usize;
            cum[l] -= 1;
            order[cum[l]] = lengths[sym];
        }
        let mut codes_sorted = vec![0u16; n];
        let mut code = 0u16;
        for i in (0..n).rev() {
            codes_sorted[i] = code;
            let shift = 16u32.saturating_sub(order[i] as u32);
            let step = if shift < 16 { 1u16 << shift } else { 0 };
            code = code.wrapping_add(step);
        }
        let mut sym_codes = vec![0u16; n];
        for sym in 0..n {
            let l = lengths[sym] as usize;
            let idx = cum[l];
            cum[l] += 1;
            sym_codes[sym] = codes_sorted[idx].reverse_bits();
        }

        let mut fast = [(0u16, 0u8); 256];
        let mut slow = Vec::new();
        for sym in 0..n {
            let l = lengths[sym];
            let c = sym_codes[sym];
            if l <= 8 {
                let step = 1usize << l;
                let mut idx = c as usize;
                while idx < 256 {
                    fast[idx] = (sym as u16, l);
                    idx += step;
                }
            } else {
                let mask = if l == 16 { 0xFFFF } else { (1u16 << l) - 1 };
                slow.push((l, mask, c, sym as u16));
            }
        }
        slow.sort_by_key(|&(l, _, _, _)| l);
        Self { fast, slow }
    }

    #[inline]
    fn decode(&self, reader: &mut BitReader<'_>) -> Result<u16, String> {
        reader.ensure(16);
        let low8 = (reader.buf & 0xFF) as usize;
        let (sym, len) = self.fast[low8];
        if len != 0 {
            reader.consume(len);
            return Ok(sym);
        }
        let low16 = (reader.buf & 0xFFFF) as u16;
        for &(l, mask, code, sym) in &self.slow {
            if (low16 & mask) == code {
                reader.consume(l);
                return Ok(sym);
            }
        }
        Err("Invalid DA prefix code".to_string())
    }
}

/// Decompresses a `DA\0\0` buffer (`seg_165f`).
/// If the buffer does not begin with `DA\0\0`, returns a clone of `data`.
pub fn decompress_da(data: &[u8]) -> Result<Vec<u8>, String> {
    if !data.starts_with(b"DA\x00\x00") {
        return Ok(data.to_vec());
    }
    if data.len() < 20 {
        return Err("DA\\0\\0 header too short".to_string());
    }
    let flags = u16::from_le_bytes([data[8], data[9]]);
    let method = u16::from_le_bytes([data[10], data[11]]);
    let uncomp_size = u32::from_le_bytes([data[16], data[17], data[18], data[19]]) as usize;
    let packed_size = u32::from_le_bytes(data[12..16].try_into().unwrap()) as usize;
    if packed_size > data.len() - 20 || uncomp_size > 64 * 1024 * 1024 {
        return Err("Invalid DA payload size".to_string());
    }

    if method == 0 {
        return data
            .get(20..20 + uncomp_size)
            .map(<[u8]>::to_vec)
            .ok_or_else(|| "Truncated raw DA payload".to_string());
    }
    if method != 1 {
        return Err(format!("Unsupported DA method {method}"));
    }

    let mut pos = 20usize;
    let has_lit_tree = (flags & 4) != 0;
    let is_8k = (flags & 2) != 0;
    let dist_low_bits: u32 = if is_8k { 7 } else { 6 };
    let dist_low_mask: u32 = (1 << dist_low_bits) - 1;
    let min_match: usize = if is_8k { 3 } else { 2 };

    let lit_decoder = if has_lit_tree {
        let lit_lens = read_tree_lengths(data, &mut pos, 256)?;
        Some(PrefixDecoder::build(&lit_lens))
    } else {
        None
    };
    let len_lens = read_tree_lengths(data, &mut pos, 64)?;
    let len_decoder = PrefixDecoder::build(&len_lens);

    let dist_lens = read_tree_lengths(data, &mut pos, 64)?;
    let dist_decoder = PrefixDecoder::build(&dist_lens);

    let mut reader = BitReader::new(data, pos);
    let mut out = Vec::with_capacity(uncomp_size);
    while out.len() < uncomp_size {
        if reader.read_bits(1) == 1 {
            let b = if let Some(ref dec) = lit_decoder {
                dec.decode(&mut reader)? as u8
            } else {
                reader.read_bits(8) as u8
            };
            out.push(b);
        } else {
            let dist_low = reader.read_bits(dist_low_bits) & dist_low_mask;
            let dist_high = dist_decoder.decode(&mut reader)? as u32;
            let dist = ((dist_high << dist_low_bits) | dist_low) as usize;
            let len_sym = len_decoder.decode(&mut reader)? as usize;
            let mut match_len = len_sym + min_match;
            if len_sym == 63 {
                match_len += reader.read_bits(8) as usize;
            }
            let copy_len = match_len.min(uncomp_size - out.len());
            let cur_len = out.len();
            if dist < cur_len {
                let src_start = cur_len - dist - 1;
                if dist == 0 {
                    let byte = out[src_start];
                    out.resize(cur_len + copy_len, byte);
                } else if dist + 1 >= copy_len {
                    out.extend_from_within(src_start..src_start + copy_len);
                } else {
                    for i in 0..copy_len {
                        let byte = out[src_start + i];
                        out.push(byte);
                    }
                }
            } else {
                for i in 0..copy_len {
                    let src_idx = (cur_len + i) as isize - (dist as isize) - 1;
                    let byte = if src_idx < 0 {
                        0
                    } else {
                        out[src_idx as usize]
                    };
                    out.push(byte);
                }
            }
        }
    }

    if reader.consumed_bytes_exceed(data.len()) {
        return Err("Truncated DA bitstream".to_string());
    }
    Ok(out)
}

/// Splits a decompressed multi-resource container (`MJP.TTL`, `MJP.CRD`, `MJP.TBX`, `.ESS`, `.ISS`).
/// Format (`seg_1747`): `u16 count`, `[u32; count]` sizes, followed by resource payloads.
pub fn split_multi_resource(buf: &[u8]) -> Result<Vec<&[u8]>, String> {
    if buf.len() < 2 {
        return Err("Multi-resource buffer too small".to_string());
    }
    let count = u16::from_le_bytes([buf[0], buf[1]]) as usize;
    let table_end = 2 + count * 4;
    if buf.len() < table_end {
        return Err("Multi-resource table truncated".to_string());
    }
    let mut sizes = Vec::with_capacity(count);
    for i in 0..count {
        let off = 2 + i * 4;
        let sz = u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]]) as usize;
        sizes.push(sz);
    }
    let mut pos = table_end;
    let mut chunks = Vec::with_capacity(count);
    for &sz in &sizes {
        let end = pos
            .checked_add(sz)
            .filter(|&end| end <= buf.len())
            .ok_or("Multi-resource payload truncated")?;
        chunks.push(&buf[pos..end]);
        pos = end;
    }
    Ok(chunks)
}

/// Decoded Electronic Arts IFF `ILBM` full-screen image (`MJP.TTL`, `MJP.CRD`).
#[derive(Clone, Debug)]
pub struct IlbmImage {
    pub width: u16,
    pub height: u16,
    pub n_planes: u8,
    pub palette: [[u8; 3]; 256],
    pub pixels: Vec<u8>,
}

/// Decodes an IFF `FORM` -> `ILBM` planar image (`seg_0701`, `VGA.BIN` `1000:172f`).
pub fn decode_ilbm(buf: &[u8]) -> Result<IlbmImage, String> {
    let data = buf;
    if data.len() < 12 || &data[..4] != b"FORM" || &data[8..12] != b"ILBM" {
        return Err("Invalid ILBM header".to_string());
    }
    let form_len = u32::from_be_bytes(data[4..8].try_into().unwrap()) as usize;
    let data = data
        .get(..form_len.checked_add(8).ok_or("Invalid FORM size")?)
        .ok_or("Truncated FORM")?;

    let mut pos = 12usize;
    let mut width = 320u16;
    let mut height = 200u16;
    let mut n_planes = 8u8;
    let mut compression = 0u8;
    let mut palette = [[0u8; 3]; 256];
    let mut body: Option<&[u8]> = None;

    while pos + 8 <= data.len() {
        let tag = &data[pos..pos + 4];
        let raw_sz =
            u32::from_be_bytes([data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]]);
        let csz = raw_sz as usize;
        pos += 8;
        let end = pos.checked_add(csz).ok_or("Invalid ILBM chunk size")?;
        let chunk = data.get(pos..end).ok_or("Truncated ILBM chunk")?;

        match tag {
            b"BMHD" if chunk.len() >= 20 => {
                width = u16::from_be_bytes([chunk[0], chunk[1]]);
                height = u16::from_be_bytes([chunk[2], chunk[3]]);
                n_planes = chunk[8];
                compression = chunk[10];
                if chunk[9] != 0 {
                    return Err("Unsupported ILBM mask".to_string());
                }
            }
            b"CMAP" => {
                let n_colors = (chunk.len() / 3).min(256);
                for i in 0..n_colors {
                    palette[i] = [chunk[i * 3], chunk[i * 3 + 1], chunk[i * 3 + 2]];
                }
            }
            b"BODY" => {
                body = Some(chunk);
                break;
            }
            _ => {}
        }
        pos = end + (csz & 1);
    }

    let body_bytes = body.ok_or_else(|| "Missing BODY chunk in ILBM".to_string())?;
    let w = width as usize;
    let h = height as usize;
    let planes = (n_planes as usize).min(8);
    if w == 0
        || h == 0
        || w * h > 16 * 1024 * 1024
        || !(1..=8).contains(&n_planes)
        || compression > 1
    {
        return Err("Unsupported ILBM dimensions or compression".to_string());
    }
    let row_plane_bytes = w.div_ceil(16) * 2;

    // Decompress ByteRun1 if compression == 1, otherwise use raw bitplanes
    let raw_planes: Cow<'_, [u8]> = if compression == 1 {
        let expected = h * planes * row_plane_bytes;
        let mut dec = Vec::with_capacity(expected);
        let mut bp = 0usize;
        while bp < body_bytes.len() && dec.len() < expected {
            let cmd = body_bytes[bp] as i8;
            bp += 1;
            if cmd >= 0 {
                let count = (cmd as usize) + 1;
                let end = bp + count;
                dec.extend_from_slice(body_bytes.get(bp..end).ok_or("Truncated ByteRun literal")?);
                bp = end;
            } else if cmd != -128 {
                let count = ((-cmd) as usize) + 1;
                let val = *body_bytes.get(bp).ok_or("Truncated ByteRun repeat")?;
                bp += 1;
                dec.extend(std::iter::repeat_n(val, count));
            }
        }
        Cow::Owned(dec)
    } else {
        Cow::Borrowed(body_bytes)
    };
    if raw_planes.len() != h * planes * row_plane_bytes {
        return Err("Invalid ILBM BODY size".to_string());
    }

    // Planar-to-chunky conversion (`VGA.BIN` `1000:172f`)
    let mut pixels = vec![0u8; w * h];
    for y in 0..h {
        let row_base = y * planes * row_plane_bytes;
        let dst_row = &mut pixels[y * w..(y + 1) * w];
        for (p, plane_bit) in (0..planes).map(|p| (p, 1u8 << p)) {
            let plane = &raw_planes[row_base + p * row_plane_bytes..][..row_plane_bytes];
            for (x, dst) in dst_row.iter_mut().enumerate() {
                if (plane[x >> 3] & (0x80u8 >> (x & 7))) != 0 {
                    *dst |= plane_bit;
                }
            }
        }
    }

    Ok(IlbmImage {
        width,
        height,
        n_planes,
        palette,
        pixels,
    })
}

#[cfg(test)]
mod tests {
    use super::decompress_da;

    #[cfg_attr(
        not(feature = "asset-tests"),
        ignore = "requires original DOS game assets"
    )]
    #[test]
    fn scene_streams_match_original_da_decoder() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut checked = 0;
        for row in include_str!("../validation/scene-decode.tsv")
            .lines()
            .skip(1)
        {
            let columns: Vec<_> = row.split('\t').collect();
            let packed = std::fs::read(root.join(columns[0])).unwrap();
            let decoded = decompress_da(&packed).unwrap();
            assert_eq!(decoded.len(), columns[1].parse::<usize>().unwrap(), "{row}");
            let checksum = decoded.iter().fold(0xcbf29ce484222325u64, |hash, &byte| {
                (hash ^ byte as u64).wrapping_mul(0x100000001b3)
            });
            assert_eq!(
                checksum,
                u64::from_str_radix(columns[2], 16).unwrap(),
                "{row}"
            );
            checked += 1;
        }
        assert_eq!(checked, 27);
    }

    fn sparse_da(literal_tree: bool, payload: &[u8], size: u32) -> Vec<u8> {
        let mut data = vec![0; 20];
        data[..4].copy_from_slice(b"DA\0\0");
        data[8..10].copy_from_slice(&(if literal_tree { 4u16 } else { 0 }).to_le_bytes());
        data[10..12].copy_from_slice(&1u16.to_le_bytes());
        data[16..20].copy_from_slice(&size.to_le_bytes());
        if literal_tree {
            data.push(15);
            data.extend([0xf8; 16]);
        }
        for _ in 0..2 {
            data.extend([3, 0xf8, 0xf8, 0xf8, 0xf8]);
        }
        data.extend(payload);
        let packed = (data.len() - 20) as u32;
        let total = (data.len() - 8) as u32;
        data[4..8].copy_from_slice(&total.to_le_bytes());
        data[12..16].copy_from_slice(&packed.to_le_bytes());
        data
    }

    #[test]
    fn invalid_da_prefix_codes_do_not_create_zero_bytes() {
        assert_eq!(
            decompress_da(&sparse_da(false, &[0x83, 0], 1)).unwrap(),
            b"A"
        );
        assert_eq!(
            decompress_da(&sparse_da(true, &[0xfd, 3, 0], 1)).unwrap(),
            [0]
        );
        for (name, literal_tree, payload, size) in [
            ("literal", true, &[0xff, 3, 0][..], 1),
            ("distance", false, &[0xfe, 0xff, 0xff, 0xff][..], 2),
            ("length", false, &[0, 0, 0xff, 1, 0][..], 2),
        ] {
            let decoded = decompress_da(&sparse_da(literal_tree, payload, size));
            assert_eq!(decoded.unwrap_err(), "Invalid DA prefix code", "{name}");
        }
    }
}
