# Common feedback reference

The original unpacked executable SHA-256 is
`565a5c5601ede858cfdc289e6dd1f3bbde7c2ec2399e52dad2dce3d1b165d465`.
`oracles/common_feedback.py` extracts all 15 five-word event rows at main DS
`1dd0 + event*10`. Main DS is original segment `180d`; the archive pointer table
at `63fa` resolves archive 0 to PROG.ESS and archive 1 to DATA.ESS.
`FUN_22a0_0147` selects zero-based resources; `FUN_22a0_083a` selects the phrase
and optional caption. The extracted fixture is `common-feedback.tsv`.

Browser dispatch `FUN_1e59_20ab` selects events 3/4/5 for next/previous/start.
`FUN_1b43_12de` selects size prompt 7; mapped address `1f3cc` queues piece prompt
8. `FUN_1e59_2857` selects return-piece 9 and other-picture 10. These seven cues
are bound to the corresponding modern actions. New common cues stop preceding
playback. Invented ESS aliases and unrelated standalone SFX requests were removed;
correct placement, hint, print, edge toggle and tray scrolling have no invented cue.

Wrong placement calls `FUN_22a0_0c24`, then `FUN_1239_15f5`. The latter toggles
port 61 through 80 cycles and restores its initial gate. `oracles/error_buzz.py`
executes those original bytes and checks all 161 port writes. Busy-loop timing
makes the DOS pitch CPU dependent. Desktop synthesis deliberately uses 880 Hz,
80 cycles, gain 3200 and 22050 Hz samples (2004 samples), controlled by the SFX
preference. This is procedural audio, not a substitute recording or decoder fallback.

The actual game was driven through next, previous, both size changes, start,
piece pickup, incorrect placement, return and browser navigation. GDB recorded
the corresponding event calls; isolated ALSA output captured 10.996 seconds,
377854 nonzero samples and peak 24384. The isolated error window had peak 3200
and 7616 nonzero samples. The game remained alive. See the runtime log, JSON and
screenshot. No human listening or physical speaker test was performed.

Regression tests check all table rows against the extracted fixture, seven event
semantics, bank identity, nonempty decoding, buzz bounds and disabled audio.
The seven bound common events now display the exact extracted captions in a
modern transient panel. Carriage returns retain the source line breaks. Speech
Text disables the panel independently of audio. Captions work with sound disabled
or unavailable; a new cue replaces the previous caption and stop-all clears it.
Display duration is the longer of decoded speech duration, two seconds, and
25 caption characters per second. This reading-time policy and panel geometry
are modernization choices, not original DOS timing or bubble geometry. Tests
cover fixture text, replacement, cancellation, expiration, muted playback and
VGA/EGA preference rendering. The actual window was checked through size changes,
caption expiration, puzzle start and piece pickup. Replacement and muting are
now traced at the common dispatcher boundary in `common-lifecycle-reference.md`;
full historical caption/dialog scripting remains unverified. Scene speech bubbles
are a separate implemented path. Exact CPU pitch, PIT phase and analog speaker
response are not established.
