# ISS player reconstruction

The Rust player now reproduces the original PC-speaker music control stream for
all **25 supplied songs**. `iss-oracle.tsv` records the tick counts and FNV-1a
hashes over every tick's speaker-gate byte and little-endian 32-bit PIT divisor.
The Rust regression test independently computes the same complete traces from
the original `.ISS` files.

Original-code identity and unpacking are shared with `nnv-oracle.md`. The unpacked
executable's SHA-256 is
`565a5c5601ede858cfdc289e6dd1f3bbde7c2ec2399e52dad2dce3d1b165d465`.
The diagnostic oracle verifies that identity before execution.

The old notes called `175e` a code segment; it is the music data segment. MZ
relocations loading DS reveal code in original segment `0239`:

- `0239:1aaa` (linear source offset `0x3e3a`): start song.
- `0239:1ad9` (`0x3e69`): music timer interrupt.
- `0239:1cda` (`0x406a`): install timer and initialize multiplexing.
- `0239:1d68` (`0x40f8`): stop playback.

Ghidra 12.2 decompiled `FUN_1239_1aaa`, `FUN_1239_1cda`, and the selected-song
loader `FUN_22a0_0454`. The interrupt has no discovered function body in that
analysis; Capstone inspected its actual instruction bytes, and Unicorn 2.1.4
executes them in the oracle.

`oracles/iss.py` loads the complete MZ image at segment `0x1000`, applies original
relocations, copies the selected song into DS `0x275e`, and runs start/timer code.
Port reads/writes are emulated, never sent to hardware. Each tick records the
actual writes' resulting PIT channel-2 divisor and speaker gate. The periodic
chain to the old BIOS timer is skipped at its call boundary; its pushed flags
are discarded, preserving the sequencer's own timer and voice state. The oracle
does not emulate the complete DOS game or analog speaker output.

Reproduction:

```sh
python3 validation/oracles/iss.py /path/to/unpacked.exe . /tmp/iss-oracle.tsv
cmp /tmp/iss-oracle.tsv validation/iss-oracle.tsv
cargo test iss_matches_original_speaker_control
```

The saved oracle was rerun: exit 0, `25 songs`; comparison returned 0. Every Rust
trace matches. The title lasts 9,217 timer ticks; SOUND0 song 0 lasts 26 ticks.
At 22,050 Hz these produce 758,309 and 2,139 PCM samples. The prior SOUND0 decoder
incorrectly produced 12,136 samples.

Repairs derived from the actual code:

- Read the event offset as a word, repeat count from byte 2, and instrument
  pointer table from byte 3. Byte 1 is not a transpose setting.
- Parse eight-byte event rows, three speaker voices, `0xfd` tempo words,
  `0xfe` repeats/end, `0x58` rests, and `0xf0` holds.
- Reproduce eight-tick voice multiplexing, signed clock comparisons, and the
  original instrument delay/vibrato byte arithmetic.
- Interpret the original note table as note values, not PIT divisors. The actual
  PIT divisor is integer `0x510000 / note_value`; the old implementation inverted
  the pitch relationship.
- Use the real timer ratio `1,193,182 / 4,452` rather than rounding each tick's
  sample count separately.

Desktop rendering converts this verified control stream to a square wave with
continuous phase and the existing application gain. Electrical PIT phase,
analog speaker response, and other historical sound-card drivers are not
reproduced or verified by the control-stream comparison. Malformed songs are
rejected; the existing one-minute application limit is retained.

The original selected-song loader also checks the direct zero-based index against
the resource count before copying at most `0x960` bytes into music memory. P11's
SOUND3 index 2 is absent from the supplied two-resource file; the original skips
it, just as the Rust player now does with an additional diagnostic.
