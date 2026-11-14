# Mickey's Jigsaw Puzzles — Rust recreation

Native desktop recreation using the original game files in this directory.
Requires Rust, SDL3 development libraries, and the platform audio libraries
used by Rodio (including ALSA development files on Linux).

```sh
cargo run
cargo run -- --setup
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Run from the asset directory, or place the assets next to the executable.
The original assets are read only. Settings go to `mjp_config.pre`; printable
images go to `prints/Pxx.pbm`.

Piece count cycles through the seven grids and an original preset mode with
11–16 scene-defined parts. The browser labels this mode `PARTS` / `ORIGINAL`;
edge-shape and grid-line settings apply to generated grids.

| Screen | Controls |
| --- | --- |
| Title | Enter, Space, or click to continue |
| Browser | Left/Right: puzzle; Up/Down: piece count; Tab/E: edge shape; Enter: play/resume; R: restart |
| Puzzle | Drag a tray piece to its destination, or click to pick/place; return it to the tray |
| Puzzle (Touch) | Tap/drag pieces; long-press or 2-finger tap: toolbox / pick up connected group; 2-finger drag: scroll tray |
| Puzzle / completed picture | WASD: move keyboard pointer; Enter: click; mouse press returns to mouse control |
| Puzzle | Up/Down or wheel: scroll tray; right-click/Space: show or hide tray |
| Puzzle | F1: hint; F2: solve; F3/G: grid; F5/P: export line art |
| Puzzle | H: toggle faint picture guide; hold Tab or the picture button: view reference and pause |
| Puzzle | Escape: browser; F4: setup |
| Completed picture | Click or keyboard pointer + Enter: skip/replay animations and activate hotspots; top-left: browser |
| Setup | Up/Down: select; Left/Right/Enter: change; Escape: apply and return |

Setup exposes VGA/EGA, music, effects, timer, grid, and speech-text options.
Settings apply immediately, including VGA/EGA changes to the active puzzle.

The play screen has a reference thumbnail, piece progress, a separate timer,
and controls below the board. The faint guide uses the original completed
picture and preserves placed/held artwork. It applies to generated grids;
original presets retain their native silhouettes. H changes the guide for the
current process. The full reference temporarily pauses play without changing pieces.
The mouse reference stays open until release, including outside its button.
The play screen has also been exercised on the live Niri/Xwayland desktop,
including fullscreen completion on a 125% scaled display; see
[desktop verification](validation/desktop-reference.md).
Save & Return persists settings. Unsupported legacy sound hardware,
printer hardware, and joystick selectors have been removed from the UI.
P07 and P10 have no supplied EGA assets and use the original VGA files instead.
P06's supplied EGA scene is malformed and also falls back to original VGA.

Returning from an unfinished puzzle to the browser preserves the board, held
piece, tray, and timer. The matching puzzle/options show RESUME; R starts over.
Starting a different puzzle or changing its options replaces the current game.
Progress is retained only while the application is running.
Video changes retain progress and stop the current animation. If loading fails,
the existing puzzle and video setting are preserved and an error is shown.

Pictures use the original 270x196 scene interior in a centered frame. Hotspots
and dialogue follow the same presentation offset; source assets remain unchanged.

The supplied puzzles are playable in the supported desktop modes; this is not
a complete DOS hardware recreation. NNV speech/effects are checked against
the original machine code for all 85 supplied phrases. The supplied credits
stream is malformed even under the original decoder and is skipped with a
terminal diagnostic. All 25 ISS songs match the original PC-speaker control
stream; desktop output approximates the physical speaker's square wave. The
original player also skips the absent P11 song reference. P11's native tape
interaction plays the original bubblegum recording. The fabricated global
circle-animation intermission has been removed.
Original dialogue polygons and multiline text are rendered with the selected
voice alternative. Speech-wait frames wait for playback, with a reading delay
when muted. Repeating effects restart after playback finishes and stop when the
animation is skipped or replaced. Cycle-exact DOS scheduling remains unverified.

See [AUDIT.md](AUDIT.md) for findings, evidence, validation limits, and next steps.
The [completion audit](COMPLETION_AUDIT.md) maps the full request to current
acceptance evidence and documented limits. The current suite passes 112 tests
in debug and release (with 1 `#[ignore]` benchmark); all 5,586 selected native
VGA/EGA scene framebuffers match.

## License & Assets

- **Engine source code**: Licensed under the [MIT License](LICENSE).
- **Original game assets**: No copyrighted 1991 DOS game files or artwork
  screenshots are included in this repository. To run the game or the
  asset-backed test suite, place your own legally owned copy of the DOS
  game files (`MJP.NFT`, `MJP.TBX`, `MJP.TTL`, `MJP.CRD`, `MJP.PRE`,
  `P01..P15.*`, `PROG.ESS`, `DATA.ESS`, `TITLE.ISS`, `SOUND0..3.ISS`,
  `SFX-*.NNV`, `BUBGUM.NNV`) in the working directory.
- **Trademarks**: *Mickey's Jigsaw Puzzles* and related characters are
  copyrights and trademarks of their respective owners. This project is an
  independent clean-room recreation and is not affiliated with or endorsed by
  Walt Disney Computer Software or Disney.
