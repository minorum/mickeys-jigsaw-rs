# Scene viewport and centered presentation

Original executable SHA-256:
`565a5c5601ede858cfdc289e6dd1f3bbde7c2ec2399e52dad2dce3d1b165d465`.
`FUN_1b43_0bdd` copies a state-specific viewport, draws the frame, then adds two
to x/y and removes three/four from width/height. The initial picture rectangle
is (0,0,273,200); its interior is (2,2,270,196), matching the original piece
capture origin and area. State 3 uses the same interior shifted right 32 pixels;
state 5 retains the full physical screen. States 2 and 6 use the picture interior.

`oracles/scene_viewport.py` executes the original setup instructions for states
2,3,5,6 and captures graphics command 0306 rectangles. UI rendering, borders and
low-level graphics dispatch are intercepted; rectangle selection/copy/insetting
execute normally. `scene-viewport.tsv` records all four results.

```sh
python3 validation/oracles/scene_viewport.py /path/to/unpacked.exe /tmp/viewports.tsv
cmp validation/scene-viewport.tsv /tmp/viewports.tsv
```

Rust formerly rendered the whole 320x200 backing scene, exposing unused solid
background at the right (notably P11/P15). Its scene background clear also ignored
the active clip. The retained regression fails at (0,0), outside a requested clip.
Background drawing now respects that clip. Scene presentation renders in native
coordinates with the original interior, then copies those exact pixels into a
centered frame. The presentation offset is 23 pixels, a modernization choice,
not a claim of reproducing original DOS UI placement. Rendering before copying
preserves EGA screen-anchored pattern phase and original reflection arithmetic.
No art was filled in, rescaled or changed.

Browser previews, animations and idle completed pictures share the view. Solved
clicks subtract the presentation offset; native hotspot tables remain unchanged.
Speech-bubble points and text receive the same offset on temporary copies.
Supplied bubble polygon x extents are 5..268, so they fit after translation.

Regression checks native pixels and matte margins for P11 VGA/EGA, clipped
background clearing, every translated hotspot center, translated dialogue pixels,
and pixel-for-pixel continuity between assembled hidden-tray boards and idle
completed pictures for all 30 scene selections. All 420 complete-game cases remain
enabled. Actual window checks include all 30 mouse completions and translated
P11 tape/P12 Donald interactions. Before/after P11 captures are retained.

This establishes native source framing and consistent modern presentation. It
is not a full original DOS screenshot or hardware raster/timing comparison.
