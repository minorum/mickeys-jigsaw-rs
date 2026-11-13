# Supplied scene resource inventory

Inventory covers all 15 supplied VGA files and all 13 supplied EGA files.
P07/P10 EGA files are absent. A standalone Rust utility included the current
`assets.rs` and `decompress.rs`; a separate raw-record walk validated command
counts and boundaries instead of relying on the parser's ignored-command arms.
No original asset changed. No Cargo build ran for this investigation.

## Commands actually present

| Frame command | VGA | EGA |
| --- | ---: | ---: |
| Sound `0020` | 222 | 195 |
| Delay `0030` | 76 | 58 |
| Subframe `0040` | 13,165 | 11,971 |
| Rectangle `0050` | 20 | 9 |
| Sprite `0060` | 959 | 807 |
| Text `0080` | 0 | 0 |
| Polygon `0090` | 0 | 0 |
| Unknown `00a0` | 0 | 1 |

EGA counts include only the readable prefix of malformed P06.PZ4. The other
27 files parse exactly to the end of their main scene chunks. Across those
27 files, every parsed background/subframe target resolves to a frame, every
sprite and sound command index resolves, and every hotspot sequence exists.
No sound-bearing frame is referenced as a nested subframe; no frame contains
multiple nonempty sound commands. Text/polygon frame rendering is therefore
not an exercised gap in these supplied scenes.

The single unknown command is P03.PZ4 frame 189, main-chunk offset 12,412:
`a0 00 06 00 03 00`. Its original meaning remains unverified.

This does not make speech-bubble rendering unused: sound resources contain a
separate polygon/text format in their tails. A bounded walk found 52 bubble
texts in VGA and 41 in the readable EGA data. For example, P01's `OHBOY-01`
contains `Oh, boy!`; `GOSH-01` contains three lines of dialogue. These tails are
were formerly retained as `raw_tail` without rendering. The speech-bubble
continuation now parses and renders them; see `speech-bubble-reference.md`.
The inventory counts come from structure/bounds checks, not a rendering oracle.

## P06 EGA source and repair

P06.PZ4 decompression succeeds but its main scene chunk becomes malformed at
offset `0x3efc` (absolute decompressed offset 16,194). A zero-size record follows
the 22nd parsed sprite, leaving 25,342 bytes unread. The former parser silently
returned 22 sprites and no sounds. Frames reference 28 unavailable sprite
indices and sounds 0/1; the EGA scene was incomplete despite successful loading.

The original-machine-code DA oracle independently produced the same complete
41,536-byte decompressed buffer, consuming all 11,296 input bytes. Its output
SHA-256 is `182012feb1a384db957613d8c434ddd6b8a4c2050646892690665644937fdcf4`.
The original and Rust buffers compare byte for byte. This is malformed supplied
scene data, rather than a discrepancy in the Rust DA decoder.

`assets.rs` now rejects invalid record/table bounds and partial scene tables.
An EGA decode/parse error reports the error and loads the original VGA file,
using the existing missing-EGA fallback route without a P06 special case.
P06 consequently loads all 53 original VGA sprites and both sound resources.

Two standalone module tests passed, with the completed log reread:

```text
Cannot load P06.PZ4: Invalid scene record size at 0x3efc; using original VGA asset
test assets::tests::malformed_ega_scene_loads_original_vga ... ok
test assets::tests::supplied_scene_resources_resolve ... ok
test result: ok. 2 passed; 0 failed; 0 ignored
```

The fallback test compares all selected sprite pixels and palette bytes with
the original VGA asset. The inventory test directly parses every supplied asset,
checks that source P06 EGA fails, and validates the 27 intact resource graphs.
The joint application suite subsequently passed all 32 tests with the real audio
module. `p06-ega-fallback-runtime.png` shows the complete scene after actual
window navigation, selecting EGA, starting P06, and completing it.

## Solved-scene character selection

The former solved click path used record 4's subframe ordinal as a hotspot
ordinal. P01 record 4 has 12 subframes, but only four hotspot descriptors. Its
first five subframes are different pieces of Goofy, making that mapping wrong.
With the pixel hit tester and points outside every hotspot rectangle:

| Click | Current record 4 hit | Current chosen sequence | Record 3 hit |
| --- | --- | --- | --- |
| `(188,24)` | command 2, child 272 | `F_GHOST` | command 1, child 284 |
| `(192,105)` | command 3, child 283 | `F_DONALD` | command 1, child 284 |
| `(202,168)` | command 4, child 280 | `F_MICKEY` | command 1, child 284 |

All three record 3 hits select the complete Goofy actor and correspond to
`F_GOOFY`. P01 record 3's first four children are 284 (`ACT_GOOF`), 249
(`ACT_GH_1`), 270 (`ACT_DD`), and 297 (`ACT_MM`). Its hotspot sequence indices
are 5, 6, 4, 3, in that order. Record 3 flags are 0 for those four groups;
record 4's fragmented shapes use `0x9c20` (one reflected shape uses `0x9c21`).

Original Ghidra 12.2 inspection, unpacked executable SHA-256
`565a5c5601ede858cfdc289e6dd1f3bbde7c2ec2399e52dad2dce3d1b165d465`:

- `FUN_1e59_2857`, event `1100`, selects sequence 1 (`FILM`), first frame,
  through `FUN_196c_06e0`, then calls `FUN_2202_064c`.
- `FUN_196c_05d4` computes the frame from the sequence header plus frame index.
  FILM's header is record 2, so first frame is record 3.
- `FUN_2202_064c` scans the current frame at `c7d0`, tests subframes except
  flag mode `30`, and retains the **last** matching command's 1-based ordinal.
  It does not return the first match. Its selected command ordinal is `13b8`.
- `FUN_1e59_1de3` scans FILM's `0011` hotspot descriptors for that ordinal and
  selects the descriptor's sequence. If no descriptor matches, it uses the
  `0010` default (`F_ALL`). It does not index the shape-puzzle frame's pieces.
- `FUN_1b43_01ee` fills the solved hotspot center table from FILM `0011`, storing
  `x + w/2`, `y + h/2` in descriptor order. `FUN_2202_064c` indexes those centers
  by subframe ordinal independently of its returned command ordinal.

The Rust witnesses establish the incorrect selection; the original static
control flow establishes the correct frame/group relationship. `puzzle.rs` now
uses FILM's first frame, retains the last matching command, checks hotspot
centers as the original does for keyboard selection, and uses FILM's default
for objects without a matching hotspot descriptor. Rectangle-first selection
was removed: the original tests geometry or exact hotspot centers.

Regression cases exercise the three Goofy points above, `(131,178)` where
commands 3 and 4 overlap (last match selects `F_MICKEY`), and `(155,24)` where
only unmapped command 6 is hit (selects `F_ALL`). An additional test checks every
P01 hotspot center. A standalone module harness with a no-output audio stub
passed all four new asset/click tests. This verifies click logic and resource
loading. The subsequent joint Cargo suite passed all 32 tests with the real
audio module. This is not a full original DOS click-session replay.

## Corpus-wide solved interaction validation

`all_solved_hotspot_centers_select_and_complete_their_animation` checks all 15
puzzles with both requested video modes, including the original VGA fallbacks.
All 134 active FILM descriptor centers select their expected sequence and
retain that descriptor's repeat count and default delay. The test dispatches
each actual solved-picture click, checks the started sequence, renders its first
state, advances until completion with a bounded step count, and renders the
resulting state. Audio is disabled, so this also checks that absent playback does
not trap these animations in a sound wait. Speech Text retains the session's
default behavior. The earlier `all-hotspots-check.log` reported 139 aggregate
descriptors: it also included five P05 EGA `F_SCREEN` descriptors, tested against
FILM's active geometry. Their sequence numbers coincided, concealing different
repeat counts. They are retained in their own header, while current solved
interaction checks follow the original selection of sequence 1 (`FILM`).

This is in-process gameplay/renderer evidence, not original DOS pixel parity,
physical mouse testing of every center, or an audible check of every animation.
Existing actual-window evidence covers P01 actor clicks and P11 tape playback.
