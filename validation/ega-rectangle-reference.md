# EGA patterned rectangles

The supplied P11 EGA scene has rectangle color 090e in frame 168 (tape animation).
All supplied VGA rectangle words repeat their color byte; this EGA rectangle
contains different bytes. Rust formerly discarded the high byte and filled the
rectangle with color 14. The actual scene frame differs visibly when flattened.

The original `FUN_196c_107a` passes both color bytes to the graphics driver.
`EGA.OVL` SHA-256 is
`55e7bf3d512b34e943cde8b5eb19f592bc524d59a5dd9ce6266a088270207104`.
Deark EXEPACK unpacking produces SHA-256
`6ec038bddd09d17aa331f2f869005d9ed4e2057495adc08628ff7007576674f0`:

```sh
deark -m exepack -od /tmp/ega-unpacked EGA.OVL
python3 validation/oracles/ega_rectangle.py /tmp/ega-unpacked/output.000.exe /tmp/ega.tsv
cmp validation/ega-rectangle.tsv /tmp/ega.tsv
```

The original EGA routine at 1000:1f1d sets an aaaa/5555 alternating bit mask
according to screen y parity and emits masked writes for the two color bytes.
`oracles/ega_rectangle.py` executes those original instructions, captures graphics
bit-mask port writes and framebuffer writes, and projects each masked low-nibble
color write into logical pixels. The projection is a limited write-mode model,
not a full EGA hardware emulator. It produces 36 colored pixels from 16 masked
byte writes for a 9x4 rectangle at (3,5). Even x+y uses color 14; odd uses color 9.
Untouched pixels retain sentinel 255. The fixture is `ega-rectangle.tsv`.

The Rust regression first failed at (4,5): actual 14 versus original 9. EGA
rectangle rendering now retains both bytes and screen-anchored parity. Uniform
colors use the existing fill path. Clipping preserves parity. Mode-2 recoloring
preserves a zero pattern byte, following the original wrapper. VGA scenes retain
the existing solid path, including assets used as EGA fallbacks.

Tests cover the original-driver pixel fixture, clipped drawing, corpus inventory
and a visible difference in the actual supplied P11 frame. The original run
completed animations for 139 aggregate descriptors; current solved interaction
coverage checks 134 active FILM descriptors (see the scene resource inventory).
An actual window run selects EGA, mutes audio, solves
P11, skips completion and clicks the tape; its screenshot and log are included.
The runtime screenshot alone does not identify transient frame 168 or establish
full native sprite raster parity.
