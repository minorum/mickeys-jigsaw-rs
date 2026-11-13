# Complete-game configuration matrix

The gameplay regression covers 15 puzzle IDs, both requested video modes, all
seven grid sizes (2x2 through 8x8), and both straight and curved edges: 420 games.
Absent/malformed EGA files use the application's normal original-VGA fallback.

For each configuration the test constructs the normal PuzzleSession, picks every
piece through pointer-press handling, alternates click-to-carry and drag-release,
and places it at its intended target through the pointer handlers. Click-to-carry
includes a short release retaining the held piece, then a second placement press.
Each placement must mark the correct piece and clear the held slot. Across the
matrix, 12180 pieces are placed. Every game must be solved with an empty tray and
all pieces placed. The test renders the completion frame, clicks to skip its
animation, renders the idle scene, and checks the top-left menu action.

Audio is disabled. Rendering uses the original font and toolbox selected from
the actual scene depth. Expected placement centers are derived from piece grid
geometry; the independent pixel-coverage tests still check the resulting cuts.
This is actual in-process gameplay-state/input-handler evidence, not an OS mouse
run for every configuration, original DOS pixel parity, or audible verification.
Existing window captures separately establish pointer input and live rendering.

Reproduce:

```sh
cargo test click_to_pick_place_complete -- --nocapture
```

`all-game-configurations.log` records the focused run: 420 completed games and
12180 placed pieces. The full suite retains this regression.
