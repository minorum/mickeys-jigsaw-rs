# Tray toggle during drag

Hiding the tray changes the board origin from x=48 to x=25. Previously Space or
right-click changed that layout without cancelling an active left-button drag;
release could place the held piece against the shifted board. The shared tray
toggle now cancels the gesture while retaining the held piece. Click-to-carry
continues normally; the next explicit placement click is accepted.

A regression covers all seven grid sizes: press a tray piece, hide the tray,
release on its new target, verify it remains held and unplaced, then explicitly
click to place it. Existing drag and click-to-carry coverage remains enabled.
An actual window run uses held left-button input plus Space, releases on the
shifted target, moves the held piece away, and explicitly clicks the target.
The held and placed screenshots show both states. The runtime image-difference
assertion alone is not internal-state proof; the regression checks that state.
