# Original preset pieces

The original size selector includes dimension 1, which sets DS:e44 to zero
and DS:e46 to one (loaded instructions 1f35f and 1f383..1f391).
The preset branch in `FUN_1e59_011c` enumerates 0040 commands and calls
`FUN_196c_0e05` with flag mask 3. These are scene subframes, independent
of the generated grid's curved-edge setting.

`preset-bounds.tsv` contains 349 bounds from that original x86 command
routine across all 27 intact supplied scene variants. Every scene's record 4
contains 11–16 pieces. The oracle preserves the source commands, applies the
loader's one-based resource-index adjustment, installs frame/sprite pointer
tables, and executes the relocated bounds code without intercepting it.
The executable's SHA-256 is checked before execution. The Rust regression
compares every result against `compute_frame_bounds`.

Some bounds extend outside the native (2,2,270,196) viewport: corpus extents
reach x=-2 and y=219. A playable implementation must preserve viewport
clipping and each piece's opaque shape, rather than treating these as grid
rectangles. EGA and VGA bounds can differ; geometry must be rebuilt when
switching scene variants.

The recreation exposes this mode as `PARTS` / `ORIGINAL`, alongside the seven
grids. Pieces retain native opacity and viewport clipping. The board renders
the preset frame's background and children in source order, with silhouettes
for unplaced parts. Geometry is rebuilt when switching video variants.
Regression tests exercise placement in all 30 puzzle/video selections,
four board-layer states in both tray positions per selection, keyboard input,
and retained progress. Native rendering precedes presentation so hiding the
tray preserves EGA checker-pattern phase.
The bounds fixture establishes geometry, not complete DOS raster parity.

Real-window validation uses the same corpus bounds as pointer destinations:

```sh
cargo build
python3 validation/window_gameplay.py --preset
```

It uses Xvfb, GDB and actual XTest mouse events, checks 30 completion callbacks,
and never invokes auto-solve. The default invocation still tests 2x2 grids.

The rebuilt window run completed all 30 preset selections with 386 placements;
the grid regression completed another 30 games with 120 placements. EGA P11
retained identical board pixels across a tray toggle, and its pickup caption
contained 2826 black glyph pixels. Logs: `preset-mode-window.log` and
`preset-grid-window.log`. The complete Rust suite reports 79 passing tests
in `preset-mode-tests.log`; strict Clippy also passes.

Reproduce with Unicorn installed and a directory containing decompressed
`scene-Pnn.PZ8.raw` / `scene-Pnn.PZ4.raw` files:

```sh
python3 validation/oracles/preset_bounds.py /path/to/unpacked.exe /path/to/intact-scenes /tmp/preset-bounds.tsv
cmp validation/preset-bounds.tsv /tmp/preset-bounds.tsv
cargo test preset_piece_bounds_match_original_machine_code
```

Exclude P06.PZ4, whose original compressed source is malformed. P07.PZ4
and P10.PZ4 are absent. These inputs must not be replaced by invented data.
