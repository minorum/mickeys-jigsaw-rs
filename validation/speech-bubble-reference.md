# Native speech-bubble reference

Read-only Ghidra 12.2 inspection of the unpacked original executable,
SHA-256 `565a5c5601ede858cfdc289e6dd1f3bbde7c2ec2399e52dad2dce3d1b165d465`.
No original asset or application source was changed by this investigation.

## Tail layout and selection

`FUN_22a0_061c` selects one sound descriptor and one corresponding bubble.
Descriptor 0 is music; descriptors 1 through N-1 are alternative voice slots.
There is one bubble-tail slot per voice slot, including absent bubbles. This
relationship was independently checked across every readable supplied scene
resource: all tail slots consume exactly the tail and equal descriptor_count-1.

Each slot starts with `u16 point_count`. Zero means an absent bubble and consumes
exactly two bytes. Otherwise it is followed by `point_count` signed `(x,y)`
word pairs, seven words, and the byte string whose length is word 6:

| After vertices | Native use in `FUN_22a0_0510` |
| --- | --- |
| words 0,1,2 | Initially copied into polygon parameters, then overwritten |
| word 3 | Not read by this renderer |
| words 4,5 | Text x,y |
| word 6 | Text byte length |

To select voice slot j, the original skips j complete tail slots, retaining
two-byte zero placeholders. P02 `MICK-02` has a blank music descriptor, two
alternative voices, and two concatenated bubble slots. It selects one voice
and the matching bubble, rather than playing both voices simultaneously.
An absent selected NNV bank prevents bubble activation. Text setting `DS:dc4`
must also be enabled before `DS:98a` is set and the chosen pointer stored at
`DS:98c/98e`. Thus empty music does not imply no voice/bubble.

Descriptor disk words at offsets +10/+12 are zero placeholders for the loaded
bank's far pointer. The original preload path fills them with offset/segment;
they are not additional per-sound behavior flags.

## Voice choice

Let V=N-1, with voices indexed from zero after the music slot. The mode is the
low seven bits of the sound-record word at raw offset 26. Bit 7 separately
sets `DS:85c`, affecting audio stop/wait behavior. Exact selection:

```text
choice = 0
coin_count = V - 1
if mode == 1:
    coin_count = 0
    mutate stored mode byte to 2
else if mode != 0:
    choice = 1
    coin_count = V - 2
repeat coin_count times:
    if original_rng() & 1:
        choice += 1
```

This coin walk is binomial, not a uniform random pick. Mode 1 forces the first
voice once; subsequent activations choose among later voices. It is runtime
state, not a parser transformation. Voice choice occurs when the first music
bank pointer is null or `DS:864` requests the voice path, and a voice slot exists.
Otherwise the first descriptor is selected as music. The original loop assumes
valid counts/modes; malformed combinations need bounds checks in a recreation.

Actual readable resources contain 164 mode-0 entries, five mode-80 entries,
and two mode-1 entries. Mode 80 occurs in P02 `ROBOT` (both video modes), P06
VGA `SKATE`, and P13 `HORSWALK` (both modes). Mode 1 occurs in P11 `THINK-11`
(both modes), with three descriptors. Therefore bit 7 is an exercised behavior.
`FUN_21bf_0081` treats `85c==1` by preserving the entry index, stopping current
audio via `04e2`, starting the prepared sound via `07f1`, setting `85c=2`, and
restoring the entry index. With `85c==2`, it waits for audio inactivity and then
can call `07f1` again. This supports repeated-effect behavior, but a full timed
replay remains necessary before claiming exact restart/cancellation semantics.

## Rendering

`FUN_22a0_0510` supplies the chosen vertices to polygon operation `3006` through
the structure at `DS:641e`. It overwrites polygon color slots `642a/642c` with
`DS:d0c` (logical color 15) and slot `642e` with `DS:cee` (logical color 0).
The stored four words such as `[24,24,248,248]` are therefore not authoritative
runtime colors. Which low-level polygon parameter is fill versus outline has
not been independently traced through the video driver; these assignments are
the observed facts.

The renderer then calls `FUN_196c_023a(text,len,x,y,logical_color[0])`. Assembly confirms
font 0, per-glyph font advance, and font-space advance. Any non-printable byte
other than space resets x to the original text x and advances y by the font's
height (`DS:13f2`); the supplied strings use CR line separators. No synthetic
font, shadow, padding, or inferred wrapping is needed. Neither this renderer
nor the text routine adds a scene/playfield x offset to the stored coordinates.
The final video driver's viewport behavior was not independently replayed.

## Timing

`FUN_196c_17a3` collects the frame sound command into `DS:90e`, the raw delay into
`DS:916`, and `(raw_delay >> 6) & 1023` into `DS:980`. In `FUN_21bf_01ed`, a
nonzero explicit delay becomes that value plus one; zero selects sequence
default `DS:982`. Timer 999 is a speech-wait sentinel, not 999 ordinary ticks.
Most supplied delay commands encode 998, which becomes this sentinel.

The frame loop waits for countdown `DS:984`, then, when speech-wait flag `97e`
is set, also waits while `DS:9b2` or `FUN_2363_016e()` indicates audio activity.
`FUN_21bf_0081` starts/stops queued sounds through `FUN_22a0_07f1`; that function
dispatches selected music or NNV according to `DS:6402`. Following sentinel
handling, when speech is inactive but text display is enabled and a bubble is
active, the original chooses 20 timer ticks; otherwise it uses sequence default
982. It clears bubble-active flag 98a after this sentinel handling.

After rendering a frame, `FUN_21bf_01ed` calls `FUN_22a0_061c` only for a nonzero
sound command, then `FUN_22a0_0510` whenever bubble flag 98a remains set. Thus
bubble state can persist across frames without another sound command. Audio
queueing and starting are separate steps in this state machine. Replacing
encoded 998 with a generic 1.5-second clamp cannot reproduce this behavior;
likewise, summing every alternative descriptor's duration is incorrect.

This is static native control-flow evidence, not a full DOS timing replay.
The exact wall-clock tick calibration and cancellation/transition interactions
still require integration verification. The raw tail walk finds 52 nonempty
VGA bubbles and 41 in the readable EGA sources; source P06 EGA is malformed as
documented separately in `scene-resource-inventory.md`.

## Continuation: color mapping, countdown and repeated effects

Fresh inspection of `FUN_1cd2_0631` resolves the logical-color mapping: VGA mode
`13` copies the 16 words at main DS `d2e` into `cee`; EGA modes `9/d` copy `d4e`.
The startup code sets main DS from relocated segment `180d`, distinct from the
music data segment. In this exact image the tables are:

```text
VGA: 16 24 40 56 72 88 104 120 136 152 168 184 200 216 232 248
EGA:  1  5 10  2  7 12  13  14  14   3   8   6  11   4   9  15
```

Thus bubble fill/text pixel indices are VGA `248/16`, EGA `15/1`. The prior
Rust `15/0` values bypassed this mapping, producing dark VGA fill and magenta
EGA ink. Regression tests now check both video modes; actual window captures
show the correction. VGA fallback scenes use the VGA mapping even when EGA is
selected in setup.

`FUN_17ac_14b1` registers countdown pointer `984` through `FUN_1239_12c8`.
Assembly shows DOS get/set-vector operations for interrupt 8, installing
`1239:1312`. That handler decrements the two pointed words once per interrupt,
stops at zero, and chains the prior handler. The application uses the nominal
BIOS PIT tick duration `65536 / 1193182` seconds. This is not a complete timed
DOS replay with every sound driver installed.

`oracles/scene_audio.py` executes original `FUN_21bf_0081` bytes with only its
device boundaries intercepted. It verifies initial stop/play, no restart while
either speaker or sampled audio is busy, restart after completion, and ordinary
replacement clearing the loop state. It also executes the original countdown
handler and checks `3 -> 2 -> 1 -> 0 -> 0`, and verifies the color-table bytes.
The executable hash is checked before loading/relocating its image.

```sh
python3 validation/oracles/scene_audio.py /path/to/unpacked.exe
```

Rust now retains the selected looping descriptor across frames, restarts it at
a frame boundary only after playback finishes, and clears it on replacement,
skip, or animation completion. The wait check runs before restart, allowing a
speech-wait frame to advance when a repeated phrase finishes. Source regressions
exercise all five mode-80 entries; an idle Rodio queue checks busy/wait behavior
without opening a hardware device. `FUN_1e59_0ed3` independently confirms that
leaving the original animation mode calls `FUN_22a0_04e2` and clears queued audio.

The real P02 window run reached repeated-effect restart calls on successive
frames and captured non-silent PCM through private ALSA output. This verifies
desktop playback and continued frame progression, not cycle-exact DOS scheduling
or audible quality. See `repeated-effect-runtime-check.log` and its JSON report.
