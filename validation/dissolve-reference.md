# P05 projector dissolve

Four supplied delay commands have nonzero presentation modes: P05 VGA frames
184/185 and EGA frames 182/183. Their raw words are 584 and 8. The high bits
retain the existing frame delays; low six bits select presentation mode 8.
These frames belong to `F_SCREEN`, reached through FILM's projector hotspot.
The earlier renderer ignored the low bits and cut directly to each picture.

Original scheduler `FUN_21bf_0081` passes DS:918 to graphics command `0e04`.
The VGA driver dispatches that command at 1000:036f, and EGA at 1000:03f0.
Both select entry 8 of the table at CS:00f0. This is a pixel dissolve:

| Driver | Routine | Native order |
| --- | --- | --- |
| VGA | 1000:063e | 17-bit shift register, feedback 12000; reject x>=320 or y>=200 |
| EGA | 1000:07fb | 15-bit shift register, feedback 6000; coordinates in a 160x100 quadrant, copied into four quadrants |

Both start with state 1 and copy position zero last. They visit all 64,000
pixel positions exactly once. Their orders differ; using one order for both
video variants would lose original behavior.

## Independent driver execution

`oracles/dissolve_order.py` executes both relocated original routines. VGA's
actual `MOVSB` destination writes provide its order. EGA's native coordinate
generator runs unchanged; its planar copy leaf at 1000:0893 is intercepted
and projected into its four indexed-pixel positions. This establishes indexed
pixel order, not intermediate EGA plane colors or cycle-exact hardware timing.
The final cursor/dirty-buffer maintenance is outside the captured transition.

Source `VGA.OVL` SHA-256:
`666e2a4b1a1f404459199644740f9207120ed74f31cc3860fa7176977f364c1f`.
Source `EGA.OVL` SHA-256:
`55e7bf3d512b34e943cde8b5eb19f592bc524d59a5dd9ce6266a088270207104`.
The oracle checks unpacked executable hashes before execution.

```sh
mkdir -p /tmp/mjp-vga /tmp/mjp-ega
deark -m exepack -od /tmp/mjp-vga VGA.OVL
deark -m exepack -od /tmp/mjp-ega EGA.OVL
python3 validation/oracles/dissolve_order.py /tmp/mjp-vga/output.000.exe /tmp/mjp-ega/output.000.exe /tmp/dissolve-order.tsv
cmp validation/dissolve-order.tsv /tmp/dissolve-order.tsv
cargo test dissolve
```

`dissolve-order.tsv` retains complete-order FNV64 hashes plus first/last
positions. Rust verifies these, uniqueness and full coverage for both orders.
The actual P05 regression checks the old picture at entry, each midpoint pixel,
the complete target, the second frame's previous picture, and cancellation.

## Desktop presentation and live validation

The compositor copies original scene pixels in the verified order, preserving
the native viewport and palette. Rendering the two scenes before mixing also
preserves EGA pattern phase. Playback uses its existing elapsed-frame clock,
so setup pauses and input remain responsive. The original routines are
CPU-bound and specify no wall-clock duration. Desktop presentation deliberately
caps the dissolve at four DOS ticks (about 220 ms) and half the frame interval.
The remaining interval displays the complete target, including the final short
frame. It preserves each frame's total delay and avoids a blocking copy loop.
This duration is a modernization choice, not original timing evidence.

```sh
cargo build
python3 validation/dissolve_gameplay.py
```

The isolated Xvfb/GDB/XTest harness uses F2 to establish the completed picture,
then clicks the projector in each video mode. Captures prove that midpoint
pixels all belong to the old or new source picture, with both represented.
It checks both frame targets, natural completion and skip back to idle. Timers and captions
are disabled for pixel comparison. Image analysis runs after the time-critical
skip, so the short sequence cannot finish naturally before that input.
This is animation evidence; separate window matrices cover piece placement.

The rebuilt live run passed in both variants, including complete presentation
of the final short frame. `dissolve-window.log` retains its pixel counts;
`dissolve-gameplay-debug.log` retains the render-progress trace. The complete
suite reports 84 passing tests in `dissolve-tests.log`; strict Clippy passes.
The rebuilt grid matrix completed all 30 games with 120 mouse placements,
without auto-solve (`dissolve-grid-window.log`). All 89 source asset hashes match.
