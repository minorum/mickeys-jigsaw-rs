# Mickey's Jigsaw Puzzles (DOS, 1991) — Reverse Engineering & Rust Recreation Architecture

> Historical working notes, not a verified specification. The 2026-11-11 audit
> found incorrect palette, audio, asset-count, and setup claims below. See
> [AUDIT.md](AUDIT.md) and [README.md](README.md) for current behavior and limits.

This document records the reverse-engineered technical specification of **Mickey's Jigsaw Puzzles** (1991, Walt Disney Computer Software, Inc. / Novotrade, MS-DOS) and the architecture of its native Rust recreation.

---

## 1. Executable Packing & Binary Structure

The original game ships with a main executable (`MICKEY.EXE`, 85,786 bytes) and four video-mode driver overlays (`VGA.BIN`, `EGA.BIN`, `CGA.BIN`, `TANDY.BIN`).

1. **Two-Layer Executable Compression**:
   - **Outer Layer (`MICKEY.EXE`)**: Compressed with **PKLITE v1.13** (`90-91 PKWARE Inc.`). Unpacking produces a 140,864-byte MZ executable.
   - **Inner Layer (`MICKEY.EXE` & `*.BIN`)**: Compressed with **Microsoft EXEPACK** (`RB` signature).
     - Notice that `VGA.BIN`, `EGA.BIN`, `CGA.BIN`, and `TANDY.BIN` are actually relocatable MZ executables (`MZ` header + EXEPACK stub) loaded dynamically at runtime via `seg_143a` (`FUN_243a_0010`), relocated to their target segment, and invoked through a unified jump table at `0x0064` (`0x11` entry points, `FUN_1000_00b2` in `VGA.BIN`).
2. **Compiler & Runtime**:
   - Compiled with **Microsoft C 5.10** (16-bit real-mode medium/large model, `SS == DS == DGROUP` at relocated segment `0x1811`).
   - Contains **462 functions** across 25 code segments in `MICKEY.EXE` (`0000` through `1811`) and **48 functions** in `VGA.BIN`.

---

## 2. Compression & Container Formats

### 2.1 `dA\0\0` Compression (`seg_165f`)
Almost all data files (`*.PZ8`, `*.PZ4`, `*.PRT`, `MJP.TTL`, `MJP.CRD`, `MJP.TBX`) use a custom LZ77 + Canonical Prefix Code bitstream with magic `dA\x00\x00`:
- **20-Byte Header**:
  - `+0x00`: `[u8; 4]` magic = `b"dA\x00\x00"`
  - `+0x04`: `u32` total container size minus 8
  - `+0x08`: `u16` flags (`1`)
  - `+0x0A`: `u16` compression method (`1` = Huffman/prefix-coded LZ77, `0` = raw)
  - `+0x0C`: `u32` compressed payload size
  - `+0x10`: `u32` uncompressed size
- **Canonical Prefix Code Tables (`+0x14..+0x34`)**:
  - `+0x14..+0x24` (16 bytes = 32 nibbles): Code lengths (`1..16` bits) for 64 length/high-distance symbols (`0..63`, low nibble first).
  - `+0x24..+0x34` (16 bytes = 32 nibbles): Code lengths (`1..16` bits) for 64 low-distance symbols (`0..63`, low nibble first).
  - Both trees are constructed identically by sorting symbols by `(bit_length, symbol_id)` and assigning canonical MSB-first bit patterns, while reading bits **LSB-first** (`bit = (cur_byte >> bit_pos) & 1`, `bit_pos = 0..7`) from the stream starting at `+0x34`.
- **Bitstream Decoding (`FUN_265f_021c`)**:
  - `bit == 1`: Read 8 bits (LSB-first) as a literal byte and append to the output buffer (and 4096-byte sliding window).
  - `bit == 0`: Decode `sym1` (`0..63`) from Tree 1 and `sym2` (`0..63`) from Tree 2:
    - `dist_high = sym1 & 0x3F` (wait: in `MICKEY.EXE`, `sym1` in `FUN_265f_021c` encodes match length when decoded first, and `dist_high` is the top bits, or more specifically: first symbol `ls` from `len_tree` gives match length `ls + 2`, or if `ls == 63`, reads an 8-bit byte `extra` for `65 + extra`; then decodes `dh` from `dist_tree` and reads 6 raw bits `dl` for `dist = ((dh << 6) | dl) + 1`).
    - Copies `match_len` bytes from `out[out.len() - dist]` (clamped to `uncomp_size`).

### 2.2 Multi-Resource Containers (`seg_1747`)
`MJP.TTL` (Title screen), `MJP.CRD` (Credits screens), and `MJP.TBX` (Toolbox sprites) store multiple video-mode resources inside a single `dA\0\0` stream:
- `+0x00`: `u16` resource count (`N`)
- `+0x02`: `[u32; N]` byte sizes of each resource
- `+0x02 + 4*N`: Concatenated resource payloads:
  - In `MJP.TTL` and `MJP.TBX`: `N = 3` (`0` = VGA 256-color, `1` = EGA/Tandy 16-color, `2` = CGA 4-color).
  - In `MJP.CRD`: `N = 6` (`0, 1` = VGA Credits Pages 1 & 2; `2, 3` = EGA/Tandy Credits Pages 1 & 2; `4, 5` = CGA Credits Pages 1 & 2).

---

## 3. Graphics, Fonts & Scene Graph Formats

### 3.1 IFF `ILBM` Full-Screen Images (`MJP.TTL`, `MJP.CRD`, `seg_0701`)
Each resource in `MJP.TTL` and `MJP.CRD` is an Electronic Arts IFF `FORM` -> `ILBM` image:
- `BMHD` (20 bytes): `width = 320`, `height = 200`, `nPlanes = 8` (VGA), `4` (EGA/Tandy), or `2` (CGA), `compression = 0` (uncompressed planar scanlines).
- `CMAP`: `3 * (1 << nPlanes)` bytes of 8-bit RGB (`0..255`).
- `BODY`: `200` scanlines, each consisting of `nPlanes` bitplanes of `40` bytes (`320` pixels, MSB `0x80` = leftmost pixel). De-planarized in `VGA.BIN` (`1000:172f`) into 8-bit chunky indices `0..255`.

### 3.2 Proportional Font (`MJP.NFT`, `seg_0cd2:02fb`, `seg_07ac:01f8`)
`MJP.NFT` (1,206 bytes on disk) is a proportional 1-bit font strip:
- `0x00..0x08`: `"FONT\xae\x04\x00\x00"` (`0x04AE = 1198` payload bytes)
- `0x08..0x10`: `"PROPER  "` (8-byte font name)
- `0x10..0x14`: `u16 width_px = 616`, `u16 height_px = 10` (`77` bytes per row)
- `0x14..0xD4`: `[u16; 96]` X-coordinate offsets (`0..616`) for ASCII `32..127`
- `0xD4..0x194`: `[u16; 96]` character pixel widths for ASCII `32..127`
- `0x194..0x4B6` (`802` bytes): Plane mask byte (`0x0F`) followed by 10 row packets. Each row packet starts with `row_cmd_bytes` (`0x50 = 80`), followed by `[count, literal_bytes...]` chunks (`0x3F` = 63 bytes + `0x0E` = 14 bytes = `77` bytes/row), unpacking to a `77 x 10 = 770`-byte 1-bit horizontal strip.

### 3.3 Toolbox (`MJP.TBX`) & Puzzle Files (`P01..P15.PZ8` & `.PZ4`)
- **`MJP.TBX`** (`seg_0cd2:0631`):
  - `PALE` chunk: `768` bytes of 6-bit VGA RGB (`0..63`) in Chunk 0 (`54` bytes in Chunk 1, `12` bytes in Chunk 2).
  - `TBOX` chunk: Sequence of **65** `0x8061` sprite records (`0..64`) containing all UI icons (Mickey hand cursors, toolbox arrows, puzzle browser thumbnails, piece-count icons `4, 9, 16, 25, 36, 49, 64`, action icons for Hint/Solve/Print/Quit, etc.).
- **`P01..P15.PZ8` (256-color VGA) & `.PZ4` (16-color EGA/Tandy)** (`seg_0cd2:0d15`):
  - `PALE` chunk(s): `768` bytes of 6-bit RGB (`0..63`) for VGA (note `P03.PZ8` contains two `PALE` chunks; the second overrides the first).
  - `MCG0` (in `.PZ8`) or `TND0` (in `.PZ4`) chunk: Stream of typed records until EOF:
    1. **Records with `(rtype & 0x8000) == 0`**: `u16 rtype`, `u16 rsz` (stored sequentially in `c7c4[0..N]`):
       - **`0x7ff0` (Sequence Header)**: `+8..+16` is an 8-byte ASCII name:
         - `c7c4[0]` (`"91020601"`): Static completed puzzle picture (`0x7ff1` at `c7c4[1]`).
         - `c7c4[2]` (`"FILM    "`): Full completion animation sequence + **interactive hotspot table** (`+16`: `u16 num_hotspots`, followed by 16-byte hotspot entries: `i16 x1, y1, x2, y2`, `char seq_name[8]`, e.g. `"F_ALL   "`, `"F_MICKEY"`, `"F_DONALD"`, `"F_GOOFY "`, `"F_PLUTO "`, `"F_MINNIE"`, `"F_DAISY "`).
         - Subsequent `0x7ff0` headers define the frame ranges `[header_idx + 1 .. next_header_idx)` for each named animation (`F_ALL`, `F_MICKEY`, etc.).
       - **`0x7ff1` (Scene / Animation Frame)**:
         - `+6`: `u16 bg_frame_idx` (`0xFFFF` if none, otherwise renders `c7c4[bg_frame_idx]` first)
         - `+8`: `u16 bg_color`: alternating low/high palette bytes; equal bytes give a flat background. `P03.PZ4` uses `0x0204`. This is a pattern word, not a transparency sentinel; see `validation/scene-raster-ega-reference.md`.
         - `+10`: `u16 sub_cnt`
         - `+12`: `sub_cnt` variable-length drawing commands (`u16 cmd_type, u16 cmd_size`):
           - `0x0020`: Trigger `0x8023` audio/speech-bubble entry (`u16 snd_idx` at `+4`).
           - `0x0030`: Frame delay (`u16 delay_ticks` at `+4`).
           - `0x0040`: Draw recursive `0x7ff1` sub-frame (`i16 dx` at `+4`, `i16 dy` at `+6`, `u16 frame_idx` at `+8`, `u16 flags` at `+10`: bit 0 = flip X, bit 1 = flip Y).
           - `0x0050`: Filled rectangle (`i16 x` at `+4`, `i16 y` at `+6`, `u16 color` at `+8`, `u16 flags` at `+10`, `u16 w` at `+12`, `u16 h` at `+14`).
           - `0x0060`: Draw `0x8061` sprite (`i16 x` at `+4`, `i16 y` at `+6`, `u16 sprite_idx` in `aeb6` at `+8`, `u16 flags` at `+10`: bit 0 = flip X, bit 1 = flip Y).
           - `0x0080`: Draw text (`i16 x` at `+4`, `i16 y` at `+6`, `u16 text_idx` in `aeb6` at `+8`, `u16 flags` at `+10`, `u16 font_color` at `+12`).
           - `0x0090`: Draw polygon (`i16 x` at `+4`, `i16 y` at `+6`, `u16 poly_idx` in `aeb6` at `+8`, `u16 flags` at `+10`, plus colors/transform at `+12..+26`).
    2. **Records with `(rtype & 0x8000) != 0`**: `u16 rtype`, `u32 rsz`:
       - **`0x8023` (Audio & Speech-Bubble Table)**: Stored in `c774[0..K]`. Contains an 8-byte ASCII tag at `+6..+14`, `u16 count` at `+16`, `count` 14-byte audio descriptors (`char stem[8], u16 phrase_idx, u32 runtime_ptr`), and `count - 1` speech-bubble polygon + text descriptors.
       - **`0x8061` (Sprite Bitmap)**: Stored in `aeb6[0..M]`.
         - `+0x00`: `u16 magic = 0x8061`
         - `+0x02`: `u32 rsz`
         - `+0x06`: `u32 runtime_ptr`
         - `+0x0A`: `u16 stride`, `+0x0C`: `u16 height`, `+0x0E`: format word (zero in supplied scenes), `+0x10`: low byte = depth (`8` in `.PZ8`, `4` in `.PZ4`), high byte = transparent palette index. Native VGA pixel reads confirm the key at `+0x11`; see `validation/sprite-pixels-reference.md`.
         - `+0x12`: `u16 sub_x`, `+0x14`: `u16 sub_y`, `+0x16`: `u16 sub_w`, `+0x18`: `u16 sub_h`
         - `+0x1A..+rsz`: Raw pixel bytes (`stride * height` bytes in 8-bit `.PZ8`; 4-bit packed nibbles in `.PZ4`).

---

## 4. Jigsaw Puzzle Mechanics & Interlocking Curves (`seg_0e59`)

- **Playfield Dimensions**: `270 x 196` pixels (within the `320 x 200` screen; offset at `(48, 2)` when the left-hand toolbox is visible, or centered/bordered at `(25, 2)` when hidden).
- **7 Difficulty Sizes (`2x2 = 4` up to `8x8 = 64` pieces)**:
  - **Column Widths (`sum = 270`, `DS:0x12AE`)**:
    - `2x2`: `[135, 135]`
    - `3x3`: `[90, 90, 90]`
    - `4x4`: `[67, 68, 68, 67]`
    - `5x5`: `[54, 54, 54, 54, 54]`
    - `6x6`: `[45, 45, 45, 45, 45, 45]`
    - `7x7`: `[39, 38, 39, 38, 39, 38, 39]`
    - `8x8`: `[33, 34, 34, 34, 34, 34, 34, 33]`
  - **Row Heights (`sum = 196`)**:
    - `2x2`: `[98, 98]`
    - `3x3`: `[65, 66, 65]`
    - `4x4`: `[49, 49, 49, 49]`
    - `5x5`: `[39, 39, 40, 39, 39]`
    - `6x6`: `[32, 33, 33, 33, 33, 32]`
    - `7x7`: `[28, 28, 28, 28, 28, 28, 28]`
    - `8x8`: `[24, 25, 24, 25, 25, 24, 25, 24]`
  - **Interlocking Tab/Blank Curves**:
    - Stored in `MICKEY.EXE` `DGROUP` at `DS:0x3D82..0x4B85` (indexed via `DS:0x4B86` and `DS:0x4B94`), providing 4 vertical edge profiles and 4 horizontal edge profiles per difficulty level. Each shared interior border between adjacent pieces is randomly assigned one of the 4 profiles and a tab direction (`+1` or `-1`), ensuring adjacent pieces interlock with pixel-exact complementarity.
- **Hotkeys & Features**:
  - `F1`: Hint (flashes/shows where the currently held or next piece belongs).
  - `F2`: Auto-solve puzzle (`FUN_1e59_1136`).
  - `Space` / Right-Click: Toggle Toolbox visibility or return piece to toolbox.
  - Completion triggers the full `"FILM    "` animation followed by interactive hotspot mode where clicking different parts of the picture plays character-specific animations (`F_MICKEY`, `F_DONALD`, `F_GOOFY`, `F_ALL`, etc.) and digitized speech/SFX.
  - P11's full animation and tape interaction play `BUBGUM.NNV` through the original scene sound records and speech-wait command. See `validation/bubblegum-reference.md`.

---

## 5. Audio Engine (`.ESS`, `.NNV`, `.ISS`)

1. **`.ESS` Speech/SFX Containers (`PROG.ESS`, `DATA.ESS`)**:
   - `u16 count`, `[u32; count]` sizes, followed by `count` `.NNV` speech/SFX banks.
2. **`.NNV` Digitized Speech & Sound Effects (`seg_1363`, `9,321.73 Hz`)**:
   - Header: `u32 num_phrases`, `[u32; num_phrases]` phrase command offsets, sample frame pool, and bytecode command streams.
   - Sample frames use `0x00` (8-bit raw PCM), `0x30` (3-bit block-floating-point PCM via `1363:1958`), `0x40` (4-bit block-floating-point PCM via `1363:19D8`), or `0x50` (5-bit block-floating-point PCM via `1363:1AD8`).
   - Phrase bytecode opcodes: `1` (silence), `2` (single sample frame), `3` (stepped packet repeat), `4` (frame repeat), `5` (pitch-contour glottal pulse train for speech vowels), `6` (2-byte NOP), `0`/`7` (end of phrase).
3. **`.ISS` Music (`TITLE.ISS`, `SOUND0..3.ISS`, `seg_175e`, EMI 5.10 at `268.0103 Hz`)**:
   - Multi-song container (`u16 count`, `[u32; count]` song lengths).
   - 4-voice time-division multiplexed synthesizer with 88-note PIT divider table (`175e:09F6`), custom synth/percussion envelopes, tempo commands (`0xFD`), loops (`0x00FF`), and end markers (`0x00FE`).

---

## 6. Rust Project Structure (Modular Bevy + SDL3 Architecture)

The Rust recreation separates platform/rendering (`sdl3`, `rodio`) from authoritative gameplay (`bevy_app`, `bevy_ecs`, `bevy_state`, `bevy_time`, `bevy_math`):

- **Application Lifecycle & Scheduling (`src/app.rs`, `src/ecs/plugins.rs`)**:
  - SDL3 owns the OS window, GPU/streaming textures, audio device loop, and raw platform events (`src/sdl_platform.rs`, `src/input.rs`), calling `step_app(&mut app, &input)` (`App::update()`) once per frame.
  - `CoreGamePlugin`, `AudioSyncPlugin`, `BrowserPlugin`, `SetupPlugin`, `PuzzlePlugin`, and `RenderTrackingPlugin` organize systems into chained `GameSet` stages (`GlobalAndScreenTransitions`, `MenuAndBrowserInput`, `PuzzleTimersAndAnimation`, `PuzzleInputAndActions`, `PuzzleCompletion`, `SyncAndChangeTracking`).
- **State Machine (`bevy_state`)**:
  - `AppScreen` (`Title`, `Credits { page }`, `Browser`, `Setup { return_to_playing }`, `Playing`) is a `bevy_state::state::States` state machine. `flush_state_transitions_system` in `PostUpdate` ensures same-frame `NextState<AppScreen>` transitions and `OnEnter`/`OnExit` hooks.
- **Timing Model (`bevy_time`)**:
  - `TimeUpdateStrategy::ManualDuration` feeds deterministic frame `dt` into `Time<Real>` and `Time<Virtual>`.
  - `sync_virtual_time_pause_system` pauses `Time<Virtual>` outside `Playing` or while `InGameMenuState` / confirmation prompts are open, preserving DOS pause semantics for puzzle timers and animations, while `Time<Real>` drives UI status/hint countdowns and `AudioEngine`.
- **Single-Source-of-Truth ECS Data Model (`src/ecs/components.rs`, `src/ecs/resources.rs`)**:
  - Each puzzle piece is an entity with `PieceIdentity`, `PieceGeometry`, `PieceEdges`, `PieceSprite`, and a single authoritative `PieceLocation` (`Tray { order }`, `Board { row, col }`, or `Held { is_anchor, group_offset, group_order }`), eliminating duplicate `board_cells`, `tray_order`, `held_piece`, and `placed` flags.
  - Interactive scene hotspots (`SceneHotspot`), active completion/character animations (`AnimationState`), and hint highlights (`HintHighlight`) are independent entities.
  - `track_render_changes_system` derives `RenderVersions` (`app_rev`, `board_rev`, `tray_rev`, `visual_rev`) from ECS change detection and authoritative entity hashes rather than manual `bump_*` counters.
- **Parametric Bézier Jigsaw Geometry (`src/puzzle.rs`, `src/ecs/puzzle_ops.rs`)**:
  - `edge_curve` generates deterministic interlocking tab/socket polylines from a canonical cubic-Bézier profile evaluated with `bevy_math::Vec2`, scaled/reflected per variant and shared across adjacent pieces so cuts partition the playfield with zero gaps or overlaps.
- **Audio & Asset Backends (`src/audio.rs`, `src/assets.rs`, `src/decompress.rs`, `src/graphics.rs`, `src/puzzle_render.rs`)**:
  - `AudioEngine` (`rodio`) is stored as a `NonSend` resource, updated in `PreUpdate`, synced to `AudioStatus`, and driven via `AudioCommand` messages drained in `PostUpdate`.
  - `AppRenderer` and `PuzzleRenderer` extract read-only render state from `&World` / `&mut World` into the `320x200` indexed framebuffer and RGBA texture for SDL3 presentation.
