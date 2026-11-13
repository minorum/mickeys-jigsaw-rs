# Completion audit — 2026-11-12

The requested audit, repairs and validation are complete. The user's rejected
unsolved screen was repaired with a faint picture guide, reference peek,
piece cards, separate timer/progress and responsive input mapping. Actual
compact/normal/large window captures and live Niri/Xwayland captures on 100%
and 125% scaled displays were inspected. Fullscreen mouse completion and
reference pause pass on both displays; see `validation/desktop-reference.md`.
All 15 puzzles are playable with the seven grid
sizes, both edge styles and original preset pieces in the supported VGA/EGA
selections, including documented source fallbacks. Source and
hardware limits are listed separately.

| Original requirement | Authoritative evidence inspected | Result |
| --- | --- | --- |
| Inspect architecture, dependencies and documentation | Seven Rust modules, screen/session/asset/audio boundaries; Cargo manifest/lock; `validation/final-dependencies.log`; current README/handoff and historical audit | Audited; Macroquad 0.4.16 and Rodio 0.19 retained; no whole-project rewrite |
| Identify working, broken, incomplete and unnecessarily complex behavior | Ranked findings and before/after evidence in `AUDIT.md`; malformed source distinguished from decoder defects | Confirmed defects repaired; unsupported legacy features explicitly documented |
| Understand original parsing, loading and rendering without trusting assumptions | Native DA, audio, geometry, conversion and drawing oracles; 27 intact scene graphs; 89-file manifest | Supplied-corpus decoding and displayed scene raster verified against original code |
| Examine gameplay, rendering, input, audio, UI and state management | Current 100-test suite, real-window matrices, audio captures, animation/dissolve traces and targeted regression windows | Exercised beyond compilation; unit/native/runtime scopes remain distinct |
| Find decoding errors, artifacts, gameplay inconsistencies and broken interactions | Reproduced overflow, palette, alpha, geometry, timing, tray, dialogue, input/state and background-pattern failures | Root causes repaired; failures and regressions retained |
| Distinguish confirmed bugs from suspicions and prioritize playability | `AUDIT.md` ranked findings; native/visual before-after checks | Fundamental failures repaired before cosmetic work; no speculative rewrites |
| Execute repairs autonomously and fix shared causes | Shared coordinate/mask/decoder/audio paths; typed playback settings; latest background fix uses the full pattern word | Changes reach callers and tests; no per-puzzle rendering workaround |
| Improve rendering, responsiveness, controls, gameplay, UI and reliability | Correct native scenes/pieces, nonblocking transitions, bounded thumbnails, drag/carry/keyboard controls, captions, pause/resume, atomic video changes and visible I/O errors | Working end-to-end behavior validated |
| Preserve original assets; avoid invented data/placeholders | `validation/scene-raster-ega-assets.log`: 89/89 hashes match; native source-damage checks | No source asset modified; valid originals used for fallbacks |
| Keep Rust idiomatic, dependencies minimal, and avoid unnecessary unsafe/abstractions | Source review, strict Clippy, unchanged two direct dependencies | No application unsafe or new runtime dependency; existing module boundaries retained |
| Run formatter, Clippy, tests and build | Current `final-desktop-{fmt,clippy,tests,build}.log`; `final-desktop-release-{tests,build}.log` | Formatter/strict Clippy pass; debug and release builds pass; both suites: 100 passed, 0 failed, 0 ignored |
| Launch and verify actual gameplay | Standard 30-game window matrix: 120 mouse placements; preset 30-game matrix: 386; tray-edge runs: 96; latest P03 grid/preset: 18 | Actual XTest input, completion traces and rendered output; no auto-solve in placement matrices |
| Add regressions and recheck significant changes | 450 in-process configurations; current native raster and source/audio fixtures; targeted P03 window RGB comparison | 12,566 handler placements; 5,586 displayed native frame hashes; 42,500 native scene pixels checked per P03 window layout |
| Clearly document unverifiable behavior | Limits below, README, `AUDIT.md` and per-feature references | No claim of complete DOS hardware emulation or physical listening/display qualification |

The 450 configurations are 15 puzzles × 2 requested video modes × 7 grid
sizes × 2 edge styles, plus 15 × 2 preset selections. They make 12,180 grid
and 386 preset placements through handlers, not 450 separate OS windows.
All 134 active FILM hotspot centers select and complete the expected animation
in those handlers. Separate real-window tests cover keyboard-only play,
drag/release, menus, resume, video refresh, audio/captions and animation timing.

Recent repairs include the invisible sixth tray-slot selection, invalid DA
prefixes manufacturing zero bytes, nonzero sprite transparency keys, and P03's
discarded background-pattern byte. The latest fix restores 6,298 P03 EGA
pixels in the static scene. Its grid and 14-part games now complete with their
displayed interiors matching native output. See
[background repair and EGA scene evidence](validation/scene-raster-ega-reference.md).

Raster verification now covers 3,079 VGA and 2,507 EGA display frames through
original traversal/wrappers and original driver handlers: 1,068,056 native draws.
The EGA path uses a calibrated, bounded masked-write model. Sprite matrices
add 3,684 VGA and 2,984 EGA full-frame cases; original EGA conversion/pixel reads
also cover all 749,756 pixels of the 722 intact packed sprites. Original NNV
output hashes cover all 85 supplied phrases; ISS control streams cover all 25
songs. Common dispatcher ordering has 120 original-code traces. References
and reproduction commands are indexed in `validation/README.md`.

## Documented limits

- Credits and P06 EGA are malformed even under original decoding; P07/P10 EGA
  files are absent. Credits are diagnosed/skipped and affected puzzles use
  their valid original VGA scenes. Pristine recovery requires intact data,
  not invented bytes.
- Native raster comparisons use the documented source viewport and offscreen
  target. Physical page switching, PIT phase, analog speaker response,
  human listening and cycle-exact DOS scheduling are not established.
- Responsive cue replacement, caption reading time and dissolve duration are
  deliberate desktop choices. Full historical dialog scripting is not claimed.
- Joystick, legacy printer/sound hardware selectors and progress saving across
  process exits are not implemented. They were not explicit requested features
  and do not prevent the supported keyboard/mouse game from being fully playable.
- Malformed-input regressions cover concrete repaired boundaries; this is not
  a complete hostile-input/fuzzing or cross-platform certification.

The latest mixed-input windows add 128 placements through two dense curved
puzzles, including 32 resizes while carrying. They exposed and verified repair
of the duplicate completed-picture timer; its pixel regression covers VGA/EGA.
The final desktop checks add real compositor captures and mouse completion
at 1920x1080 and 5120x1440, including fractional scaling. No confirmed defect
or required implementation/verification item remains after this audit.
