# Current state — 2026-11-12

The requested audit/repair/validation pass is complete. The rejected unsolved
screen has been reworked and checked in actual compact/normal/large windows
and the live Niri/Xwayland desktop at 100% and 125% display scale. Reference
pause and fullscreen mouse completion pass on both desktop outputs. No
confirmed defect or required verification item remains. Read [COMPLETION_AUDIT.md](COMPLETION_AUDIT.md) for requirement-by-
requirement acceptance and limits, [README.md](README.md) for running/controls,
and [AUDIT.md](AUDIT.md) for the historical findings and repairs.

The game supports all 15 puzzles, generated 2x2 through 8x8 grids with both
edge styles, and original 11–16-part presets. Mouse carry/drag and keyboard
play, hints, timer, print export, captions/audio, solved interactions,
pause/resume and live video refresh have regression/runtime evidence.
Progress is retained for one unfinished puzzle within the process.

# Latest repair

Live desktop verification now covers Niri/Xwayland at 1920x1080 and 5120x1440,
including a 125% scaled display. Own-window compositor captures, paused
reference/timer and full mouse completion pass on both. The test copy uses
isolated preferences and no audio; the original focus is restored afterward.
See `validation/desktop-reference.md` and `desktop{,-scaled}-runtime.log`.
Current final checks use `final-desktop-` logs; 100 tests pass in both profiles.

# Completed-artwork repair

Removed the duplicate timer painted over the completed picture. The header
owns the clock, including its existing small-window fallback. A regression
reproduced 326 altered scene pixels in its VGA fixture and now checks an
unchanged completed framebuffer with timer on/off in VGA and EGA.

Two actual 64-piece curved games now complete with 128 placements combining
drag releases, keyboard pickup, 32 resizes while carrying, hidden-tray
placement and click-carry. Before/after completion captures retain the exposed
timer failure and clean native picture. Current checks use `completed-timer-`
and `mixed-input-` logs; 100 tests pass in debug and release, strict Clippy,
formatter and both builds pass.

# Reference input repair

The mouse reference button now captures its initial press until release.
It no longer ends the reference on pointer exit or activates on an unrelated
drag crossing it. The window regression reproduces the old failure, then
checks 1.2 seconds of unchanged reference/timer after exit, ignores a crossing
drag, and completes the puzzle. Current checks use `reference-capture-` logs;
99 tests pass in debug and release, with strict Clippy, formatter and builds.

# Native scene repair

P03's EGA backdrop uses the two-color background word `0x0204`. Rust had
discarded its high byte, replacing 6,298 visible static-scene pixels with
solid color 4. The shared background path now uses the existing pattern
helper with the full word. No asset-specific branch or source edit was used.

All 3,079 VGA and 2,507 intact EGA display framebuffers now match original
traversal, wrappers and driver handlers under the documented offscreen
profiles: 1,068,056 native draws. The EGA path uses actual native conversion
and a calibrated bounded masked-write model. Native frame raster, modern
presentation, audio timing and physical hardware are distinct evidence scopes.
See `validation/scene-raster-ega-reference.md`.

# Verification

- 100 tests passed, none ignored, in both debug and release.
- Formatter and strict Clippy passed; debug and release builds passed.
- All 89 original asset hashes match `validation/assets.sha256`.
- 450 in-process configurations make 12,566 placements; all 134 active FILM
  hotspot centers select and complete the expected animation.
- Real-window matrices complete 30 generated-grid games (120 placements),
  30 preset games (386), and four tray-edge games (96), without auto-solve.
- The latest P03 EGA grid/preset window test completes 18 mouse placements,
  confirms completion before animation skip, and checks 42,500 displayed
  native scene pixels per layout.
- All 85 NNV phrases and 25 ISS songs retain original-code fixture checks.
  Actual isolated ALSA captures are non-silent; human listening is not claimed.

Current UI checks use `play-screen-` logs and `play-screen-reference.md`.
Native raster logs use `scene-raster-ega-` and `scene-raster-release-` prefixes.
Before logs retain the reproduced failures. Earlier checks and references are
indexed in `validation/README.md`. Source `ARCHITECTURE.md` includes historical
reverse-engineering notes and is not the authoritative format specification.

Host verification uses `/usr/bin/cargo-fmt-1.99.0 fmt -- --check` and
`/usr/bin/cargo-clippy-1.99.0 clippy --all-targets -- -D warnings` because the
corresponding rustup components are unavailable. Standard `cargo test`,
`cargo test --release`, `cargo build` and `cargo build --release` succeed.

# Documented limits and follow-up scope

Credits and P06 EGA are malformed under original decoding; P07/P10 EGA files
are absent. Keep the credits diagnostic/skip and valid original VGA fallbacks.
Pristine recovery requires intact source data, not invented bytes.

Physical DOS page switching, PIT phase, analog speaker response, human
listening and cycle-exact scheduling are not established. Responsive cue
replacement, reading time and dissolve duration are deliberate desktop choices.

Joystick, legacy hardware selectors and saving progress across process exits
are not implemented and were not explicit required features. Full historical
dialog scripting, hostile-input fuzzing and additional platforms would be new
follow-up scopes. Human visual endorsement and listening are not claimed;
actual rendered layouts and input behavior have been verified.

For a future change, re-run the relevant regressions and inspect finished logs.
Do not modify original assets or infer fresh runtime proof from old captures.
No worker owns source files and no concurrent builds should share a target dir.
