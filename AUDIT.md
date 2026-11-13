# Audit and repairs — 2026-11-11

Current acceptance evidence is in [COMPLETION_AUDIT.md](COMPLETION_AUDIT.md):
100 tests pass in both debug and release; all 5,586 selected VGA/EGA scene
framebuffers match original-code references. The dated baseline and earlier
open questions below retain the investigation history; current evidence
supersedes the old sprite/scene raster uncertainty.

The user's unsolved-screen screenshot exposed a separate acceptance failure:
a blank board, sparse tray and cramped timer. The current presentation adds
an optional faint reference, full-picture peek, progress, readable timer,
piece cards and correctly mapped responsive viewport. See
`validation/play-screen-reference.md`. The final live desktop checks include
Niri/Xwayland window captures and fullscreen completion on two outputs,
including fractional scaling; see `validation/desktop-reference.md`.

A follow-up window regression caught hover-dependent reference holding.
The button now captures a press through release and ignores crossing drags;
the original failure and successful runtime check use `reference-capture-` logs.

Mixed-input 64-piece window runs then exposed the old completion timer still
painting over the scene beneath the new header timer. It is removed; a VGA/EGA
pixel regression and repeated mixed-input completions cover the repair. See
`completed-timer-before.log`, `completed-timer-tests.log` and `mixed-input-runtime.log`.

The initial Rust implementation was entirely untracked. Commit `b87b8a9`
preserves that implementation before repairs; original game files were already
tracked. No original asset was edited. The pre-existing architecture notes contain
contradictory format claims and are not a verified specification.

## Baseline

- Seven Rust modules, roughly 4,500 lines, Macroquad 0.4 and Rodio 0.19.
- CPU indexed framebuffer, recursive scene graph, nearest-neighbor texture upload.
- Asset loaders feed a puzzle session, a screen state machine, and synchronous
  audio synthesis. No unsafe Rust in the application.
- Six tests: two passed, four failed. See `validation/baseline-test.log`.
- Tests never started puzzles beyond P01 or checked decoded palette values.
- Existing piece opacity checks counted transparent pixels as content.

## Prioritized findings and results

| Priority | Confirmed problem | Repair / evidence |
| --- | --- | --- |
| High | Debug overflow starting P04 and later | Wrapping seed arithmetic; all 15 puzzles start in tests |
| High | VGA colors blown out by clamping RGB bytes to 63 | Copy original RGB bytes; byte-for-byte palette tests across all 15 assets |
| High | EGA colors shifted by a fictitious six-byte palette header | Read palette from byte zero; tests across all 13 supplied EGA assets |
| High | P07 sprites with depth `0x0208` became transparent | Decode the low depth byte; validate pixel payloads rather than hiding failures |
| High | NNV decoder misreads commands, addresses, scales, and repeat/pitch packets | Reconstruct against original machine code; all 85 phrase counts and whole-output hashes match |
| High | Scene sound count and descriptor data read two bytes early | Read original count at 24 and descriptors at 28; verify all scene speech references |
| High | Large tray pieces overlap and are clipped past recognition | Fit thumbnails inside each slot; full-size held pieces remain unchanged |
| Medium | Five-slot tray padding selects an invisible sixth piece | Bound selection to the last visible slot; all layouts, scroll positions, mouse/keyboard handlers and four real-window completions cover the edge |
| High | Opaque puzzle color 248 changed to black in P02 and other pictures | Separate piece coverage from palette colors; compare all 420 puzzle/video/grid/edge combinations with the rendered picture |
| Medium | Held-image center differs from snap center for asymmetric tabs | Anchor the held image to the base-cell center |
| Medium | Wrong toolbox sprites spill into the board | Use font arrows inside the existing controls |
| Medium | Hardcoded UI color 15 is dark gray in VGA puzzle palettes | Select white from the loaded palette without changing asset colors |
| Medium | UI palette indices and VGA toolbox used for EGA scenes | Resolve control colors through the scene palette; use the original EGA toolbox for actual EGA assets and VGA toolbox for fallbacks |
| Medium | Missing P07/P10 EGA files silently prevented selection | Explicit fallback to the supplied VGA assets |
| Medium | Truncated resources padded with fabricated pixels/data | Reject truncated DA, containers, ILBM bodies, and sprite payloads |
| Medium | An unknown DA prefix code becomes a zero symbol and can manufacture output | Return an error from prefix lookup and propagate it through literal/distance/length decoding; retain valid coded zero data and corpus hashes |
| Medium | Container code searched arbitrary payloads for FORM | Remove signature-scanning workaround; retain exact resource boundaries |
| Medium | Credits decode consumes bits beyond EOF | Report and skip credits; no replacement image or invented bytes |
| Medium | Disabled music continues; detached effects evade stop_all | Own effect sinks; stop disabled categories immediately |
| Medium | Silent tests still open physical audio devices and can stall | Initialize output only when music or effects are enabled; lazy enable remains supported |
| Medium | Standalone speech banks never loaded; aliases overwrite real SFX files | Load original named NNV files and preserve them over aliases |
| Medium | Animation discards elapsed remainder | Advance all elapsed frames and retain the remainder |
| Medium | Every setup interaction reloads the preview from disk | Reload only when video mode changes |
| Medium | F4 inside setup loses the return-to-game destination | Ignore nested setup entry |
| Medium | Letterbox clicks activate edge controls | Reject clicks outside the viewport and puzzle screen |
| Medium | Print/config write failures silently disappear | Surface write errors; regression tests cover failures and exact PBM output |
| Low | Setup exposes unimplemented hardware choices | Show implemented settings only; preserve legacy fields in saved data |
| Medium | Fabricated circle intermission interrupts every third completion | Remove unsupported global mode/counter; retain original P11 tape/full-animation bubblegum playback |

Some initial test failures were invalid assumptions: the toolbox contains 65
sprites, not 61; VGA and EGA scenes need not have equal sprite counts; and credits
resources include multiple video modes. These expectations were corrected without
claiming that unsupported decoders now work.

## Original preset-piece mode

The original preset-piece mode is now implemented as `PARTS` / `ORIGINAL`.
Native x86 fixtures establish all 349 piece bounds across 27 intact variants;
30 requested puzzle/video selections exercise placement, including source
fallbacks. Board checks cover four placement states in both tray positions,
source-order layering, silhouette colors, clipping and native EGA pattern
phase. Resume, live asset replacement and keyboard input retain this mode.
See [preset evidence](validation/preset-pieces-reference.md) for reproduction
and the remaining DOS raster-parity limit.

## Documented limits and historical open questions

1. **Physical audio and common dialogue fidelity (medium).** Common feedback now
   uses the original 15-entry PROG/DATA table; seven modern UI actions use verified
   cues. Guessed aliases and absent SFX-4/SFX-5 requests were removed. Incorrect
   placement now produces the original 80-cycle buzz with a fixed desktop pitch.
   The seven bound common feedback cues now display their original captions.
   Historical dialogue sequencing remains incomplete.
   See `validation/common-feedback-reference.md`. All 85 NNV phrases match original
   samples; all 25 ISS songs match original speaker control streams. Desktop
   synthesis does not model physical PIT phase or analog speaker response.
2. **Credits stream (medium).** Strict decoding reports `Truncated DA bitstream`.
   Before the truncation check, later FORM headers appeared 19 bytes past their
   declared boundaries and a header byte was corrupt. An independent Info-ZIP
   implode decode reproduced the displacement and reported insufficient compressed
   input (30,755 bytes needed versus 30,300 supplied). Executing the original
   decoder now confirms the same short output and displaced headers, with a
   successful title-file positive control. This establishes malformed source
   data rather than a missing format detail. See `validation/credits-oracle.md`.
   Do not alter the source asset to conceal it.
3. **Scene fidelity (medium).** Original sound-resource speech bubbles are now
   parsed and rendered (52 VGA texts, 41 EGA texts), with matching alternative
   voice selection and speech waits. All five bit-7 repeated effects now retain
   and restart their selected descriptor; original control bytes and real P02
   playback verify the behavior. Cycle-exact scheduling remains unverified. Original-machine-code bounds
   comparison found and repaired premature child reflection in rendering, bounds
   and hit testing. All 64 nested flip combinations now match; full sprite pixel
   parity remains unproven. See `validation/frame-bounds-reference.md`.
   P11 EGA frame 168 now retains its original two-color rectangle pattern;
   original EGA driver writes and clipping regressions verify parity and colors.
   See `validation/ega-rectangle-reference.md`.
   Original-machine-code dispatch tests establish that P03 EGA command `00a0`
   is ignored by both render dispatchers; see `validation/scene-dispatch-reference.md`. Text/polygon frame commands do
   not occur in the supplied scenes. Solved-character clicks now use the original
   FILM actor frame and last matching command. Corpus-wide regression now checks
   all 134 active FILM hotspot centers across 30 VGA/EGA selections, starts each expected
   animation through solved click handling, completes it with audio disabled,
   and renders its initial/final state. This includes original VGA fallbacks. Malformed
   P06 EGA is rejected and falls back to its complete original VGA scene. See
   `validation/scene-resource-inventory.md`; loading is not full scene parity.
   Toolbox panels, shadows, browser controls and error text now use palette-aware
   colors. The original EGA toolbox is selected for EGA scenes; VGA fallback
   scenes retain the VGA toolbox. See the new UI runtime captures.
   Sequence commands now retain native repetition counts and default frame
   delays. All 48 original scheduler traces match, including zero/one-pass
   semantics, wrapping and explicit delay overrides. P02 robot and P11 phone
   window traces check repetition, speed, natural completion, restart and skip
   in both video modes. See `validation/sequence-playback-reference.md`.
   P05's projector now uses native presentation mode 8 instead of a hard cut.
   Both driver pixel orders match original execution; midpoint/source-pixel,
   completion and skip checks pass in real VGA/EGA windows. The nonblocking
   desktop duration is deliberately capped within the frame delay, with
   original CPU-speed timing unclaimed. See `validation/dissolve-reference.md`.
4. **Input/session behavior (medium).** Both click-to-carry and drag-and-release
   are implemented. Drag release ignores toolbar actions and outside-window
   coordinates; navigation cancels the gesture without dropping the held piece.
   Tray visibility changes also cancel an active drag while retaining the held
   piece; this prevents release against a board shifted by 23 pixels.
   Keyboard-only play now uses WASD to move a bounded logical pointer and Enter
   to invoke existing click handling, including solved hotspots and menus.
   Mouse press returns to mouse control. Joystick input remains absent.
   Browser re-entry now resumes a matching unfinished puzzle, preserving board,
   tray, held piece, and timer; R explicitly restarts. Progress is not saved
   across application exits. Video-mode changes now refresh the active puzzle
   and preview atomically, retain progress, and cancel the old animation/audio.
   Failure retains both old visuals and the previous video setting.
   Setup now pauses both audio channels together with animation updates and
   caption countdowns; returning resumes the existing playback position.
5. **Defensive parsing (low).** DA output is capped and truncation is rejected,
   but this is not a complete hostile-input/fuzzing audit. Recognized scene commands,
   puzzle chunk declarations, font rows and glyph bounds reject truncation. Puzzle
   palettes require at least 16 complete RGB colors. Other asset-loader paths have not
   received equivalent malformed-input coverage. Prefix-tree variants beyond the supplied
   assets have not been validated.

## Verification

Formatting, Clippy with warnings denied, tests, and a debug build were run using
the installed Gentoo Rust 1.99 tools. The rustup shim lacks the fmt/clippy
components, so the working formatter and linter entry points on this host are:

```sh
/usr/bin/cargo-fmt-1.99.0 fmt -- --check
/usr/bin/cargo-clippy-1.99.0 clippy --all-targets -- -D warnings
cargo test
cargo build
```

The existing scene renderer's three large argument lists have targeted Clippy
expectations; interfaces were preserved instead of adding a renderer abstraction.
Original curve tables are excluded from rustfmt expansion to keep their diff small.

Active tests cover exact palette values, asset availability, all puzzle starts,
all seven grid dimensions with both edge modes, exact pixel coverage without gaps
or overlaps, click-to-complete gameplay, animation timing, thumbnail bounds, setup
failure handling, malformed inputs, and PBM byte preservation. Original NNV
integration is enabled and passes after the decoder repair.
The completion regression now plays all 420 puzzle/video/grid/edge configurations
through pointer handlers, placing 12180 pieces and checking completion, animation
skip, idle rendering and menu return. See `validation/all-game-configurations.md`.

First-pass suite: **21 passed, 0 failed, 1 ignored**. A separate read-only code review
found no important introduced regressions in the inspected parsing, audio,
animation, setup, and input paths. The review was static; runtime evidence is
listed separately in `validation/README.md`.

An earlier continuation passed **74 tests, 0 failed, 0 ignored**. The NNV oracle
executes original decoder bytes from an independently unpacked executable; see
`validation/nnv-oracle.md`. A real game run with an isolated ALSA capture backend
produced non-silent output and remained responsive through starting and solving
a puzzle; capture statistics are in `validation/audio-runtime.json`.
The subsequent ISS/P06 window run captured 11.67 seconds of audio and completed
P06 after selecting EGA; see `validation/iss-runtime.json` and its screenshot.

Xvfb with software OpenGL permits real window rendering and XTest input. Runtime
screenshots and the checked paths are recorded under `validation/`. This is not
physical-display, human usability, audible-output, or original DOS parity proof.

## Historical continuation priorities

Scene rendering, retained dialogue ordering, interactive animations and
input/session behavior now have the evidence linked by the completion audit.
Full physical DOS scheduling remains a documented fidelity limit.
The unsupported global bubblegum intermission was removed after tracing original
P11 F_ALL/F_TAPE audio and completion control flow; see
`validation/bubblegum-reference.md`.
NNV reconstruction is complete for the supplied phrase corpus. Credits recovery
requires an intact source file rather than another decoder workaround.

## Font boundary repair

Font loading now rejects truncated rows, packets crossing row boundaries, short or
oversized decoded rows, zero dimensions and glyphs outside the strip. It no longer
pads missing bitmap bytes or silently discards excess bytes. Regression coverage
checks every truncated prefix of the supplied payload, malformed packet lengths
and glyph bounds. All 770 original bitmap bytes are checked against the original
literal packets; no source font data was changed. The optional trailing byte is
preserved as an allowed trailer.

## Noncanonical preference toggle repair

Setup displayed every nonzero preference as ON, but XOR toggles trapped values
such as 2 and 65535 in an enabled state. Music, effects, timer, grid and Speech
Text now toggle according to zero/nonzero state and produce canonical 0/1 values.
Regression inputs cover 1, 2, 3 and 65535. Actual setup loaded five flags set to 2,
disabled them and saved zeroes in an isolated asset directory. See
`validation/preferences-runtime-check.log` and screenshot. No original settings
or assets were modified.

The earlier right-edge presentation question is resolved by the scene viewport
repair below. Native source framing is established; physical DOS UI equivalence
remains a separate unverified claim.

## Muted scene caption selection

P12 LOOK-12 has both a music alternative and captioned speech. With music and
SFX disabled, the selector previously took the inactive music path and omitted
its speech caption despite Speech Text being enabled. The selector now permits
speech/caption selection when music is disabled. Music preference retains its
priority when enabled and SFX is disabled. Both supplied video variants have
regressions for muted captions, Speech Text off, and music priority. The actual
window shows Donald's "Hey guys, look over here." while both audio settings are
OFF; animation continues afterward. See `validation/muted-caption-runtime.png`
and the before/corpus/runtime logs. No asset data or decoder changed.

## Window-level completion matrix and feedback transition

Real XTest input now completes all 15 puzzles in both requested video modes,
placing 120 pieces without auto-solve. The harness verifies actual completion
callbacks after each game; representative window screenshots are retained. See
`validation/window-gameplay.md` for scope, dependencies and reproduction.
The run exposed stale pickup captions on the completion animation. Completion
now stops outgoing common feedback before starting the scene sequence; all 420
in-process configurations assert the caption is cleared after the last piece.

## Keyboard-only gameplay

WASD now moves a visible, bounded logical pointer and Enter activates the existing
click path. Players can pick/return/place pieces, skip animations, activate solved
hotspots and open the picture menu without a mouse. The existing arrow-key tray
scroll and Space tray toggle remain available. Mouse press returns control to the
mouse. This is a modern control scheme rather than native DOS keyboard parity.
Regression covers completing all seven grids, bounds and mouse handoff. A real
key-only P01 window run completed four pieces, skipped animation and returned to
the browser; the real 30-game mouse matrix also passes on this build. See
`validation/keyboard-gameplay.md`. No original asset or runtime dependency changed.

## Original piece source and 8x8 grid repair

Original-machine-code capture of all 203 base grid rectangles establishes source
origin (2,2). Rust incorrectly sampled (25,2), shifting artwork 23 pixels. The
source origin is corrected independently of display placement. The same oracle
also corrected the first four 8x8 row heights to 25,24,25,24; all 203 base
rectangles now match. The all-configuration pixel-coverage test now uses the
native source origin, rather than agreeing with the former incorrect assumption.
See `validation/piece-capture-reference.md` and the retained failing pixel test.
This establishes base capture geometry, not full original curved-mask or sprite
raster parity. No source data was altered.

## Scene viewport presentation repair

Original viewport setup now has a four-state machine-code oracle. The picture
interior is (2,2,270,196); rendering the full physical backing frame exposed
unused background at the right. Browser previews, animations and completed
pictures now show the exact native interior in a centered frame. Scene background
clears respect clipping. Rendering in native coordinates before presentation
preserves EGA checker phase; hotspot input and speech geometry use the matching
23-pixel presentation offset. All 134 active FILM hotspot centers select/complete their
expected sequences. A new 30-selection pixel-continuity test compares assembled
hidden-tray boards with idle completed pictures. See
`validation/scene-viewport-reference.md`. The earlier right-edge uncertainty is
resolved at the source-framing level; physical DOS UI equivalence is not claimed.

## Partial scene command and chunk rejection

Puzzle scene parsing no longer clamps a declared chunk length to available input
or silently omits undersized recognized sequence/frame commands. It reports the
truncated command or chunk. Palettes shorter than 16 RGB colors are rejected; a complete 48-byte
16-color palette is decoded directly rather than silently left black. These are
puzzle-loader changes; the toolbox loop is unchanged.

Regressions cover all 86 undersized prefixes of nine recognized command types,
complete minimum-size payloads, palette sizes 0..48, and an oversized main-chunk
declaration. Unknown 00a0 commands remain skippable, matching the original dispatch
evidence. Retained before logs show accepted short 0010 and clamped main-chunk
cases failing the new assertions. Existing valid asset, EGA fallback, all-game,
hotspot, pixel and audio tests remain enabled. This is bounded malformed-input
coverage, not a full fuzzing or semantic graph-validation claim.

## Toolbox boundary and palette repair

The toolbox loader now rejects oversized chunk declarations, invalid/truncated
records, incomplete trailing headers and empty sprite resources. Palette decoding
uses the loaded sprite depth: the supplied CGA resource needs its full 12-byte
four-color palette, not a 48-byte minimum. Complete 48-byte EGA palettes also
load instead of remaining black. All original variants retain their 65/62/62
sprites. Original-machine-code DA output hashes now guard all three decoded
resources; see `validation/toolbox-boundary-reference.md`. This is asset parsing
coverage, not physical CGA gameplay or exhaustive fuzz validation.

## Native sprite transparency repair

Original VGA pixel reads across all 885 supplied sprites exposed a wrong
transparency field: record byte 17 holds the key, not byte 15. P07 sprite 38
and P10 sprite 37 use key 2, so the old decoder discarded color-zero art and
drew/hit-tested transparent background. The shared decoder now preserves the
original key; tests cover every stored color and all four orientations of
the exceptional sprites through rendering and hit testing. See
`validation/sprite-pixels-reference.md`. Complete native blit/clipping and
EGA raster parity were subsequently checked under the documented offscreen
profiles; see `validation/scene-raster-ega-reference.md`.

## Background pattern repair

The complete EGA scene comparison found P03's `0x0204` background flattened
to palette color 4. Native rendering alternates colors 4 and 2; 6,298 static
picture pixels were wrong. The shared background path now preserves the full
word through the existing pattern helper, including clipping and source phase.
All 2,507 EGA and 3,079 VGA display frames match original traversal/driver
output. Focused before/after regressions and two real P03 games verify the
repair: 18 mouse placements and 42,500 displayed native pixels per layout.
Source assets are unchanged. See `validation/scene-raster-ega-reference.md`.
