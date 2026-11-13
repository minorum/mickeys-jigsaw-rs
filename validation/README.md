# Validation evidence

Current active suite: **100 passed, 0 failed, 0 ignored** in debug and release;
see `final-desktop-tests.log` and `final-desktop-release-tests.log`.
Live desktop compositor captures and completion at 100%/125% scaling are
documented in `desktop-reference.md`; both output runs pass.
`mixed-input-runtime.log` adds two complete 64-piece games with mixed mouse,
keyboard, resize and tray input. Their screenshots exposed and now verify
removal of the duplicate clock covering the completed artwork.
The revised unsolved screen, reference pause and responsive mouse mapping have
real-window checks; see `play-screen-reference.md` and raw `play-screen-*.png`.
Native whole-scene drawing matches 3,079 VGA and 2,507 EGA framebuffers.
The EGA comparison caught and fixed P03's lost backdrop pattern byte; real
grid/preset windows now match native scene pixels after mouse completion.
See `scene-raster-ega-reference.md` and `scene-raster-vga-reference.md`.
Native EGA drawing matches 2,984 framebuffer hashes
under the calibrated model in `ega-sprite-blits-reference.md`.
Native EGA conversion and all source pixel reads
match Rust; see `ega-sprite-pixels-reference.md`.
Native common-feedback replacement/muting traces
and Rodio queue checks are in `common-lifecycle-reference.md`.
Full-frame VGA blits and packed four-bit pixel reads
match native drivers; see `sprite-blits-reference.md`.
Native VGA pixel reads cover all 885 sprites and
expose the corrected nonzero transparency keys; see `sprite-pixels-reference.md`.
The original NNV
integration is now enabled and passes. `nnv-known-failure.log` is historical
pre-repair evidence; see `nnv-oracle.md` for the original-code comparison.
`test.log`, `clippy.log`, `build.log`, and `fmt.log` are completed command logs.
The formatter check produces an empty log on success. All four final checks
returned zero. Before-repair logs retain the reproduced failures.

`iss-oracle.md` documents exact original-code control-stream comparisons for all
25 supplied songs. `scene-resource-inventory.md` records scene validation and
the corrected solved-character click mapping. The joint suite exercises the
real audio module, strict scene parsing, P06 fallback, and character clicks.
`iss-runtime.json` records 11.67 seconds of isolated ALSA capture from the real
window. `p06-ega-fallback-runtime.png` shows its complete original VGA scene
after selecting EGA and completing the puzzle. No audio was listened to.
`solved-click-runtime.log` records real XTest clicks under GDB on the debug
binary: completion starts sequence 2, Goofy `(188,24)` starts 5, overlap
`(131,178)` starts 3, and unmapped `(155,24)` starts default 2. Breakpoints only
log `start_sequence` arguments; production code was not instrumented.
`browser-current.png` shows the corrected puzzle-ID label.

Speech-bubble continuation: `speech-bubble-reference.md` records original-code
format, alternative selection, rendering, and wait semantics. Tests cover all
93 supplied nonempty captions, truncation, zero placeholders, the intro-once
mode, muted reading waits, the Text preference, and signed-coordinate clipping.
`speech-bubble-runtime.png` is an actual window capture after selecting Goofy;
`speech-setup-runtime.png` shows the working Speech Text control. Its isolated
ALSA statistics are in `speech-bubble-runtime.json`; audio was not listened to.
The later correction uses original logical-color tables rather than raw logical
indices. `bubble-color-before.log` records the reproduced VGA mismatch;
`speech-bubble-ega-runtime.png` shows correct EGA fill and ink. Both VGA/EGA
window runs completed with non-silent isolated output.

`oracles/scene_audio.py` executes original repeated-effect control and countdown
code and checks original color-table bytes; `scene-audio-oracle.log` contains
the completed run. All five repeating entries and busy/wait/cancellation cases
are covered in Rust tests. `repeated-effect-runtime-check.log` records multiple
real restart calls; its JSON report records 9.92 seconds of captured audio.
These checks do not establish cycle-exact DOS scheduling or audible quality.

`drag-runtime-check.log` records one XTest left-button press in the first tray
slot, held motion to logical `(273,34)`, then one release. A GDB breakpoint
immediately after placement confirmed piece 2 was placed. `drag-placed-runtime.png`
shows it attached to the actual board. No second placement click was sent.
Regression tests exercise all seven grid sizes, click-to-carry compatibility,
outside/control releases, gesture cancellation, and returning a piece once.

`resume-runtime-check.log` verifies drag/place → Escape → RESUME/Enter retains
the exact placed-picture pixel region, then Escape → R clears it. GDB recorded
two new-session constructions: initial start and explicit restart. Screenshots
are `resume-browser-runtime.png`, `resumed-board-runtime.png`, and
`restarted-board-runtime.png`. Unit tests also retain held piece, tray order,
and elapsed time, reject incompatible options/solved games, and preserve the
old session when an explicit restart fails to load assets.

`piece-colors-before.log` reproduces source index 248 becoming 0 in P02, 2×2,
straight edges, at picture `(32,40)`. Pieces now use explicit coverage rather
than replacing a valid color with black. The exact-picture coverage regression
checks all 15 puzzles, both video selections, seven grids and both edge styles
(420 cases). A separate test covers all 256 indices through normal blits,
thumbnails and hint silhouettes, plus a transparent mask pixel.
`piece-colors-runtime-check.log` confirms actual P02 piece-0 placement and checks
the captured RGB at logical `(80,42)` against source color 248: `(250,239,250)`.
See `p02-piece-colors-runtime.png`. This tests piece fidelity to the rendered
asset picture; it does not establish full original-DOS scene-rendering parity.

`live-video-before.log` reproduces setup changing only the preview while active
sprites remained 8-bit and the old animation remained active. Live switches now
retain placed/held pieces, tray/scroll, hint, toolbox state, timer and completion,
while rebuilding source assets and cancelling old animation/drag/audio state.
Tests cover both directions, P06 fallback, failed loading, and atomic rollback
when the preview loads but the different paused puzzle cannot load.
`live-video-runtime-check.log` records P01 VGA → EGA → VGA with one game start
and two visual rebuilds. The placed-picture region changes in EGA and returns
pixel-for-pixel in VGA; see `video-switch-before.png`, `video-switch-ega.png`,
and `video-switch-back.png`. Those historical captures predate the UI palette
repair below; this check establishes asset switching and progress retention.

UI palette continuation: `ui-palette-before.log` records hardcoded control-color
failure; `toolbox-mode-before.log` records the VGA-toolbox mismatch on an EGA
scene; `toolbox-index-before.log` records silent out-of-range variant clamping.
The supplied toolbox contains three exact-boundary variants: VGA (65 sprites),
EGA (62), CGA (62). The game caches VGA/EGA variants and chooses using actual
scene depth, retaining VGA for P06/P07/P10 fallbacks. No source pixels are remapped.
Custom controls resolve intended RGB colors through the current scene palette.

`ui-runtime-check.log` checks actual EGA banner/button RGB, neutral gray tray,
black/white-only timer text, and a neutral VGA-fallback tray. Captures are
`ega-browser-ui-runtime.png`, `ega-puzzle-ui-runtime.png`, and
`vga-fallback-ui-runtime.png`. Regression tests also check the loaded mode-specific
toolbox against rendered gameplay and explicit rejection of variant index 3.

`bubblegum-reference.md` corrects the unsupported global-intermission claim.
`completion-before.log` reproduces the fabricated screen after completion 3;
`completion-runtime-check.log` confirms all three completions now return to
identical browser pixels (`third-completion-browser-runtime.png`). The original
P11 tape/full-animation sound routes and speech wait remain covered in both
asset modes. `p11-native-tape-runtime-check.log` traces frame 161, sound 1,
voice 0, and records 13.72 seconds of isolated output. See its JSON statistics
and `p11-native-tape-runtime.png`. Audio was not listened to.

`assets.sha256` covers every original file in commit `864f434`. Each current file
was also compared byte for byte with its original Git blob before writing the
manifest. There were no mismatches.

Runtime used an isolated Xvfb display `:93`, 1024×768, with the actual 960×600
Macroquad game window and XTest mouse/keyboard input. The game was launched with
`--setup` for deterministic navigation. Screenshots were captured from XGetImage,
not reconstructed from the renderer's data.

Checked in the running game:

- Setup → browser → P01 VGA puzzle; readable controls and fitted thumbnails.
- Click first tray slot; F1 hint; place piece 2 at logical `(273, 34)`.
  `placed-piece-vga.png` shows it attached to the board and removed from the tray.
- F4 → F4 → Escape preserves return to the active puzzle.
- F2 completion, click to skip animation, top-left return to browser.
- Switch to EGA in setup; browse/play/complete P01 with corrected colors.
- Select P07 while EGA remains selected; start and complete using its VGA fallback.
  `completion-p07-fallback.png` shows the original scene rendering.

The title was also observed in a running window before these checks. The audio
continuation additionally captured real game playback through an isolated ALSA
backend; see `audio-runtime.json` and `nnv-oracle.md`. Audio was not listened to.
No human playability evaluation, physical-display testing,
performance benchmark, or original-DOS side-by-side comparison was performed.
Software OpenGL reports expected DRI3 warnings under Xvfb; no application panic
appeared in the final runtime log. The game and private X server were stopped.
