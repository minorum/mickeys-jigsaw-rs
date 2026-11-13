# Common speech lifecycle

The executable reference now runs the original common-feedback dispatcher
at 22a0:083a, rather than only extracting its event table. The unpacked main
executable is checked against SHA-256
`565a5c5601ede858cfdc289e6dd1f3bbde7c2ec2399e52dad2dce3d1b165d465`.
MZ relocations are applied and SS=DS preserves its near stack-pointer model.

`oracles/common_lifecycle.py` exercises all 15 events under eight combinations
of speech-hardware availability, the speech preference and resource-load
success/failure: 120 traces. File I/O, physical playback, cursor operations
and the playback-wait boundary are intercepted. Captions are disabled in the
native trace; this does not measure playback duration or caption scheduling.

Observed ordering:

- With hardware and speech enabled: stop prior playback, load the selected
  archive/resource, then queue the selected phrase if loading succeeds.
- Failed loading stops prior playback but does not queue or wait.
- Disabled hardware or speech leaves playback alone and performs no load.
- Successful events call the playback-wait helper, except event 6.

The resource and phrase arguments are read from native calls and checked
against the native event table. The seven modern UI cues are all among the
events that wait in DOS. The modern UI deliberately stays responsive and
allows another action to replace speech; it does not busy-wait for a phrase.
Its transient caption reading time is also a documented modernization choice.
Unused legacy dialog events are not added to the modern UI.

The Rust regression injects real Rodio idle queues for preceding speech and
music, then triggers each of the seven cues with speech enabled and disabled.
It consumes queue output and verifies cancellation/preservation against the
native trace, independently of a hardware audio device. It also checks caption
replacement and stop-all cleanup. Existing pause tests cover both channels;
the prior real-window/ALSA capture covers actual seven-cue playback.
No production behavior changed after this comparison.

```
python3 validation/oracles/common_lifecycle.py /tmp/mickey-original-unpacked/final/output.000.exe validation/common-lifecycle.tsv
cargo test common_feedback_lifecycle_matches_original_muting_and_replacement
```

This resolves the common dispatcher ordering question for the retained cues.
Full historical dialog scripting, native caption geometry/timing and physical
speaker continuity remain outside this trace's coverage. It does not claim
that intercepted hardware calls actually produced sound.
