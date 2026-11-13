# Keyboard-only gameplay

During a puzzle or solved picture, WASD moves a visible logical pointer at
120 pixels/second. Diagonal motion is normalized. Enter invokes the existing
click handler at that pointer; it starts at the first tray slot (20,40). Up/Down
still scroll the tray, Space toggles it, Escape returns to the browser and F4
opens setup. Native solved-picture hotspots, animation skip and menu activation
use the same click path. Mouse press returns to mouse control. The pointer stays
inside 320x200 and survives setup/resume and video changes with its session.

This is a modern keyboard control scheme, not a claim of reproducing original
DOS keyboard navigation. No OS pointer warp, new asset, dependency or unsafe
code was added. Keyboard input cancels an outstanding mouse drag; placement still
requires Enter. Pointer drawing uses palette-aware colors and follows caption
rendering so the pointer remains visible over dialogue.

`keyboard_pointer_can_complete_every_grid_and_yield_to_mouse` completes every
2x2..8x8 grid through keyboard pointer movement/activation, checks animation skip,
menu activation, screen bounds and handoff to mouse input. Existing full-game
matrix retains 420 configurations through the pointer handlers.

An actual XTest key-only window run selects P01 at 2x2, uses Enter and timed WASD
holds to place all four pieces, checks the real completion callback in GDB, skips
the animation with Enter, and activates the top-left menu with the keyboard
pointer. No pointer motion, button event or auto-solve is sent. The held-piece,
completed-picture and browser screenshots, debugger log and check log retain
the evidence. This run uses muted VGA and does not establish all configurations
through physical keyboard input. The existing 30-game mouse matrix is also rerun
against this build to check the original control path.

Reproduce unit coverage:

```sh
cargo test keyboard_pointer_can -- --nocapture
```

For manual play, press Enter to pick the first tray piece, move it with WASD,
and press Enter to try placement. Return a held piece by moving onto the tray
and pressing Enter. Setup includes the new controls in its footer.
