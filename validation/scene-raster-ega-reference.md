# Native EGA scenes and background-pattern repair

The scene oracle now runs the original traversal, drawing wrappers, packed
sprite converter and EGA opcode handlers together. It reuses the calibrated
mode-2 masked-write model from `ega_sprite_blits.py`, including native
reflection-table initialization and fresh-latch assertions. Buffer binding and
dirty-list commands use the same prebound offscreen profile as the VGA check.
This does not emulate physical page switching or every EGA hardware mode.

All 12 intact PZ4 scenes contribute 2,507 eligible display frames and 482,529
native draw commands. Together with VGA, the reference covers 5,586 display
frames and 1,068,056 draws. Root selection remains static FILM plus sequences
selected by FILM default/hotspot descriptors. The 27 zero resource references
in the corpus belong to unused EDIT frame 1; no background or subframe targets
that frame. They do not substitute an asset sprite for a displayed UI resource.

This comparison exposed a real renderer defect. P03's static frame 3 uses
background frame 194, whose background word is `0x0204`. The original renderer
alternates palette colors 4 and 2; Rust discarded the high byte and painted
solid color 4. The native/rust static images differed in 6,298 visible pixels.
The retained failing scene test identifies P03 frame 3; a focused regression
also fails before the repair. Diagnostic raster captures show the difference.

The shared background path now uses the existing two-color rectangle helper
with the complete word. It preserves clipping and native-coordinate phase;
equal palette bytes remain a fast flat fill. The old viewport test now encodes
its flat color as `0x0505`. No asset-specific branch or source-file edit is used.
The historical `0xffff = transparent` documentation was also corrected.

After repair all 2,507 EGA framebuffers match, as do all 3,079 VGA frames.
Both debug and release suites pass 96 tests. Re-generating the previous VGA
scene and EGA sprite references after making the model reusable produces
byte-identical fixtures; see the recheck logs.

The rebuilt game completed P03 EGA in a 2x2 grid and the original 14-part
layout: 18 mouse placements, no auto-solve. GDB confirms completion before the
animation-skip key. Each resulting window capture matches 42,500 native scene
pixels, excluding decorative borders/UI. Expected RGB comes from the native
indexed result and source PALE bytes. See `background-pattern-runtime.log`,
its debugger trace, and grid/preset screenshots. Audio/timer/captions were
disabled in this targeted pixel test.

```
python3 validation/oracles/scene_raster.py /tmp/mickey-original-unpacked/final/output.000.exe /tmp/mickey-ega-unpacked/output.000.exe /tmp/preset-native-corpus validation/scene-raster-ega.tsv
python3 validation/oracles/scene_raster.py /tmp/mickey-original-unpacked/final/output.000.exe /tmp/mickey-ega-unpacked/output.000.exe /tmp/preset-native-corpus /tmp/p03-native-frame.tsv --asset P03.PZ4 --frame 3 --pixels /tmp/p03-native-frame.idx
python3 validation/background_pattern_gameplay.py /tmp/p03-native-frame.idx /tmp/preset-native-corpus/scene-P03.PZ4.raw
cargo test
cargo test --release
```

The damaged/missing EGA files retain their documented VGA fallbacks. Physical
display/speaker behavior and cycle-exact scheduling remain unverified. Native
frame raster, modern presentation, input and audio evidence are distinct;
this check does not claim a complete DOS hardware emulator.
