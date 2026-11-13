# Native animation repetitions and speed

The loader previously retained the selected sequence index but discarded the
next two words in sequence commands. `0010` defaults are 12-byte commands;
`0011` hotspots are 22 bytes. Both store sequence index at byte 4, repetitions
at byte 6, and default frame delay at byte 8.

Original `FUN_1e59_1de3` installs those settings into DS:978 and DS:982.
`FUN_21bf_01ed` decrements a nonzero repetition counter at the last frame,
requests completion when it reaches zero, and otherwise returns to frame 1.
Zero also requests completion after the first pass. The recreation previously
played every sequence once, at a fixed two ticks per frame.

Supplied examples include P02 `F_ROBOT` (three passes), P06 `F_FLAG` (six),
and P11 `F_PHONE` (eleven ticks per ordinary frame). An explicit delay command
still overrides the default with encoded ticks plus one; zero selects the
default. Speech-wait handling retains its separate audio/text behavior.

## Independent fixture

`oracles/sequence_playback.py` executes the relocated original scheduler.
It substitutes only frame rendering, audio/input work and timer display,
supplying three frames with an optional explicit delay on frame 2. The native
counter, wrap, completion event and delay-selection instructions execute
without interception. Its executable SHA-256 is checked before running:
`565a5c5601ede858cfdc289e6dd1f3bbde7c2ec2399e52dad2dce3d1b165d465`.

`sequence-playback.tsv` contains 48 traces: counts 0/1/2/3/4/6, default delays
2/3/4/11, and explicit delays 0/9. Rust checks every displayed frame and delay,
including termination. Other regressions check parser payload bounds, the
actual P02 robot, elapsed remainder across a wrap, and all 134 active FILM hotspot
settings. The common playback descriptor carries index, repetitions and delay
together from parsing through dispatch.

```sh
python3 validation/oracles/sequence_playback.py /path/to/unpacked.exe /tmp/sequence-playback.tsv
cmp validation/sequence-playback.tsv /tmp/sequence-playback.tsv
cargo test sequence_
```

## Live checks

```sh
cargo build
python3 validation/sequence_gameplay.py
```

The isolated Xvfb/GDB/XTest harness clicks P02's robot and P11's phone in both
video modes after F2 establishes the completed picture. Piece-placement proof
belongs to the separate `window_gameplay.py` matrix. This harness checks the
complete frame trace, individual frame intervals,
natural completion followed by a successful restart, and keyboard skip.
The robot displays 24 frames; the phone displays six, including its existing
muted 20-tick speech wait. Output audio is disabled. Desktop wall-clock checks
allow scheduler/rendering tolerance; this is not cycle-exact DOS timing or
physical speaker verification. Original source assets remain unchanged.

The focused window trace is retained in `sequence-playback-window.log` and the
GDB frame stream in `sequence-gameplay-debug.log`. The full suite reports
82 passing tests in `sequence-playback-tests.log`; native fixture reproduction
and strict Clippy results are retained beside it.
