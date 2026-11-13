# NNV reconstruction and original-code oracle

The Rust decoder now matches sample counts and complete-output FNV-1a hashes for
all **85 phrases in 40 banks** (29 standalone NNV files and 11 ESS resources).
`nnv-oracle.tsv` records each result. The regression test consumes this manifest
and decodes the actual assets; it does not reuse the oracle implementation.

Original `MICKEY.EXE` SHA-256:
`3c6b8054b5cd67ade96e8757b698193805a8859729a4908dedff99df511568bb`.
[Deark](https://github.com/jsummers/deark) commit
`add9602d6cf77a189fc2762ed13077431dd7016e` removes PKLITE and then
EXEPACK layers into a separate temporary artifact, SHA-256:
`565a5c5601ede858cfdc289e6dd1f3bbde7c2ec2399e52dad2dce3d1b165d465`.
No original file was modified; no unpacked executable is committed.

Ghidra 12.2 inspected the original functions. The sample decoder is hand-written
assembly with dynamically resumed execution, so Capstone and Unicorn 2.1.4 were
used to inspect and execute its actual bytes. `oracles/nnv.py` checks the unpacked
artifact hash before running the routine at original relative `1363:0db4`.

The oracle supplies one channel state, a bank, an identity volume table, and an
output buffer. It executes original command dispatch, frame decoding, all three
lookup tables, fractional repetitions, stepped packets, pitch contours, and
continuation across 512-byte output buffers. It stops at the original end-of-
phrase path before padding the final buffer with silence. It does not emulate
the full game, BIOS, audio device, or final two-channel mixer.

Reproduce the independently generated manifest:

```sh
python3 validation/oracles/nnv.py /path/to/unpacked.exe . /tmp/nnv-oracle.tsv
cmp /tmp/nnv-oracle.tsv validation/nnv-oracle.tsv
cargo test nnv_matches_original_machine_code
```

The saved script was rerun: exit 0, `40 banks, 85 phrases`; `cmp` returned 0.
The regression test passes for every manifest entry. The old Bubblegum decoder
returned zero samples; the original and repaired decoder produce 108,517.

Root causes corrected:

- Sample mode belongs to the command, not the first byte of a frame.
- Packet commands occupy four or five bytes; silence occupies two.
- Frame addresses use 19 direct address bits.
- Packed scales are refreshed per block: 20 samples for 3/5-bit modes and
  19 for 4-bit mode. The 5-bit header splits its scale across two bytes.
- Repetition counts use complement encoding and fractional carry; stepped frames
  advance by the original packed block widths.
- Pitch commands update byte-sized periods and read contour segments as the
  original does; they do not synthesize invented replacement waveforms.

The Rust implementation rejects truncated input and limits phrase output to two
minutes. That bound is a defensive application policy, not a claimed original
game limit. Signed playback PCM retains the application's existing 0.75 gain.

The `0x8023` scene sound parser independently had its count/data offsets two bytes
early. It now parses the source descriptors and checks their sizes. All supplied
scene NNV references decode successfully; one unresolved ISS reference is tracked
separately. The originally ignored audio integration test is enabled and passes.

Runtime validation routed the rebuilt game through an ALSA plug/file backend
with a null slave and a paced FIFO recorder, isolated from host speakers. In a
real Xvfb window, setup → browser → puzzle → auto-solve produced 7.4036 seconds
of 48 kHz stereo S16_LE PCM, with 172,578 nonzero samples and peak 26,878. The game
remained alive after the input sequence. This proves output-path activity, not
an audible assessment or faithful ISS music synthesis.
