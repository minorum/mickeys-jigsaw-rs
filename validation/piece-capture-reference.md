# Original grid-piece capture geometry

The original executable SHA-256 is
`565a5c5601ede858cfdc289e6dd1f3bbde7c2ec2399e52dad2dce3d1b165d465`.
`FUN_1e59_011c` initializes both grid source coordinates to 2, advances them by
the original width/height table, writes those rectangles into capture headers,
and passes them to graphics command 3f0e. These are source-scene coordinates,
independent of where the modern UI places its board on the physical screen.

The new oracle executes the original cutting routine for all seven grid sizes
with curved-edge masking disabled and captures every original 3f0e argument.
It uses SS=DS as required by the compiler's near-pointer access to stack locals.
Allocators return isolated writable buffers; graphics, source rendering and mask
backup/finalization are intercepted. It does not run a native framebuffer or
claim curved-mask byte parity. The original width/height lookup, rectangle copy,
coordinate advancement and control flow execute normally. The resulting 203
base rectangles are in `piece-capture.tsv`.

Two confirmed mismatches:

- Rust sampled the scene at (25,2) instead of (2,2), shifting all piece artwork
  by 23 pixels and dropping the left strip of the intended image. Display origin
  x=25 had been incorrectly reused as a source-image coordinate.
- The first four 8x8 row heights were 24,25,24,25 instead of 25,24,25,24. The full
  original row sequence is 25,24,25,24,25,24,25,24. Column widths and other grids
  match the captured base rectangles.

The regression failed on the first source pixel (old 16 versus expected 19).
It now checks all 203 native rectangles and source pixels for straight pieces.
The existing 420-configuration coverage test now compares every piece's pixels
with the independently established source origin (2,2), rather than the old
self-consistent (25,2) assumption. All grid/edge/video combinations still cover
exactly the intended 270x196 image. No original asset was modified.

Reproduce:

```sh
python3 validation/oracles/piece_capture.py /path/to/unpacked.exe /tmp/captures.tsv
cmp validation/piece-capture.tsv /tmp/captures.tsv
cargo test grid_pieces_match_original
```

Native static data also contains a 273x200 scene viewport and the UI routine
shrinks that viewport by border offsets to a 270x196 interior. This supports the
source origin; it does not alone prove completed-picture or browser composition
on the physical screen. The right-edge presentation is now repaired using this source framing; see
`scene-viewport-reference.md`. Full physical DOS UI equivalence remains unproven.

The rebuilt game is checked with the key-only completion and the full 30-game
mouse window matrix. New partial P01 screenshots in both requested modes show
placed artwork before completion, in addition to the existing completion captures.
These are application runtime checks, not original DOS screenshot comparisons.
