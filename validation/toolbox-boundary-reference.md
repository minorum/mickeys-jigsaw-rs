# Toolbox boundaries and palette sizes

Toolbox chunks formerly shortened oversized declarations to available input,
stopped at malformed sprite records, ignored short trailing headers and returned
whatever sprites had already parsed. These paths now report errors. Empty
resources are rejected; well-formed unknown chunks remain skippable.

Palette validation uses sprite depth. The original decoder produces three
MJP.TBX variants with these properties:

| Variant | Sprites | Depth | PALE bytes | Minimum RGB entries |
| --- | ---: | ---: | ---: | ---: |
| VGA | 65 | 8 | 768 | 256 |
| EGA | 62 | 4 | 54 | 16 |
| CGA | 62 | 2 | 12 | 4 |

A fixed 48-byte minimum would wrongly reject the supplied CGA resource. A new
all-variant check caught this during development. Parsing now defers palette
interpretation until sprite depth is known, reads the appropriate RGB entries,
and rejects insufficient payload. Complete 48-byte 16-color palettes also decode;
the former parser silently left them black. CGA's four supplied entries now load
instead of being ignored. This adds asset compatibility, not a new CGA video mode.

`oracles/toolbox.py` runs the existing original-machine-code DA oracle, inventories
all three resources and hashes every decoded chunk. The original output is 70925
bytes, exactly its declared size; SHA-256 is
`75bb2045a66df702f5b112370a4b563fe5ff0f81eb84e6af2f8031238a37f5bd`.
The original decoder makes one end-of-input lookahead and closes with status 0;
its full diagnostic is retained. `toolbox-oracle.tsv` contains independent FNV-1a
hashes and boundaries. Rust decompression must match all three hashes, and the
loader checks all original sprite counts and depths.

```sh
python3 validation/oracles/toolbox.py /path/to/unpacked.exe MJP.TBX /tmp/toolbox.tsv
cmp validation/toolbox-oracle.tsv /tmp/toolbox.tsv
cargo test toolbox_
cargo test test_load_toolbox
```

Malformed fixtures cover oversized outer chunks, invalid/oversized records,
trailing chunk/record headers, undersized 16-color palettes and empty resources.
Before logs retain both acceptance of a truncated declaration and failure to
read the complete 48-byte palette. Full valid-game and window checks remain
enabled. These checks are not exhaustive fuzzing or physical CGA rendering proof.
No original source asset was modified.
