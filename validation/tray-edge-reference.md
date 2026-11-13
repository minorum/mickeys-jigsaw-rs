# Tray bottom-row selection

The tray body has 156 interactive rows, y=30..185. Five visible slots use
integer height 156/5=31, leaving one padding row. The old input calculation
mapped y=185 to slot index 5, selecting the invisible sixth piece instead of
the last visible slot. Preset mode and grids 6..8 use this layout.

The repair bounds the calculated slot to the visible range. The bottom padding
belongs to the last slot. Scroll-button behavior at y=186 remains separate.
The regression covers all eight layout choices, three scroll positions,
bottom rows 184/185, mouse and keyboard activation, returning a piece without
reordering, the adjacent scroll button, and empty lower slots.

The old unit result selected piece 0 instead of visible piece 1 in P01 preset
mode. The old real-window 6x6 run also failed to reach completion. Logs are
`tray-edge-before.log` and `tray-edge-window-before.log`.

```sh
cargo test tray_bottom_padding_selects_the_last_visible_piece
cargo build
python3 validation/tray_edge_gameplay.py
```

The isolated Xvfb/GDB/XTest run completes P01's 6x6 and preset games in VGA and
EGA. Each starts by clicking y=185 and placing the fifth visible piece, then
places the remaining pieces normally. All four completion callbacks and 96
mouse placements pass, without auto-solve. Native capture/bounds fixtures
provide independent target positions. `tray-edge-window.log` and the partial
board screenshots retain the result. The complete suite reports 85 passing
tests in `tray-edge-tests.log`; formatter, strict Clippy and build also pass.
