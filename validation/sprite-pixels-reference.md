# Native sprite transparency

The VGA driver reads its transparency key from the high byte of the sprite
depth word (record offset 17). Rust previously used offset 15, which is zero
throughout the supplied scene corpus. P07 sprite 38 and P10 sprite 37 use key 2;
zero-valued art was discarded while key-2 background pixels were drawn and
counted as hits. P10 sprite 24 declares key 246, although its stored pixels are
all color 39, so correcting that metadata does not change its appearance.

`oracles/sprite_pixels.py` runs the original unpacked VGA driver's opcode 3b
handler at 1000:1fa5 under Unicorn. It installs each original sprite's metadata
and raw pixels in emulated memory, supplies source coordinates, and reads the
signed returned pixel: -1 for transparency, otherwise the palette index. The
driver SHA-256 is checked before execution. No Rust decoder generates the
expected result.

The oracle reads the first occurrence of every stored color in every sprite:
885 VGA sprites, 16,587 native reads across all 15 puzzles. The fixture hashes
each sampled source position and signed result. Rust independently loads the
source assets and reproduces these hashes using its pixel and opacity APIs.
The retained before log fails on P07 sprite 38. A separate regression checks
every pixel of the three exceptional sprites through blitting and scene hit
testing, including all four reflection combinations.

Reproduce with the unpacked driver and original decoded scene corpus:

```
python3 validation/oracles/sprite_pixels.py /tmp/mickey-vga-unpacked/output.000.exe /tmp/preset-native-corpus validation/sprite-pixels.tsv
cargo test vga_sprite_pixels_match_original_driver
cargo test nonzero_sprite_keys_preserve_rendering_and_hit_testing
```

This proves native VGA source color/opacity semantics for all stored colors,
plus Rust rendering/hit integration for the exceptional keys. It does not
establish complete native blit clipping, EGA conversion, or whole-frame parity.
