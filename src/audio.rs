//! Audio synthesis and playback engine for Mickey's Jigsaw Puzzles (1991, DOS):
//! - `.ESS` multi-resource containers (`PROG.ESS`, `DATA.ESS`)
//! - `.NNV` speech & sound-effect synthesizer (`seg_1363`, `9,321.73 Hz`, block-floating-point PCM + glottal pulse train)
//! - `.ISS` PC-speaker music

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use rodio::{OutputStream, OutputStreamHandle, Sink, Source};

use crate::decompress::split_multi_resource;

/// Base sample rate of the `.NNV` speech/SFX engine (`1,193,182 / 128 = 9,321.73 Hz`).
pub const NNV_SAMPLE_RATE: u32 = 9322;

/// Output sample rate used for synthesized music & mixed audio.
pub const AUDIO_SAMPLE_RATE: u32 = 22050;

#[derive(Clone, Copy, Debug)]
#[repr(usize)]
pub enum CommonSound {
    NextPicture = 3,
    PreviousPicture = 4,
    StartPuzzle = 5,
    ChooseSize = 7,
    ChoosePiece = 8,
    ReturnPiece = 9,
    OtherPicture = 10,
}

impl CommonSound {
    pub fn caption(self) -> &'static str {
        match self {
            Self::NextPicture => "What's the next picture?",
            Self::PreviousPicture => "Let's look at the last picture.",
            Self::StartPuzzle => "Let's make this one.",
            Self::ChooseSize => "Please choose how many pieces\ryou want in our puzzle.",
            Self::ChoosePiece => "Let's choose a piece.",
            Self::ReturnPiece => "Let's put this one back for now.",
            Self::OtherPicture => "Let's make a different\rpuzzle.",
        }
    }
}

const COMMON_FEEDBACK: [(&str, usize); 15] = [
    ("DATA_0", 3),
    ("PROG_0", 0),
    ("PROG_1", 0),
    ("DATA_0", 0),
    ("DATA_0", 1),
    ("DATA_0", 2),
    ("DATA_0", 4),
    ("DATA_1", 0),
    ("DATA_2", 0),
    ("DATA_3", 0),
    ("DATA_4", 0),
    ("DATA_5", 0),
    ("DATA_6", 0),
    ("DATA_7", 0),
    ("DATA_8", 0),
];

fn error_buzz_samples() -> &'static [i16] {
    static SAMPLES: OnceLock<Vec<i16>> = OnceLock::new();
    SAMPLES.get_or_init(|| {
        const FREQUENCY: u32 = 880;
        const CYCLES: u32 = 80;
        (0..AUDIO_SAMPLE_RATE * CYCLES / FREQUENCY)
            .map(|sample| {
                if (sample * FREQUENCY * 2 / AUDIO_SAMPLE_RATE) & 1 == 0 {
                    3200
                } else {
                    -3200
                }
            })
            .collect()
    })
}

/// Exact 3-bit block-floating-point lookup table at `1363:1958` (`16 scales x 8 codes`).
const NNV_TABLE_3BIT: [u8; 128] = [
    128, 125, 126, 127, 128, 129, 130, 131, 128, 124, 125, 127, 128, 129, 131, 132, 128, 123, 125,
    126, 128, 130, 131, 133, 128, 122, 124, 126, 128, 130, 132, 134, 128, 120, 123, 125, 128, 131,
    133, 136, 128, 118, 121, 125, 128, 131, 135, 138, 128, 115, 119, 124, 128, 132, 137, 141, 128,
    111, 117, 122, 128, 134, 139, 145, 128, 106, 113, 121, 128, 135, 143, 150, 128, 100, 109, 119,
    128, 137, 147, 156, 128, 92, 104, 116, 128, 140, 152, 164, 128, 81, 97, 112, 128, 144, 159,
    175, 128, 68, 88, 108, 128, 148, 168, 188, 128, 51, 77, 102, 128, 154, 179, 205, 128, 29, 62,
    95, 128, 161, 194, 227, 128, 1, 43, 86, 128, 170, 213, 255,
];

/// Exact 4-bit block-floating-point lookup table at `1363:19D8` (`16 scales x 16 codes`).
const NNV_TABLE_4BIT: [u8; 256] = [
    128, 121, 122, 123, 124, 125, 126, 127, 128, 129, 130, 131, 132, 133, 134, 135, 128, 120, 121,
    122, 123, 124, 126, 127, 128, 129, 130, 132, 133, 134, 135, 136, 128, 118, 119, 121, 122, 124,
    125, 127, 128, 129, 131, 132, 134, 135, 137, 138, 128, 116, 117, 119, 121, 123, 124, 126, 128,
    130, 132, 133, 135, 137, 139, 140, 128, 113, 115, 117, 119, 122, 124, 126, 128, 130, 132, 134,
    137, 139, 141, 143, 128, 110, 112, 115, 117, 120, 123, 125, 128, 131, 133, 136, 139, 141, 144,
    146, 128, 106, 109, 112, 115, 118, 122, 125, 128, 131, 134, 138, 141, 144, 147, 150, 128, 101,
    105, 109, 113, 116, 120, 124, 128, 132, 136, 140, 143, 147, 151, 155, 128, 95, 100, 105, 109,
    114, 119, 123, 128, 133, 137, 142, 147, 151, 156, 161, 128, 88, 94, 100, 105, 111, 117, 122,
    128, 134, 139, 145, 151, 156, 162, 168, 128, 80, 87, 93, 100, 107, 114, 121, 128, 135, 142,
    149, 156, 163, 169, 176, 128, 69, 78, 86, 94, 103, 111, 120, 128, 136, 145, 153, 162, 170, 178,
    187, 128, 57, 67, 77, 87, 98, 108, 118, 128, 138, 148, 158, 169, 179, 189, 199, 128, 42, 54,
    66, 79, 91, 103, 116, 128, 140, 153, 165, 177, 190, 202, 214, 128, 23, 38, 53, 68, 83, 98, 113,
    128, 143, 158, 173, 188, 203, 218, 233, 128, 1, 19, 37, 55, 74, 92, 110, 128, 146, 164, 182,
    201, 219, 237, 255,
];

/// Exact 5-bit block-floating-point lookup table at `1363:1AD8` (`16 scales x 32 codes`).
const NNV_TABLE_5BIT: [u8; 512] = [
    128, 113, 114, 115, 116, 117, 118, 119, 120, 121, 122, 123, 124, 125, 126, 127, 128, 129, 130,
    131, 132, 133, 134, 135, 136, 137, 138, 139, 140, 141, 142, 143, 128, 111, 112, 113, 114, 115,
    116, 118, 119, 120, 121, 122, 123, 125, 126, 127, 128, 129, 130, 131, 133, 134, 135, 136, 137,
    138, 140, 141, 142, 143, 144, 145, 128, 108, 109, 111, 112, 113, 115, 116, 117, 119, 120, 121,
    123, 124, 125, 127, 128, 129, 131, 132, 133, 135, 136, 137, 139, 140, 141, 143, 144, 145, 147,
    148, 128, 105, 107, 108, 110, 111, 113, 114, 116, 117, 119, 120, 122, 123, 125, 126, 128, 130,
    131, 133, 134, 136, 137, 139, 140, 142, 143, 145, 146, 148, 149, 151, 128, 101, 103, 105, 107,
    109, 110, 112, 114, 116, 117, 119, 121, 123, 124, 126, 128, 130, 132, 133, 135, 137, 139, 140,
    142, 144, 146, 147, 149, 151, 153, 155, 128, 97, 99, 102, 104, 106, 108, 110, 112, 114, 116,
    118, 120, 122, 124, 126, 128, 130, 132, 134, 136, 138, 140, 142, 144, 146, 148, 150, 152, 154,
    157, 159, 128, 93, 95, 97, 100, 102, 104, 107, 109, 112, 114, 116, 119, 121, 123, 126, 128,
    130, 133, 135, 137, 140, 142, 144, 147, 149, 152, 154, 156, 159, 161, 163, 128, 87, 90, 93, 95,
    98, 101, 104, 106, 109, 112, 114, 117, 120, 123, 125, 128, 131, 133, 136, 139, 142, 144, 147,
    150, 152, 155, 158, 161, 163, 166, 169, 128, 81, 84, 87, 91, 94, 97, 100, 103, 106, 109, 112,
    116, 119, 122, 125, 128, 131, 134, 137, 140, 144, 147, 150, 153, 156, 159, 162, 165, 169, 172,
    175, 128, 74, 78, 81, 85, 88, 92, 96, 99, 103, 106, 110, 114, 117, 121, 124, 128, 132, 135,
    139, 142, 146, 150, 153, 157, 160, 164, 168, 171, 175, 178, 182, 128, 66, 70, 74, 78, 82, 86,
    91, 95, 99, 103, 107, 111, 116, 120, 124, 128, 132, 136, 140, 145, 149, 153, 157, 161, 165,
    170, 174, 178, 182, 186, 190, 128, 56, 61, 66, 71, 75, 80, 85, 90, 94, 99, 104, 109, 114, 118,
    123, 128, 133, 138, 142, 147, 152, 157, 162, 166, 171, 176, 181, 185, 190, 195, 200, 128, 45,
    51, 56, 62, 67, 73, 78, 84, 89, 95, 100, 106, 111, 117, 122, 128, 134, 139, 145, 150, 156, 161,
    167, 172, 178, 183, 189, 194, 200, 205, 211, 128, 32, 39, 45, 52, 58, 64, 71, 77, 83, 90, 96,
    103, 109, 115, 122, 128, 134, 141, 147, 153, 160, 166, 173, 179, 185, 192, 198, 204, 211, 217,
    224, 128, 18, 25, 33, 40, 47, 55, 62, 69, 77, 84, 91, 99, 106, 113, 121, 128, 135, 143, 150,
    157, 165, 172, 179, 187, 194, 201, 209, 216, 223, 231, 238, 128, 1, 9, 18, 26, 35, 43, 52, 60,
    69, 77, 86, 94, 103, 111, 120, 128, 136, 145, 153, 162, 170, 179, 187, 196, 204, 213, 221, 230,
    238, 247, 255,
];

/// Original note values.
const ISS_NOTE_VALUES: [u16; 88] = [
    149, 158, 166, 176, 187, 199, 210, 223, 236, 250, 265, 280, 298, 315, 333, 353, 375, 397, 420,
    446, 472, 500, 531, 561, 595, 630, 667, 707, 749, 794, 841, 891, 944, 1000, 1059, 1122, 1189,
    1260, 1335, 1414, 1498, 1587, 1682, 1782, 1888, 2000, 2119, 2245, 2378, 2520, 2670, 2828, 2997,
    3175, 3364, 3564, 3775, 4000, 4238, 4490, 4757, 5040, 5339, 5657, 5993, 6350, 6727, 7127, 7551,
    8000, 8476, 8980, 9514, 10079, 10679, 11314, 11986, 12699, 13454, 14254, 15102, 16000, 16951,
    17959, 19027, 20158, 21358, 22628,
];

fn decode_nnv_frame(
    bank: &[u8],
    start: usize,
    mode: u8,
    limit: usize,
    samples: &mut Vec<u8>,
) -> Result<usize, &'static str> {
    if mode == 0 {
        let mut pos = start;
        loop {
            let byte = *bank.get(pos).ok_or("Truncated raw NNV frame")?;
            pos += 1;
            if byte == 0 {
                return Ok(pos);
            }
            if samples.len() >= limit {
                return Err("NNV phrase exceeds two minutes");
            }
            samples.push(byte);
        }
    }
    let mut start = start;
    loop {
        let first = *bank.get(start).ok_or("Missing NNV frame")?;
        let (width, scale, mut bit) = match mode {
            0x30 => (3, (first & 15) as usize, 4),
            0x40 => (4, (first & 15) as usize, 4),
            0x50 => {
                if first >> 3 == 0 {
                    return Ok(start + 1);
                }
                let second = *bank.get(start + 1).ok_or("Truncated NNV scale")?;
                (5, (((first & 7) << 1) | (second & 1)) as usize, 3)
            }
            _ => return Err("Unsupported NNV sample mode"),
        };
        for _ in 0..if mode == 0x40 { 19 } else { 20 } {
            if mode == 0x50 && bit == 8 {
                bit += 1;
            }
            let mut code = 0usize;
            for i in 0..width {
                let byte = *bank
                    .get(start + (bit + i) / 8)
                    .ok_or("Truncated NNV frame")?;
                code |= (((byte >> ((bit + i) & 7)) & 1) as usize) << i;
            }
            bit += width;
            if code == 0 {
                return Ok(start + bit.div_ceil(8));
            }
            if samples.len() >= limit {
                return Err("NNV phrase exceeds two minutes");
            }
            samples.push(match mode {
                0x30 => NNV_TABLE_3BIT[scale * 8 + code],
                0x40 => NNV_TABLE_4BIT[scale * 16 + code],
                _ => NNV_TABLE_5BIT[scale * 32 + code],
            });
        }
        start += match mode {
            0x30 => 8,
            0x40 => 10,
            _ => 13,
        };
    }
}

fn decode_nnv_phrase(bank: &[u8], phrase: usize) -> Result<Vec<u8>, &'static str> {
    let count = u32::from_le_bytes(
        bank.get(..4)
            .ok_or("Missing NNV header")?
            .try_into()
            .unwrap(),
    ) as usize;
    if phrase >= count || count > bank.len().saturating_sub(4) / 4 {
        return Err("Invalid NNV phrase table");
    }
    let entry = 4 + phrase * 4;
    let mut pc = u32::from_le_bytes(bank[entry..entry + 4].try_into().unwrap()) as usize;
    if pc < 4 + count * 4 {
        return Err("Invalid NNV phrase offset");
    }
    let limit = NNV_SAMPLE_RATE as usize * 120;
    let mut samples = Vec::new();
    let mut pitch_left = 0u8;
    let mut pitch_period = 0u8;
    let mut pitch_delta = 0i8;

    loop {
        let cmd = *bank.get(pc).ok_or("Unterminated NNV phrase")?;
        let opcode = cmd & 7;
        let mode = cmd & 0x70;
        match opcode {
            0 | 7 => pc += 1,
            6 => {
                bank.get(pc..pc + 2).ok_or("Truncated NNV control")?;
                pc += 2;
            }
            1 => {
                let next = *bank.get(pc + 1).ok_or("Truncated NNV silence")?;
                let length = 4096 - (((cmd >> 4) as usize) << 8 | next as usize);
                if samples.len() + length > limit {
                    return Err("NNV phrase exceeds two minutes");
                }
                samples.resize(samples.len() + length, 128);
                pc += 2;
            }
            2..=5 => {
                let length = if opcode == 2 { 4 } else { 5 };
                let header = bank.get(pc..pc + length).ok_or("Truncated NNV packet")?;
                let mut frame = header[1] as usize
                    | (header[2] as usize) << 8
                    | ((header[3] & 7) as usize) << 16;
                pc += length;
                if opcode == 2 {
                    decode_nnv_frame(bank, frame, mode, limit, &mut samples)?;
                } else {
                    let packets = 32 - (header[3] >> 3) as usize;
                    let packed = header[4];
                    let base_repeats = 8 - ((packed >> 2) & 7) as usize;
                    let fraction = ((packed & 3) as usize) << 6;
                    let max_step = (packed >> 5) as usize;
                    let step = match mode {
                        0 => 20,
                        0x30 => 8,
                        0x40 => 10,
                        0x50 => 13,
                        _ => return Err("Unsupported NNV sample mode"),
                    };
                    let mut remainder = 0;
                    for _ in 0..packets {
                        remainder += fraction;
                        let repeats = base_repeats + (remainder >> 8);
                        remainder &= 255;
                        let mut next_frame = frame;
                        for repeat in 0..repeats {
                            let offset = if opcode == 3 {
                                repeat % (max_step + 1) * step
                            } else {
                                0
                            };
                            next_frame =
                                decode_nnv_frame(bank, frame + offset, mode, limit, &mut samples)?;
                            if opcode == 5 {
                                if pitch_left == 0 {
                                    let contour = bank
                                        .get(pc..pc + 2)
                                        .ok_or("Truncated NNV pitch contour")?;
                                    pc += 2;
                                    let word = u16::from_le_bytes(contour.try_into().unwrap());
                                    pitch_delta = (word as i8).wrapping_shl(3) >> 3;
                                    pitch_period = ((word >> 5) as u8).wrapping_mul(2);
                                    pitch_left = 16 - (contour[1] >> 4);
                                } else {
                                    pitch_period = pitch_period.wrapping_add(pitch_delta as u8);
                                }
                                let silence = (pitch_period >> 1) as usize;
                                pitch_left -= 1;
                                if samples.len() + silence > limit {
                                    return Err("NNV phrase exceeds two minutes");
                                }
                                samples.resize(samples.len() + silence, 128);
                            }
                        }
                        frame = next_frame;
                    }
                }
            }
            _ => unreachable!(),
        }
        if cmd & 8 != 0 {
            return Ok(samples);
        }
    }
}

/// Signed PCM at 9,322 Hz.
pub fn synthesize_nnv_phrase(bank: &[u8], phrase_idx: usize) -> Vec<i16> {
    decode_nnv_phrase(bank, phrase_idx)
        .unwrap_or_default()
        .into_iter()
        .map(|sample| (i16::from(sample) - 128) * 192)
        .collect()
}

#[derive(Clone, Copy)]
struct IssVoice {
    pitch: u16,
    period: u8,
    step: i8,
    delay: u8,
    remaining: u8,
    active: bool,
}

fn iss_speaker_ticks(song: &[u8]) -> Result<Vec<(bool, u32)>, &'static str> {
    let word = |pos: usize| -> Result<u16, &'static str> {
        Ok(u16::from_le_bytes(
            song.get(pos..pos + 2)
                .ok_or("Truncated ISS word")?
                .try_into()
                .unwrap(),
        ))
    };
    let start = word(0)? as usize;
    let mut loops = *song.get(2).ok_or("Truncated ISS header")?;
    if start < 3 || start >= song.len() {
        return Err("Invalid ISS event offset");
    }
    let mut pc = start;
    let mut voices = [IssVoice {
        pitch: 2000,
        period: 0,
        step: 0,
        delay: 0,
        remaining: 0,
        active: false,
    }; 3];
    let mut selected = 2;
    let mut order = [2; 3];
    let mut pending = 0;
    let mut row_started = false;
    let mut tempo = 0u16;
    let mut clock = 0u16;
    let mut divisor = 65536;
    let mut ticks = Vec::new();

    for _ in 0..16080 {
        clock = clock.wrapping_add(1);
        if (clock as i16) >= (tempo as i16) {
            loop {
                match *song.get(pc).ok_or("Unterminated ISS song")? {
                    0xfe => {
                        if loops == 0 {
                            ticks.push((false, 65536));
                            return Ok(ticks);
                        }
                        loops -= 1;
                        pc = start;
                    }
                    0xfd => {
                        tempo = word(pc + 1)?;
                        clock = tempo;
                        pc += 3;
                    }
                    _ => break,
                }
            }
            let row = song.get(pc..pc + 8).ok_or("Truncated ISS event")?;
            let boundary = !row_started || clock.wrapping_sub(tempo) & 7 == 0;
            if !row_started {
                pending = 0;
                for voice in (0..3).rev() {
                    let note = row[voice * 2];
                    if note == 0xf0 {
                        if voice == selected {
                            order[pending] = voice;
                            pending += 1;
                            break;
                        }
                    } else if note != 0x58 {
                        order[pending] = voice;
                        pending += 1;
                    }
                }
                if pending == 0 {
                    order[0] = 2;
                    pending = 1;
                }
                row_started = true;
            }
            if boundary {
                if pending == 0 {
                    clock = clock.wrapping_sub(tempo);
                    row_started = false;
                    pc += 8;
                } else {
                    pending -= 1;
                    selected = order[pending];
                    let note = row[selected * 2];
                    let voice = &mut voices[selected];
                    match note {
                        0x58 => voice.active = false,
                        0xf0 => {}
                        _ => {
                            voice.pitch = *ISS_NOTE_VALUES
                                .get(note as usize)
                                .ok_or("Invalid ISS note")?;
                            let instrument = row[selected * 2 + 1] as usize;
                            let entry = 3 + instrument * 2;
                            if entry + 2 > start {
                                return Err("Invalid ISS instrument index");
                            }
                            let pos = word(entry)? as usize + 2;
                            let vibrato = word(pos)?;
                            voice.period = vibrato as u8;
                            voice.step = (vibrato >> 8) as i8;
                            voice.delay = word(pos + 2)? as u8;
                            voice.remaining = 0;
                            voice.active = true;
                        }
                    }
                }
            }
        }
        let voice = &mut voices[selected];
        if clock & 1 == 0 {
            if voice.delay != 0 {
                voice.delay -= 1;
            } else {
                if voice.remaining == 0 {
                    voice.step = voice.step.wrapping_neg();
                    voice.remaining = voice.period;
                }
                voice.remaining = voice.remaining.wrapping_sub(1);
                voice.pitch = voice.pitch.wrapping_add_signed(i16::from(voice.step));
            }
        }
        let gate = voice.active && voice.pitch > 0x51;
        if gate {
            divisor = 0x510000 / u32::from(voice.pitch);
        }
        ticks.push((gate, divisor));
    }
    Err("ISS song exceeds one minute")
}

pub fn synthesize_iss_song(song: &[u8]) -> Vec<i16> {
    let Ok(ticks) = iss_speaker_ticks(song) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut phase = 0.0f64;
    for (index, (gate, divisor)) in ticks.into_iter().enumerate() {
        let end = (index as u64 + 1) * 4452 * u64::from(AUDIO_SAMPLE_RATE) / 1_193_182;
        let step = 1_193_182.0 / (f64::from(divisor) * f64::from(AUDIO_SAMPLE_RATE));
        while out.len() < end as usize {
            out.push(if gate {
                if phase < 0.5 { 3200 } else { -3200 }
            } else {
                0
            });
            phase = (phase + step).fract();
        }
    }
    out
}

/// Zero-copy mono PCM source backed by a shared or static slice.
struct SampleSlice<T> {
    samples: T,
    pos: usize,
    sample_rate: u32,
}

impl<T: AsRef<[i16]>> SampleSlice<T> {
    fn new(sample_rate: u32, samples: T) -> Self {
        Self {
            samples,
            pos: 0,
            sample_rate,
        }
    }
}

impl<T: AsRef<[i16]>> Iterator for SampleSlice<T> {
    type Item = i16;

    #[inline]
    fn next(&mut self) -> Option<i16> {
        let slice = self.samples.as_ref();
        if self.pos < slice.len() {
            let sample = slice[self.pos];
            self.pos += 1;
            Some(sample)
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let rem = self.samples.as_ref().len().saturating_sub(self.pos);
        (rem, Some(rem))
    }
}

impl<T: AsRef<[i16]>> Source for SampleSlice<T> {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> u16 {
        1
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn total_duration(&self) -> Option<Duration> {
        let len = self.samples.as_ref().len() as u64;
        Some(Duration::from_nanos(
            len * 1_000_000_000 / u64::from(self.sample_rate),
        ))
    }
}

/// Complete audio manager holding decoded `.ESS`, `.NNV`, and `.ISS` banks and an optional `rodio` output stream.
pub struct AudioEngine {
    pub nnv_banks: HashMap<String, Vec<u8>>,
    pub iss_songs: HashMap<(String, usize), Vec<u8>>,
    nnv_cache: HashMap<(String, usize), Arc<[i16]>>,
    iss_cache: HashMap<(String, usize), Arc<[i16]>>,
    _stream: Option<OutputStream>,
    stream_handle: Option<OutputStreamHandle>,
    sfx_sinks: Vec<Sink>,
    common_caption: Option<(CommonSound, f32)>,
    common_caption_rev: Option<u64>,
    next_caption_rev: u64,
    music_sink: Option<Sink>,
    pub music_enabled: bool,
    pub sfx_enabled: bool,
}

impl AudioEngine {
    pub fn new(game_dir: &Path, music_enabled: bool, sfx_enabled: bool) -> Self {
        let mut nnv_banks = HashMap::new();
        let mut iss_songs = HashMap::new();

        if let Ok(entries) = fs::read_dir(game_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("NNV")
                    && let (Some(stem), Ok(bytes)) =
                        (path.file_stem().and_then(|s| s.to_str()), fs::read(&path))
                {
                    nnv_banks.insert(stem.to_string(), bytes);
                }
            }
        }

        // Load standalone BUBGUM.NNV if not already discovered above
        if !nnv_banks.contains_key("BUBGUM")
            && let Ok(bub) = fs::read(game_dir.join("BUBGUM.NNV"))
        {
            nnv_banks.insert("BUBGUM".to_string(), bub);
        }

        // PROG.ESS
        if let Ok(prog_ess) = fs::read(game_dir.join("PROG.ESS"))
            && let Ok(chunks) = split_multi_resource(&prog_ess)
        {
            for (i, ch) in chunks.iter().enumerate() {
                nnv_banks.insert(format!("PROG_{}", i), ch.to_vec());
            }
        }

        // DATA.ESS
        if let Ok(data_ess) = fs::read(game_dir.join("DATA.ESS"))
            && let Ok(chunks) = split_multi_resource(&data_ess)
        {
            for (i, ch) in chunks.iter().enumerate() {
                nnv_banks.insert(format!("DATA_{}", i), ch.to_vec());
            }
        }

        // Load TITLE.ISS and SOUND0..3.ISS
        for stem in ["TITLE", "SOUND0", "SOUND1", "SOUND2", "SOUND3"] {
            let p = game_dir.join(format!("{}.ISS", stem));
            if let Ok(raw) = fs::read(p)
                && let Ok(chunks) = split_multi_resource(&raw)
            {
                for (idx, ch) in chunks.iter().enumerate() {
                    iss_songs.insert((stem.to_string(), idx), ch.to_vec());
                }
            }
        }

        let mut engine = Self {
            nnv_banks,
            iss_songs,
            nnv_cache: HashMap::new(),
            iss_cache: HashMap::new(),
            _stream: None,
            stream_handle: None,
            sfx_sinks: Vec::new(),
            common_caption: None,
            common_caption_rev: None,
            next_caption_rev: 0,
            music_sink: None,
            music_enabled,
            sfx_enabled,
        };
        engine.set_enabled(music_enabled, sfx_enabled);
        engine
    }

    fn decoded_nnv_samples(&mut self, stem: &str, phrase_idx: usize) -> Arc<[i16]> {
        let key = (stem.to_string(), phrase_idx);
        if let Some(cached) = self.nnv_cache.get(&key) {
            return Arc::clone(cached);
        }
        let Some(bank_bytes) = self.nnv_banks.get(stem) else {
            return Arc::from([]);
        };
        let samples: Arc<[i16]> = synthesize_nnv_phrase(bank_bytes, phrase_idx).into();
        self.nnv_cache.insert(key, Arc::clone(&samples));
        samples
    }

    pub fn play_nnv(&mut self, stem: &str, phrase_idx: usize) {
        if !self.sfx_enabled {
            return;
        }
        let samples = self.decoded_nnv_samples(stem, phrase_idx);
        self.play_effect(samples, NNV_SAMPLE_RATE);
    }

    pub fn play_common(&mut self, event: CommonSound) {
        if self.sfx_enabled {
            self.stop_all();
        }
        let (bank, phrase) = COMMON_FEEDBACK[event as usize];
        let samples = self.decoded_nnv_samples(bank, phrase);
        let duration = (samples.len() as f32 / NNV_SAMPLE_RATE as f32)
            .max(event.caption().len() as f32 / 25.0)
            .max(2.0);
        self.common_caption = Some((event, duration));
        self.next_caption_rev = self.next_caption_rev.wrapping_add(1);
        self.common_caption_rev = Some(self.next_caption_rev);
        if self.sfx_enabled {
            self.play_effect(samples, NNV_SAMPLE_RATE);
        }
    }

    pub fn common_caption(&self) -> Option<&'static str> {
        self.common_caption
            .as_ref()
            .map(|(event, _)| event.caption())
    }

    /// Returns a revision token for the currently displayed common caption, or `None` when idle.
    pub fn caption_key(&self) -> Option<u64> {
        self.common_caption_rev
    }

    pub fn update(&mut self, dt: f32) {
        if let Some((_, remaining)) = self.common_caption.as_mut() {
            *remaining -= dt.max(0.0);
            if *remaining <= 0.0 {
                self.common_caption = None;
                self.common_caption_rev = None;
            }
        }
    }

    pub fn play_error(&mut self) {
        if self.sfx_enabled {
            self.play_effect(error_buzz_samples(), AUDIO_SAMPLE_RATE);
        }
    }

    fn play_effect<T: AsRef<[i16]> + Send + 'static>(&mut self, samples: T, rate: u32) {
        if samples.as_ref().is_empty() {
            return;
        }
        if let Some(ref handle) = self.stream_handle
            && let Ok(sink) = Sink::try_new(handle)
        {
            sink.append(SampleSlice::new(rate, samples));
            self.sfx_sinks.retain(|s| !s.empty());
            self.sfx_sinks.push(sink);
        }
    }

    pub fn play_iss(&mut self, stem: &str, song_idx: usize) {
        if !self.music_enabled {
            return;
        }
        let key = (stem.to_string(), song_idx);
        let samples = if let Some(cached) = self.iss_cache.get(&key) {
            Arc::clone(cached)
        } else {
            let Some(song_bytes) = self.iss_songs.get(&key) else {
                eprintln!("Music resource unavailable: {stem}:{song_idx}");
                return;
            };
            let synthesized: Arc<[i16]> = synthesize_iss_song(song_bytes).into();
            self.iss_cache.insert(key, Arc::clone(&synthesized));
            synthesized
        };
        if samples.is_empty() {
            return;
        }
        if let Some(ref sink) = self.music_sink {
            sink.stop();
        }
        if let Some(ref handle) = self.stream_handle
            && let Ok(sink) = Sink::try_new(handle)
        {
            sink.append(SampleSlice::new(AUDIO_SAMPLE_RATE, samples));
            self.music_sink = Some(sink);
        }
    }

    pub fn stop_all(&mut self) {
        self.common_caption = None;
        self.common_caption_rev = None;
        self.sfx_sinks.clear();
        if let Some(m) = self.music_sink.take() {
            m.stop();
        }
    }

    pub fn is_playing(&self) -> bool {
        self.sfx_sinks.iter().any(|sink| !sink.empty())
            || self.music_sink.as_ref().is_some_and(|sink| !sink.empty())
    }

    pub fn set_paused(&self, paused: bool) {
        for sink in self.sfx_sinks.iter().chain(self.music_sink.iter()) {
            if paused {
                sink.pause();
            } else {
                sink.play();
            }
        }
    }

    pub fn set_enabled(&mut self, music: bool, sfx: bool) {
        self.music_enabled = music;
        self.sfx_enabled = sfx;
        if (music || sfx) && self._stream.is_none() {
            match OutputStream::try_default() {
                Ok((stream, handle)) => {
                    self._stream = Some(stream);
                    self.stream_handle = Some(handle);
                }
                Err(err) => eprintln!("Audio unavailable: {err}"),
            }
        }
        if !music && let Some(sink) = self.music_sink.take() {
            sink.stop();
        }
        if !sfx {
            self.sfx_sinks.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rodio::buffer::SamplesBuffer;

    #[cfg_attr(
        not(feature = "asset-tests"),
        ignore = "requires original DOS game assets"
    )]
    #[test]
    fn common_feedback_lifecycle_matches_original_muting_and_replacement() {
        for event in [
            CommonSound::NextPicture,
            CommonSound::PreviousPicture,
            CommonSound::StartPuzzle,
            CommonSound::ChooseSize,
            CommonSound::ChoosePiece,
            CommonSound::ReturnPiece,
            CommonSound::OtherPicture,
        ] {
            for speech in [false, true] {
                let row = include_str!("../validation/common-lifecycle.tsv")
                    .lines()
                    .skip(1)
                    .find(|line| {
                        let fields: Vec<_> = line.split('\t').collect();
                        fields[0].parse::<usize>().unwrap() == event as usize
                            && fields[1] == "1"
                            && fields[2] == if speech { "1" } else { "0" }
                            && fields[3] == "0"
                    })
                    .unwrap();
                let replaces = row.split('\t').nth(4).unwrap().starts_with("stop,");
                let mut audio =
                    AudioEngine::new(Path::new(env!("CARGO_MANIFEST_DIR")), false, false);
                audio.sfx_enabled = speech;
                let (voice, mut voice_output) = Sink::new_idle();
                let (music, mut music_output) = Sink::new_idle();
                voice.append(SamplesBuffer::new(1, NNV_SAMPLE_RATE, vec![1234i16; 100]));
                music.append(SamplesBuffer::new(1, AUDIO_SAMPLE_RATE, vec![1234i16; 100]));
                audio.sfx_sinks.push(voice);
                audio.music_sink = Some(music);
                audio.play_common(event);
                for output in [&mut voice_output, &mut music_output] {
                    let active = output.by_ref().take(100).any(|sample| sample != 0.0);
                    assert_eq!(active, !replaces, "{row}");
                }
                assert_eq!(audio.common_caption(), Some(event.caption()));
                audio.set_paused(true);
                audio.set_paused(false);
                audio.stop_all();
                assert!(audio.common_caption().is_none());
                assert!(audio.sfx_sinks.is_empty());
                assert!(audio._stream.is_none());
            }
        }
    }

    #[cfg_attr(
        not(feature = "asset-tests"),
        ignore = "requires original DOS game assets"
    )]
    #[test]
    fn setup_pause_preserves_both_audio_channels() {
        let mut audio = AudioEngine::new(Path::new(env!("CARGO_MANIFEST_DIR")), false, false);
        let (speech, _speech_queue) = Sink::new_idle();
        let (music, _music_queue) = Sink::new_idle();
        speech.append(SamplesBuffer::new(1, NNV_SAMPLE_RATE, vec![1i16; 100]));
        music.append(SamplesBuffer::new(1, AUDIO_SAMPLE_RATE, vec![1i16; 100]));
        audio.sfx_sinks.push(speech);
        audio.music_sink = Some(music);
        audio.set_paused(true);
        assert!(audio.sfx_sinks[0].is_paused());
        assert!(audio.music_sink.as_ref().unwrap().is_paused());
        assert!(audio.is_playing());
        audio.set_paused(false);
        assert!(!audio.sfx_sinks[0].is_paused());
        assert!(!audio.music_sink.as_ref().unwrap().is_paused());
        audio.set_paused(true);
        audio.stop_all();
        audio.set_enabled(false, false);
        audio.set_paused(false);
        assert!(audio.sfx_sinks.is_empty());
        assert!(audio.music_sink.is_none());
    }

    #[cfg_attr(
        not(feature = "asset-tests"),
        ignore = "requires original DOS game assets"
    )]
    #[test]
    fn common_captions_match_source_and_work_without_audio() {
        let mut engine = AudioEngine::new(Path::new(env!("CARGO_MANIFEST_DIR")), false, false);
        let fixture = include_str!("../validation/common-feedback.tsv");
        for event in [
            CommonSound::NextPicture,
            CommonSound::PreviousPicture,
            CommonSound::StartPuzzle,
            CommonSound::ChooseSize,
            CommonSound::ChoosePiece,
            CommonSound::ReturnPiece,
            CommonSound::OtherPicture,
        ] {
            let row = fixture.lines().nth(event as usize + 1).unwrap();
            let expected = row.split('\t').nth(4).unwrap().replace("\\r", "\r");
            assert_eq!(event.caption(), expected);
            engine.play_common(event);
            assert_eq!(engine.common_caption(), Some(event.caption()));
            engine.update(0.5);
            assert!(engine.common_caption().is_some());
            engine.update(60.0);
            assert!(engine.common_caption().is_none());
        }
        engine.play_common(CommonSound::ChooseSize);
        engine.play_common(CommonSound::ChoosePiece);
        assert_eq!(engine.common_caption(), Some("Let's choose a piece."));
        engine.stop_all();
        assert!(engine.common_caption().is_none());
        assert!(engine._stream.is_none());
    }

    #[cfg_attr(
        not(feature = "asset-tests"),
        ignore = "requires original DOS game assets"
    )]
    #[test]
    fn ess_resources_keep_original_identity_without_guessed_aliases() {
        let engine = AudioEngine::new(Path::new(env!("CARGO_MANIFEST_DIR")), false, false);
        assert!(!engine.nnv_banks.contains_key("SFX-1"));
        assert!(!engine.nnv_banks.contains_key("P01-1"));
        assert_eq!(engine.nnv_banks.len(), 40);
        assert!(engine.nnv_banks.contains_key("SFX-2"));
        assert!(engine.nnv_banks.contains_key("PROG_0"));
        assert!(engine.nnv_banks.contains_key("DATA_8"));
    }

    #[cfg_attr(
        not(feature = "asset-tests"),
        ignore = "requires original DOS game assets"
    )]
    #[test]
    fn common_feedback_matches_original_event_table() {
        let engine = AudioEngine::new(Path::new(env!("CARGO_MANIFEST_DIR")), false, false);
        let mut checked = 0;
        for line in include_str!("../validation/common-feedback.tsv")
            .lines()
            .skip(1)
        {
            let columns: Vec<_> = line.split('\t').collect();
            let event: usize = columns[0].parse().unwrap();
            let expected = format!("{}_{}", columns[1].trim_end_matches(".ESS"), columns[2]);
            let phrase: usize = columns[3].parse().unwrap();
            assert_eq!(COMMON_FEEDBACK[event], (expected.as_str(), phrase));
            assert!(!synthesize_nnv_phrase(&engine.nnv_banks[&expected], phrase).is_empty());
            checked += 1;
        }
        assert_eq!(checked, 15);
        for (event, caption) in [
            (CommonSound::NextPicture, "What's the next picture?"),
            (
                CommonSound::PreviousPicture,
                "Let's look at the last picture.",
            ),
            (CommonSound::StartPuzzle, "Let's make this one."),
            (CommonSound::ChooseSize, "Please choose how many pieces"),
            (CommonSound::ChoosePiece, "Let's choose a piece."),
            (CommonSound::ReturnPiece, "Let's put this one back for now."),
            (CommonSound::OtherPicture, "Let's make a different"),
        ] {
            let line = include_str!("../validation/common-feedback.tsv")
                .lines()
                .find(|line| {
                    line.split('\t')
                        .nth(4)
                        .is_some_and(|text| text.starts_with(caption))
                })
                .unwrap();
            let original: usize = line.split('\t').next().unwrap().parse().unwrap();
            assert_eq!(event as usize, original, "{event:?}");
        }
    }

    #[cfg_attr(
        not(feature = "asset-tests"),
        ignore = "requires original DOS game assets"
    )]
    #[test]
    fn error_buzz_is_bounded_and_respects_muting() {
        let samples = error_buzz_samples();
        assert_eq!(samples.len(), 2004);
        assert_eq!(samples.iter().min(), Some(&-3200));
        assert_eq!(samples.iter().max(), Some(&3200));
        assert_eq!(
            samples.windows(2).filter(|pair| pair[0] != pair[1]).count(),
            159
        );
        let mut engine = AudioEngine::new(Path::new(env!("CARGO_MANIFEST_DIR")), false, false);
        engine.play_error();
        engine.play_common(CommonSound::NextPicture);
        assert!(engine._stream.is_none());
        assert!(engine.sfx_sinks.is_empty());
    }

    #[cfg_attr(
        not(feature = "asset-tests"),
        ignore = "requires original DOS game assets"
    )]
    #[test]
    fn scene_wait_does_not_overlap_busy_audio_or_trap_a_loop() {
        use crate::assets::{FrameCommand, PuzzleAsset, SceneRecord};
        use crate::puzzle::PuzzleSession;
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut asset = PuzzleAsset::load(dir, 2, false).unwrap();
        let sound = asset
            .sounds
            .iter()
            .position(|entry| entry.tag == "ROBOT")
            .unwrap();
        let frame = asset.records.iter().position(|record| matches!(record,
            SceneRecord::Frame { commands, .. } if commands.iter().any(|command|
                matches!(command, FrameCommand::Sound { snd_idx: Some(index) } if *index == sound))
        )).unwrap();
        asset.sequences[2].start_frame = frame;
        asset.sequences[2].end_frame = frame + 2;
        let mut session = PuzzleSession::new(2, asset, 2, false, false, false);
        let mut audio = AudioEngine::new(dir, false, false);
        audio.sfx_enabled = true;
        session.on_puzzle_completed(&mut audio);
        let (sink, _queue) = Sink::new_idle();
        sink.append(SamplesBuffer::new(
            1,
            NNV_SAMPLE_RATE,
            synthesize_nnv_phrase(&audio.nnv_banks["P02"], 2),
        ));
        audio.sfx_sinks.push(sink);
        session.active_anim.as_mut().unwrap().wait_for_audio = true;
        session.update(10.0, &mut audio);
        assert_eq!(session.active_anim.as_ref().unwrap().cur_frame, frame);
        assert_eq!(audio.sfx_sinks.len(), 1);
        audio.stop_all();
        session.update(0.0, &mut audio);
        assert_eq!(session.active_anim.as_ref().unwrap().cur_frame, frame + 1);
        assert!(
            session
                .active_anim
                .as_ref()
                .unwrap()
                .looping_sound
                .is_some()
        );
        session.update(10.0, &mut audio);
        assert!(session.active_anim.is_none());
        assert!(!audio.is_playing());
    }

    #[cfg_attr(
        not(feature = "asset-tests"),
        ignore = "requires original DOS game assets"
    )]
    #[test]
    fn disabled_audio_does_not_open_a_device() {
        let engine = AudioEngine::new(Path::new(env!("CARGO_MANIFEST_DIR")), false, false);
        assert!(engine._stream.is_none());
        assert!(engine.stream_handle.is_none());
    }

    #[cfg_attr(
        not(feature = "asset-tests"),
        ignore = "requires original DOS game assets"
    )]
    #[test]
    fn iss_uses_original_playback_timing() {
        let engine = AudioEngine::new(Path::new(env!("CARGO_MANIFEST_DIR")), false, false);
        for (stem, ticks) in [("SOUND0", 26u64), ("TITLE", 9217)] {
            let song = &engine.iss_songs[&(stem.to_string(), 0)];
            let expected = ticks * 4452 * u64::from(AUDIO_SAMPLE_RATE) / 1_193_182;
            assert_eq!(synthesize_iss_song(song).len() as u64, expected, "{stem}");
        }
    }

    #[cfg_attr(
        not(feature = "asset-tests"),
        ignore = "requires original DOS game assets"
    )]
    #[test]
    fn iss_matches_original_speaker_control() {
        let engine = AudioEngine::new(Path::new(env!("CARGO_MANIFEST_DIR")), false, false);
        let mut checked = 0;
        for line in include_str!("../validation/iss-oracle.tsv").lines().skip(1) {
            let fields: Vec<_> = line.split('\t').collect();
            assert_eq!(fields.len(), 4);
            let song: usize = fields[1].parse().unwrap();
            let count: usize = fields[2].parse().unwrap();
            let expected = u64::from_str_radix(fields[3], 16).unwrap();
            let ticks =
                iss_speaker_ticks(&engine.iss_songs[&(fields[0].to_string(), song)]).unwrap();
            assert_eq!(ticks.len(), count, "{}:{song}", fields[0]);
            let hash = ticks
                .into_iter()
                .flat_map(|(gate, divisor)| {
                    std::iter::once(u8::from(gate)).chain(divisor.to_le_bytes())
                })
                .fold(14695981039346656037u64, |hash, byte| {
                    (hash ^ u64::from(byte)).wrapping_mul(1099511628211)
                });
            assert_eq!(hash, expected, "{}:{song}", fields[0]);
            checked += 1;
        }
        assert_eq!(checked, 25);
    }

    #[test]
    fn iss_rejects_invalid_events_and_excessive_duration() {
        for song in [
            &[][..],
            &[255, 255, 0, 254][..],
            &[3, 0, 0, 253][..],
            &[3, 0, 0, 89, 0, 88, 0, 88, 0, 88, 0, 254][..],
            &[3, 0, 0, 0, 0, 88, 0, 88, 0, 88, 0, 254][..],
        ] {
            assert!(iss_speaker_ticks(song).is_err());
            assert!(synthesize_iss_song(song).is_empty());
        }
        let long = [3, 0, 0, 253, 255, 127, 88, 0, 88, 0, 88, 0, 88, 0, 254];
        assert_eq!(iss_speaker_ticks(&long), Err("ISS song exceeds one minute"));
    }

    #[cfg_attr(
        not(feature = "asset-tests"),
        ignore = "requires original DOS game assets"
    )]
    #[test]
    fn nnv_matches_original_machine_code() {
        let engine = AudioEngine::new(Path::new(env!("CARGO_MANIFEST_DIR")), false, false);
        let mut checked = 0;
        for line in include_str!("../validation/nnv-oracle.tsv").lines().skip(1) {
            let fields: Vec<_> = line.split('\t').collect();
            assert_eq!(fields.len(), 4);
            let phrase = fields[1].parse().unwrap();
            let count: usize = fields[2].parse().unwrap();
            let expected = u64::from_str_radix(fields[3], 16).unwrap();
            let samples = decode_nnv_phrase(&engine.nnv_banks[fields[0]], phrase)
                .unwrap_or_else(|error| panic!("{}:{phrase}: {error}", fields[0]));
            assert_eq!(samples.len(), count, "{}:{phrase}", fields[0]);
            let hash = samples
                .into_iter()
                .fold(14695981039346656037u64, |hash, byte| {
                    (hash ^ u64::from(byte)).wrapping_mul(1099511628211)
                });
            assert_eq!(hash, expected, "{}:{phrase}", fields[0]);
            checked += 1;
        }
        assert_eq!(checked, 85);
    }

    #[test]
    fn nnv_rejects_truncated_commands_and_unbounded_output() {
        let mut samples = Vec::new();
        assert_eq!(
            decode_nnv_frame(&[128, 128, 128, 128, 0], 0, 0, 3, &mut samples),
            Err("NNV phrase exceeds two minutes")
        );
        samples.clear();
        assert_eq!(
            decode_nnv_frame(&[0x1f, 0x11, 0], 0, 0x40, 2, &mut samples),
            Err("NNV phrase exceeds two minutes")
        );
        let header = [1, 0, 0, 0, 8, 0, 0, 0];
        for command in [&[0x41][..], &[0x42, 0, 0][..], &[0x43, 0, 0, 0][..]] {
            let bank = [header.as_slice(), command].concat();
            assert!(decode_nnv_phrase(&bank, 0).is_err());
        }
        let mut bank = header.to_vec();
        for _ in 0..300 {
            bank.extend_from_slice(&[1, 0]);
        }
        bank.push(15);
        assert!(decode_nnv_phrase(&bank, 0).is_err());
        assert!(decode_nnv_phrase(&[], 0).is_err());
        assert!(decode_nnv_phrase(&header, 1).is_err());
    }

    #[cfg_attr(
        not(feature = "asset-tests"),
        ignore = "requires original DOS game assets"
    )]
    #[test]
    fn nnv_decode_cache_returns_identical_samples() {
        let mut engine = AudioEngine::new(Path::new(env!("CARGO_MANIFEST_DIR")), false, false);
        let first = engine.decoded_nnv_samples("DATA_0", 0);
        let second = engine.decoded_nnv_samples("DATA_0", 0);
        assert!(!first.is_empty());
        assert_eq!(first, second);
        assert!(engine.decoded_nnv_samples("MISSING", 0).is_empty());
    }
}
