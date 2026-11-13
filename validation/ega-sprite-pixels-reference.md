# Original EGA sprite conversion and pixel reads

The original EGA path converts packed four-bit sprite data into compressed
per-color masks. Rust renders the packed data directly to its indexed
framebuffer. This comparison checks that both paths produce the same source
pixels and transparency, without implementing a substitute native decoder.

`oracles/ega_sprite_pixels.py` runs two isolated Unicorn machines. The first
executes the original main program's packed conversion at 1239:0ccd, using
the supplied sprite header/data and a separate destination buffer. It checks
the converted stride, height, color-group count and buffer size. The second
installs that actual converted resource into the original EGA driver and
executes its pixel-read opcode at 1000:2e1e for every original source pixel.
No converter or pixel-read instruction is intercepted or replaced.

Both binaries are SHA-256 checked before MZ relocation/loading:

- Main: `565a5c5601ede858cfdc289e6dd1f3bbde7c2ec2399e52dad2dce3d1b165d465`
- EGA: `6ec038bddd09d17aa331f2f869005d9ed4e2057495adc08628ff7007576674f0`

Coverage is all 722 sprites in the 12 intact supplied PZ4 scenes: 749,756
native reads. Every returned palette index agrees with the packed source;
zero pixels return -1 for transparency. The fixture hashes every signed
pixel result in source order. Rust independently loads the original assets
and verifies every decoded pixel/opacity result against those hashes.
All pixels match; no additional production change was needed.

```
python3 validation/oracles/ega_sprite_pixels.py /tmp/mickey-original-unpacked/final/output.000.exe /tmp/mickey-ega-unpacked/output.000.exe /tmp/preset-native-corpus validation/ega-sprite-pixels.tsv
cargo test ega_sprite_pixels_match_original_conversion_and_driver
```

The packed source comes from the existing native-decoded scene corpus. The
malformed P06 PZ4 and absent P07/P10 PZ4 remain excluded; their documented
VGA fallbacks are unchanged. No original asset is modified or supplemented.

Limits: this checks native resource conversion and source pixel/hit opacity,
not EGA video-memory planar writes or every recursively rendered scene frame.
Converted stride padding beyond the original source stride is not sampled;
the original visible crop stays within the source stride.
