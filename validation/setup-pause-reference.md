# Setup playback pause

Opening setup previously suspended PuzzleSession updates while Rodio continued
consuming speech and music. Returning could therefore leave animation waits and
dialogue out of step with the already-finished sound.

The app now applies pause/resume after input transitions, including F4's early
return, so every entry and exit uses the current screen state. Caption countdowns
also stop during setup. Existing Rodio sinks resume at their retained playback
position. Preference changes that stop a channel still remove its playback;
resuming does not recreate it. No original asset data or decode logic changed.

A device-free regression checks pause/resume on both speech and music sinks and
muting while paused. An actual Xvfb window run starts the original size-selection
speech, opens setup with F4, waits for output buffering to settle, captures a
1.2-second interval, and returns with Escape. Isolated ALSA capture has zero peak
in the paused interval and nonzero samples after return. See
`setup-pause-runtime.json`, `setup-pause-runtime-check.log` and screenshots.
This proves desktop playback pause, not cycle-exact original DOS synchronization.
