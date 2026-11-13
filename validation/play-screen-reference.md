# Play-screen repair — 2026-11-12

The user rejected the unsolved screen despite the preceding technical checks.
Native raster correctness does not establish presentation quality.

The current screen adds a header with an authentic reference thumbnail,
piece progress and timer; separated tray cards and drawn scroll arrows;
a control footer; and a restrained colored mat instead of black letterboxes.
The 320x200 scene coordinates and source assets are unchanged.

Generated grids start with a faint completed-picture guide. H hides it.
The guide blends only uncovered background in the uploaded RGBA image;
placed and held sprite masks protect their original colors. Grid edges,
hints, captions and the indexed renderer remain separate. Original presets
retain native background/silhouette drawing. Holding Tab or the picture
button shows the full reference and pauses input/timer; releasing resumes.

## Current checks

- `play-screen-tests.log`: 99 passed, 0 failed, 0 ignored.
- `play-screen-release-tests.log`: the same 99 tests pass in release.
- `play-screen-{fmt,clippy,build}.log`: formatter, strict Clippy and debug build.
- `play-screen-release-build.log`: release build.
- `play-screen-assets.log`: all 89 original asset hashes match.
- `play-screen-runtime.log`: actual P01 VGA window, matching the user's scene; reference/timer remain
  unchanged during a held Tab; four mouse placements complete after lookup.
- Raw `play-screen-{unsolved,without-guide,reference,completed}.png` show
  the actual game. `play-screen-{compact,normal,large}.png` cover 480x360,
  960x720 and 1720x1380 windows and were visually inspected.
- `play-screen-background-runtime.log`: four grid and fourteen preset mouse
  placements complete; each completed interior matches 42,500 native pixels.
- `play-screen-window-{grid,preset}.log`: all 60 game windows complete with
  506 mouse placements, no auto-solve; EGA P11 preserves board pixels when
  toggling the tray and retains its visible caption.
- `play-screen-tray-edge.log`: four more game windows, 96 mouse placements,
  including bottom-padding selection in VGA/EGA grid and preset trays.
- `play-screen-sequence.log`: four live P02/P11 animation runs preserve
  native frame orders/speeds, natural completion, restart and skip.
- `play-screen-dissolve.log`: VGA/EGA P05 retain mixed original pixels,
  both dissolve targets, natural completion and immediate skip.

The viewport regression checks geometry and pointer conversion at four
sizes. The guide regression checks original VGA/EGA colors for every opaque
placed/held pixel, leaves source pixels unchanged, and bypasses solved views.

The older native-window scripts now use `click_game` for scene coordinates.
Their `shot_game` captures sample actual displayed pixel centers and normalize
to the historical 960x600 game-only image for existing raster comparisons.
These omit the new header/footer; raw play-screen captures provide UI evidence.
The large Xvfb test moves the pointer inside the window before sending keys,
and asserts the playing header before treating a reference capture as evidence.

This intermediate checkpoint established behavior and observed layouts;
human visual endorsement was not inferred. The later live compositor checks
in `desktop-reference.md` complete the required runtime verification.

## Mouse reference capture

`reference-capture-before.log` reproduces the reference view ending when the
pointer leaves its held button. The button previously required continuous
hover and also activated when an unrelated drag crossed it. It now captures
only a press that starts on the button and stays active until release.

`reference-capture-runtime.log` runs the revised window test: the complete
reference image and timer remain unchanged for 1.2 seconds after leaving the
button; dragging across it does not activate it; four subsequent mouse
placements complete the puzzle. `reference-capture-tests.log` and
`reference-capture-release-tests.log` each report 99 passing tests. The matching
Clippy, formatter and build logs record the current checks.

## Dense mixed-input games and completed artwork

`mixed_input_gameplay.py` completes P01's curved 8x8 grid in VGA and EGA
without auto-solve. Each game makes 16 drag-release placements, 16 keyboard
pickups followed by window resize and mouse placement, 16 placements with
the tray hidden, and 16 ordinary click-carry placements. All 128 pieces are
placed; both completion callbacks are recorded in `mixed-input-runtime-debug.log`.
Windows alternate between 960x720 and 480x360 while carrying 32 pieces.

These actual window captures revealed a duplicate timer painted over the
completed scene. `completed-timer-before-{vga,ega}.png` retain that failure.
`completed-timer-before.log` independently reproduces 326 altered native pixels
in the regression fixture. The in-picture timer is removed; the header and
small-window clock remain. The test compares completed framebuffers with
timer off/on in both video modes.

`completed-timer-{tests,release-tests}.log` now record 100 passing tests.
After the repair, both 64-piece games were replayed; `mixed-input-runtime.log`
and `mixed-input-{vga,ega}.png` record completion and unobscured artwork.
The matching formatter, Clippy and debug/release build logs pass. These focused
tests do not claim physical desktop compositor or human visual acceptance.
