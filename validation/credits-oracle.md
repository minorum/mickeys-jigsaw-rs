# Original DA decoder oracle

The supplied credits stream is inconsistent even when decoded by the original
program's machine code. No decoder or source asset was changed to hide it.

Identity (SHA-256):

| Artifact | Hash |
| --- | --- |
| Original `MICKEY.EXE` | `3c6b8054b5cd67ade96e8757b698193805a8859729a4908dedff99df511568bb` |
| Deark PKLITE then EXEPACK output | `565a5c5601ede858cfdc289e6dd1f3bbde7c2ec2399e52dad2dce3d1b165d465` |
| Original `MJP.CRD` | `bab7b42d5d6ed85ecca8b87faaa1b1464ebf38ee78b9dfa755c61e1748b3fd8e` |
| Original `MJP.TTL` | `844c2805153d183a34b859a82648163d0b6187eb5c5b4b2a1d3a26cc6aebecad` |

`oracles/credits.py` uses Unicorn 2.1.4 to execute the unpacked 16-bit original
decoder, including tree construction, bit consumption, dictionary initialization,
match copying, and sliding-window movement. MZ relocations use load segment
`0x1000`; DS is `0x280d`. Entry is `265f:0258`, relative original segment
`165f:0258`. Initialization reproduces the workspace pointer assignments from
`165f:000c`, with independent scratch buffers at `5000:0000` and `6000:0000`.

Only these boundaries are replaced:

- `265f:01c6`: input byte reader, returns exact file bytes and `0xffff` at EOF.
- `265f:0a76`: output transfer, copies the original decoder's requested bytes.
- `265f:0197`: file close, preserves its error argument and closed state.

Ghidra 12.2 independently decompiled `FUN_265f_0258` and `FUN_265f_0ae9`.
The original uses flag bit 1 to select minimum match length 2 or 3. Its length
extension and distance-plus-one behavior match the Rust implementation.

Reproduction, using the separately unpacked executable (never committed):

```sh
python3 validation/oracles/credits.py /path/to/unpacked.exe MJP.CRD
python3 validation/oracles/credits.py /path/to/unpacked.exe MJP.TTL
```

Both runs exited 0; their completed logs were reread:

```text
MJP.CRD: input=30320/30320 eof_calls=1
output=225614 declared=226018 close_errors=[0]
sha256=e9a7e69fbd2cd0fd0f950919595b6dab966f8b676b4b1985661b49a3cf1825f3
FORM offsets=[26, 64869, 129693, 161797, 193901, 209969]
MJP.TTL: input=41766/41766 eof_calls=1
output=113010 declared=113010 close_errors=[0]
sha256=4e807e5b74befb5dad7d7651d27a4f3cf1322688c55e562b3142e0e6972f934e
FORM offsets=[14, 64838, 96942]
```

The title is a positive control: its original-machine-code output equals the
Rust decoder's complete output byte for byte. The credits output equals the
first 225,614 bytes of the Rust decoder's former zero-padded output byte for byte.
The source credits require 404 more output bytes than the original produces.
All credits FORM headers after the first are 19 bytes past the declared resource
boundaries; the second header also contains wrong `ff` bytes in zero fields.
The original decoder silently returns success after this short EOF result, so
its return code is not integrity evidence. The strict Rust rejection is valid.

This verifies the original decoder routine, not an original DOS game session or
the on-screen appearance of the malformed credits. Recovering pristine credits
requires an independently verified source asset; inventing missing bytes or
scanning for displaced headers would not repair the supplied data faithfully.
