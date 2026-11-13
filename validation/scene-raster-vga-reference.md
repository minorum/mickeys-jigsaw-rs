# Native VGA scene raster comparison

This check composes complete display frames with the original scene traversal,
leaf wrappers and VGA drawing instructions. It extends the sprite-level
matrix to nested scene graphs, backgrounds and command order.

`oracles/scene_raster.py` uses two isolated Unicorn machines, one for the main
program and one for the VGA driver. Their checked unpacked SHA-256 values are:

- Main: `565a5c5601ede858cfdc289e6dd1f3bbde7c2ec2399e52dad2dce3d1b165d465`
- VGA: `02de74a740e4fb51108e78f2da655983399674b35191a53fb6391dad05772630`

MZ relocations are applied. Main DS data is copied into the isolated scene
context, with SS=DS. Original frame/sprite tables are installed with the same
zero-based resource fixup used by the native loader. Display dimensions are
320x200; horizontal presentation shift is zero. The source scene viewport
is (2,2,270,196), as established separately from the original game.

The main program runs display entry 196c:17a3, including native recursive
196c:14f0 and native bounds/reflection helpers. Its sprite/rectangle wrappers
also execute: inherited flags, silhouette colors and draw modes are not
reimplemented in Python. Only graphics transport at 196c:0008 is intercepted.
Each drawing command executes the corresponding original VGA opcode handler.
Sprite pointers are rebound to the same source bytes in the driver machine.
The prebound offscreen destination replaces buffer-binding/dirty-list commands
06/09; those commands are not claims about page switching. Unknown drawing
opcodes assert rather than silently disappear. No sprite/rectangle raster
loop is intercepted or replaced.

Coverage selects the static FILM frame plus all frames in sequences named by
FILM default and hotspot descriptors. Arbitrary library frames are excluded
as display roots: the original root dispatcher accepts SubFrame commands,
while internal resources draw through the nested dispatcher. Across all 15
VGA assets this yields 3,079 complete frames and 585,527 native draw commands
(up to 373 per frame). The native result hashes all 64,000 indexed pixels.

Rust independently loads the original files, initializes the same destination
and viewport, and calls its actual recursive renderer. All 3,079 framebuffer
hashes match. The regression checks the fixture count exactly. No production
change was required by this comparison.

```
python3 validation/oracles/scene_raster.py /tmp/mickey-original-unpacked/final/output.000.exe /tmp/mickey-vga-unpacked/output.000.exe /tmp/preset-native-corpus validation/scene-raster-vga.tsv
cargo test vga_scene_rasters_match_original_traversal_and_driver
```

Limits: this establishes VGA frame raster equivalence under the source
viewport and offscreen-target profile. It does not establish the equivalent
whole-scene EGA result, physical video page switching, audio timing, or
caption/UI overlays. Animation timing/dissolves and modern presentation have
their separate native/runtime evidence. The source assets remain unchanged.
