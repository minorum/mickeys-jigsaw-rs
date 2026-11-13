# Real-window complete-game matrix

Run after `cargo build`:

```sh
python3 validation/window_gameplay.py
```

Requires Linux X11 libraries, Xvfb, GDB and Python Pillow. The harness owns an
isolated Xvfb display :93; it refuses to proceed if that display cannot start.
It creates a temporary asset directory containing links to the 89 original
assets and separate canonical preferences with audio muted. It does not write
the original files or the user's local preferences. The game is the normal debug
binary under GDB; no source instrumentation or state injection is used.

XTest sends pointer clicks to pick and place all four pieces in each of the 15
puzzles, first VGA then EGA (including the normal fallback files). Tray targeting
uses the current deterministic shuffle to aim the input; correctness is checked
by the actual game's completion callback, not by assuming those targets worked.
After each group of four placements the harness requires exactly the expected
completion-event sequence in the live debugger log. It sends Escape to return
to the browser and proceeds to the next puzzle. No F2/auto-solve input is sent.
Representative P01/P11/P15 screenshots are captured in both modes.

Result: 30 games completed, 120 pointer-placed pieces. The saved debugger log
contains IDs 1..15 twice in order. `window-gameplay-check.log` records each run.
This checks OS-level input, running game state, rendering, browser navigation and
video switching. It does not prove full DOS pixel parity, audible output, every
OS/display configuration, or all grid sizes through OS input; the separate
in-process matrix covers all 420 configurations.

## Completion feedback defect found

The initial screenshots showed the last pickup caption over the completion
animation. The new regression failed with "stale pickup caption after P01".
`on_puzzle_completed` now stops outgoing audio/common feedback before starting
the native completion sequence. Its own scene audio/captions remain available.
The all-configuration test checks that common captions are cleared for every
completed game. `completion-feedback-before.png` and its failing test log retain
the baseline. The final window matrix reruns the entire 30-game sequence.
