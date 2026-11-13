# Live desktop verification — 2026-11-12

`desktop_gameplay.py` launches the current debug game under the real
Niri/Xwayland session with isolated preferences, read-only source symlinks and
audio disabled. It checks focus before injecting input, identifies the owned
inferior through GDB's child process, and closes only its own processes.
Niri's PID field refers to the Xwayland bridge on this host, so it is not used
as the game's process identity.

The completed runs are recorded in:

- `desktop-runtime.log`: windowed and 1920x1080 fullscreen on the current display.
- `desktop-scaled-runtime.log`: windowed and 5120x1440 fullscreen on DP-2.
- `desktop-environment.log`: scale 1.0 and 1.25 respectively; the original
  focus is restored and no test game window remains.
- `desktop{,-scaled}-runtime-debug.log`: original game completion callbacks.

Each run checks the ordinary unsolved view, then compares complete compositor
captures during a 1.2-second reference hold. It releases the reference,
enters fullscreen, places all four pieces through real XTest mouse events,
and verifies completion without auto-solve. The `desktop-*.png` captures were
visually inspected. They capture only the game window through Niri itself,
not an Xvfb buffer or an unrelated portion of the desktop.

Niri writes captures asynchronously. Each destination is removed before the
request; the script waits for a newly written, decodable PNG before reading it.
Synthetic Tab is released during cleanup even if a check fails. Niri's native
screenshot action also places each capture in the clipboard.

Reproduce on a compatible desktop with:

```
python3 validation/desktop_gameplay.py
python3 validation/desktop_gameplay.py --monitor DP-2
```

The monitor argument must name an existing output. The script requires one
discoverable Niri/Wayland session, Xwayland `:0` (or `--display`), GDB, Pillow
and the existing X11/XTest helper libraries. It performs visible interaction
with its own window and temporarily focuses/fullscreens it.

`desktop-xvfb-regression.log` verifies the helper's default `:93` behavior is
unchanged after adding explicit desktop-display selection. Current formatter,
strict Clippy, 100 debug/release tests, both builds and all 89 original hashes
pass in the `final-desktop-` logs. Native Wayland backend support and human
listening are not inferred from this Xwayland verification.
