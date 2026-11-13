# Nested reflection bounds

Original executable SHA-256:
`565a5c5601ede858cfdc289e6dd1f3bbde7c2ec2399e52dad2dce3d1b165d465`.
The oracle executes relocated x86 `FUN_196c_0aea` and its recursive collector
`FUN_196c_0ba3`, without intercepting the bounds routines. Three injected frames
contain two translated subframes and a 13x17 rectangle. The oracle varies all
four reflection-bit settings at each of the three levels, producing 64 rows.
Screen extents are 320x200 and the outer origin is (13,19).

Rust previously passed child reflection flags when measuring the child bounds
used to adjust a reflected translation. The original explicitly pushes zero
for that measurement's flags: bounds collector at 1a2cb, renderer at 1ac82,
hit tester at 224ba. In each case SUB AX,AX precedes two PUSH AX instructions
before x/y/frame and output-pointer arguments. Reflection is applied later by
XOR when recursing; measuring with the child flags applied it prematurely.

A failing regression is retained in `frame-bounds-before.log`: outer=1,
middle=1, inner=0 gave Rust x=-287 versus original x=311. All three measurement
call sites now use zero flags. `frame-bounds.tsv` contains original results.
The Rust test checks all 64 bounds, rendered rectangle pixels with viewport
clipping, and inclusive/exclusive hit boundaries against those fixture bounds.
This establishes the tested recursive geometry, not full DOS sprite raster or
hotspot parity for every supplied scene.

Reproduce the independent fixture:

```sh
python3 validation/oracles/frame_bounds.py /path/to/unpacked.exe /tmp/bounds.tsv
cmp validation/frame-bounds.tsv /tmp/bounds.tsv
```
