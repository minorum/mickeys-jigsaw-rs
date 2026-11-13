# Native sprite raster checks

The previous transparency repair is now guarded by full-frame comparisons
against the original VGA drawing routines, beyond selected pixel reads.

`oracles/sprite_blits.py` verifies the unpacked VGA driver's SHA-256, applies
its MZ relocations, installs original sprite metadata and raw data, and runs
the native drawing entry at 1000:1bb8. The driver selects its own normal,
horizontal, vertical or combined reflected routine and performs clipping.
The test uses the native offscreen path (drawing-target selector 2) with a
320-byte row stride and an initialized 320x200 indexed destination. It does
not replace native loops with Python drawing code or emulate VGA ports.

Each of the 885 supplied VGA sprites is drawn in all four orientations.
Cases include a fully visible sprite, left clipping, bottom/right clipping,
and top clipping; clip rectangles alternate between the full screen and the
original scene viewport. Opaque copy and color-42 silhouette cases also cover
the first sprite of each puzzle and every sprite with a nonzero key. There
are 3,684 full 64,000-byte framebuffer hashes. Rust loads each source asset,
sets the same clip rectangle, and compares its entire framebuffer hash.
Every case matches; this check required no additional production change.

The packed four-bit source path is checked independently with the original
Tandy driver's pixel-read opcode at 1000:3061. `sprite_pixels.py` selects the
driver by its checked image hash. All 722 sprites in the 12 intact supplied
PZ4 scenes are covered, sampling the first occurrence of every stored color
(3,936 native reads). Rust's EGA asset selection decodes the same packed
source. All sample position/result hashes match. P06's damaged PZ4 and the
absent P07/P10 PZ4 files are excluded; their existing VGA fallbacks remain.
The Tandy comparison establishes packed-source decoding, not a new Tandy
display mode or physical Tandy hardware behavior.

```
python3 validation/oracles/sprite_blits.py /tmp/mickey-vga-unpacked/output.000.exe /tmp/preset-native-corpus validation/sprite-blits.tsv
python3 validation/oracles/sprite_pixels.py /tmp/mickey-tandy-unpacked/output.000.exe /tmp/preset-native-corpus validation/sprite-pixels-tandy.tsv
cargo test vga_sprite_blits_match_original_driver
cargo test packed_sprite_pixels_match_original_driver
```

The Tandy driver was unpacked from the unchanged source `TANDY.OVL` with
Deark's `exepack` module. Its unpacked SHA-256 is
`84bc097a9a38cdfc951b8c7f1d5011cb359e4722724a7e8948696f70fd6d2f08`.

Remaining limits: this is a finite raster matrix, not every possible clip,
flag or malformed sprite. Native EGA packed-to-planar conversion and planar
blits, recursive whole-frame raster parity, and feedback lifecycle timing
remain unverified. No original source assets were modified.
