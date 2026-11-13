# Bubblegum belongs to the P11 scene

The earlier architecture document asserted a global "Bubblegum Contest"
intermission after several completed puzzles. That assertion had no reference
evidence. The Rust implementation added a circle, highlight, "POP!", a 4.5-second
timer, an automatic three-puzzle trigger, and a B shortcut.

The supplied P11 VGA and EGA scene data positively identify the original route:

| Item | Original data |
| --- | --- |
| Sound resource | zero-based index 1, tag `BUBGUM` |
| Music alternative | `SOUND3`, song 2 |
| Voice alternative | `BUBGUM`, phrase 0 |
| Bubble tail | zero point count; no caption polygon |
| Animation routes | `F_ALL` and `F_TAPE` |
| VGA trigger frames | 6 and 161, respectively |
| VGA tape wait | frame 165, delay word 63872, encoded speech wait 998 |

`BUBGUM.NNV` contains one phrase. The original-code-backed NNV manifest records
108,517 decoded samples. The former synthetic pop attempted nonexistent phrase
1; the synthetic screen also ended well before phrase 0 finished at the current
9,322-Hz desktop playback rate. Neither invented graphic came from source assets.

Ghidra 12.2 inspection used unpacked executable SHA-256
`565a5c5601ede858cfdc289e6dd1f3bbde7c2ec2399e52dad2dce3d1b165d465`.
The completion path in `FUN_1e59_2857`, label `2c16`, checks placed pieces,
handles the timer/completion message, initializes completed-picture interaction
through `FUN_1b43_01ee`, advances scene playback through `FUN_21bf_01ed`, and
updates audio resources. There is no third-completion screen branch in this
inspected path. `FUN_1b43_01ee` installs FILM hotspots for the solved picture.
The untimed completion feedback call is `FUN_22a0_083a(13)`; its original common
text is "I guess we're done with this one." It is common speech feedback, not
the synthetic circle screen. Full common-feedback mapping remains separate work.

The repair removes the unsupported global screen, timing, shortcut, and solved
counter. The original P11 animation/sound records, decoder, NNV file and native
speech-wait handling remain intact. A data regression checks both P11 video
assets' F_ALL/F_TAPE sound routes and tape wait command.

`completion-before.log` reproduces the real window returning to the same browser
after completions 1 and 2, then deviating after completion 3. The repaired
`completion-runtime-check.log` repeats the same key sequence and checks identical
browser viewport pixels after all three completions. The native tape runtime
separately selects P11, completes it, skips the full animation, and clicks the
original tape hotspot center `(247,156)`. Its debugger trace records native frame
161 selecting sound resource 1 / voice 0; isolated ALSA capture verifies output.
These are desktop runtime checks, not a full original DOS session replay or
human listening assessment.
