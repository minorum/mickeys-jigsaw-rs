# Invalid DA prefix codes

`PrefixDecoder::decode` previously returned symbol zero if neither its fast
table nor its long-code list matched the input. That value was treated as a
valid literal, distance or length symbol. The output-size and EOF checks could
still pass, so malformed compressed input silently produced invented bytes.

A standalone probe returned `Ok([0, 0])` for a stream with unused nine-bit
distance/length codes. The public decoder regression separately reproduced an
invalid literal becoming `Ok([0])`; its failure is in `da-prefix-before.log`.
The repair returns an error from the shared prefix primitive and propagates it
through all three consumers. Valid sparse-tree streams and explicitly encoded
zero data remain supported.

The regression supplies three malformed streams (literal, distance and length)
and two positive controls: a raw literal A and a valid coded zero byte. Tree
sizes, payload sizes and output counts are complete; these cases exercise
invalid code selection rather than accidental truncation.

```sh
cargo test invalid_da_prefix_codes_do_not_create_zero_bytes
cargo test
```

The full suite retains original-corpus decompression, 85 native speech phrase
hashes, 25 native music control streams, toolbox hashes, source palettes,
geometry and gameplay checks. This is regression coverage for the demonstrated
failure, not a complete audit of every DA variant or hostile byte stream.

The rebuilt suite passes all 86 tests (`da-prefix-tests.log`), with formatter,
strict Clippy and build also passing. The current game completes all 30 grid
window games, 120 mouse placements without auto-solve (`da-prefix-window.log`).
The tray-edge recheck completes four more games and 96 placements, with puzzle
and layout confirmed by the debugger (`da-prefix-tray-edge-window.log`).
The original probe now returns `Err("Invalid DA prefix code")`; before/after
logs are retained. All 89 original source asset hashes still match.
