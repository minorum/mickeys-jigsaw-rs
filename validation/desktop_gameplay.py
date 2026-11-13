from pathlib import Path
import argparse
import ctypes as c
import json
import os
import signal
import subprocess
import tempfile
import time

from PIL import Image

parser = argparse.ArgumentParser()
parser.add_argument('--display', default=':0')
parser.add_argument('--monitor')
args = parser.parse_args()
repo = Path(__file__).resolve().parents[1]
output = repo / 'validation'
runtime = Path(os.environ.get('XDG_RUNTIME_DIR') or f'/run/user/{os.getuid()}')
sockets = list(runtime.glob('niri.*.sock'))
assert len(sockets) == 1, sockets
wayland = [p.name for p in runtime.glob('wayland-*') if p.is_socket()]
assert len(wayland) == 1, wayland
environment = {**os.environ, 'DISPLAY': args.display, 'XDG_RUNTIME_DIR': str(runtime), 'WAYLAND_DISPLAY': wayland[0], 'NIRI_SOCKET': str(sockets[0])}
title = "Mickey's Jigsaw Puzzles (1991 DOS Recreation)"


def niri(*command):
    return subprocess.check_output(['niri', 'msg', '-j', *command], env=environment, text=True)


original = json.loads(niri('focused-window'))
assert not any(window['title'] == title for window in json.loads(niri('windows'))), 'Another game window is already open'
window_id = None
game = None
child_pid = None
held_tab = None
prefix = 'desktop-scaled' if args.monitor else 'desktop'
with tempfile.TemporaryDirectory(prefix='mickey-desktop-') as temporary:
    temp = Path(temporary)
    try:
        for line in (output / 'assets.sha256').read_text().splitlines():
            name = line.split('  ', 1)[1]
            (temp / name).symlink_to(repo / name)
        import struct
        (temp / 'mjp_config.pre').write_bytes(struct.pack('<14H', 1, 1, 0, 0, 1, 6, 1, 3, 1, 0, 0, 8, 1, 0))
        (temp / 'alsa.conf').write_text('pcm.!default { type plug slave.pcm { type null } }\n')
        (temp / 'input.gdb').write_text('''set pagination off
set confirm off
break mickeys_jigsaw_puzzles::puzzle::PuzzleSession::on_puzzle_completed
commands
silent
printf "COMPLETED=%ld,%ld\\n", self.puzzle_num, self.grid_dim
continue
end
run --setup
''')
        log = temp / 'game.log'
        game = subprocess.Popen(['gdb', '-q', '-batch', '-x', str(temp / 'input.gdb'), str(repo / 'target/debug/mickeys-jigsaw-puzzles')], cwd=temp, env={**environment, 'ALSA_CONFIG_PATH': str(temp / 'alsa.conf')}, start_new_session=True, stdout=log.open('w'), stderr=subprocess.STDOUT)
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline:
            assert game.poll() is None, log.read_text()
            windows = [window for window in json.loads(niri('windows')) if window['title'] == title]
            if windows:
                assert len(windows) == 1, windows
                own = windows[0]
                children = Path(f'/proc/{game.pid}/task/{game.pid}/children').read_text().split()
                owned = [int(pid) for pid in children if Path(f'/proc/{pid}/exe').resolve() == repo / 'target/debug/mickeys-jigsaw-puzzles']
                assert len(owned) == 1, owned
                child_pid = owned[0]
                window_id = own['id']
                break
            time.sleep(.1)
        assert window_id is not None, log.read_text()
        if args.monitor:
            niri('action', 'move-window-to-monitor', '--id', str(window_id), args.monitor)
        niri('action', 'focus-window', '--id', str(window_id))
        time.sleep(.3)
        def focused():
            current = json.loads(niri('focused-window'))
            assert current and current['id'] == window_id, 'Game lost focus; aborting input'

        def capture(name):
            focused()
            path = output / f'{prefix}-{name}.png'
            path.unlink(missing_ok=True)
            niri('action', 'screenshot-window', '--id', str(window_id), '--path', str(path))
            deadline = time.monotonic() + 5
            while time.monotonic() < deadline:
                try:
                    with Image.open(path) as captured:
                        captured.verify()
                    return path
                except OSError:
                    time.sleep(.02)
            raise AssertionError(f'Capture did not finish: {path}')

        os.environ['MICKEY_TEST_DISPLAY'] = args.display
        from x11_input import key as raw_key, click as raw_click, game_point, t, x, d, root

        def key(name):
            focused()
            raw_key(name)

        children = c.POINTER(c.c_ulong)()
        count = c.c_uint()
        owner, parent = c.c_ulong(), c.c_ulong()
        x.XQueryTree.argtypes = [c.c_void_p, c.c_ulong, c.POINTER(c.c_ulong), c.POINTER(c.c_ulong), c.POINTER(c.POINTER(c.c_ulong)), c.POINTER(c.c_uint)]
        assert x.XQueryTree(d, root, c.byref(owner), c.byref(parent), c.byref(children), c.byref(count))
        x.XFetchName.argtypes = [c.c_void_p, c.c_ulong, c.POINTER(c.c_char_p)]
        x.XFree.argtypes = [c.c_void_p]
        clients = []
        for index in range(count.value):
            name = c.c_char_p()
            if x.XFetchName(d, children[index], c.byref(name)) and name.value:
                if name.value.decode(errors='replace') == title:
                    clients.append(children[index])
                x.XFree(name)
        x.XFree(children)
        assert len(clients) == 1, clients
        client = clients[0]
        x.XGetGeometry.argtypes = [c.c_void_p, c.c_ulong, c.POINTER(c.c_ulong), c.POINTER(c.c_int), c.POINTER(c.c_int), c.POINTER(c.c_uint), c.POINTER(c.c_uint), c.POINTER(c.c_uint), c.POINTER(c.c_uint)]
        x.XTranslateCoordinates.argtypes = [c.c_void_p, c.c_ulong, c.c_ulong, c.c_int, c.c_int, c.POINTER(c.c_int), c.POINTER(c.c_int), c.POINTER(c.c_ulong)]

        def geometry():
            a, b = c.c_int(), c.c_int()
            width, height, border, depth = c.c_uint(), c.c_uint(), c.c_uint(), c.c_uint()
            child = c.c_ulong()
            assert x.XGetGeometry(d, client, c.byref(child), c.byref(a), c.byref(b), c.byref(width), c.byref(height), c.byref(border), c.byref(depth))
            assert x.XTranslateCoordinates(d, client, root, 0, 0, c.byref(a), c.byref(b), c.byref(child))
            return a.value, b.value, width.value, height.value

        def click(px, py):
            focused()
            left, top, width, height = geometry()
            point = game_point(px, py, width, height)
            raw_click(left + point[0], top + point[1])

        key('Escape')
        key('Down')
        key('Return')
        time.sleep(.4)
        unsolved = capture('unsolved')
        with Image.open(unsolved) as image:
            assert image.getpixel((image.width - 208, 44))[:3] == (54, 76, 87), 'Expected unsolved view without a held reference'
        focused()
        tab = x.XKeysymToKeycode(d, x.XStringToKeysym(b'Tab'))
        t.XTestFakeKeyEvent(d, tab, 1, 0)
        held_tab = tab
        x.XFlush(d)
        time.sleep(.15)
        first = capture('reference')
        time.sleep(1.2)
        second = capture('reference-paused')
        with Image.open(first) as before, Image.open(second) as after:
            assert before.tobytes() == after.tobytes(), 'Desktop reference/timer did not pause'
        t.XTestFakeKeyEvent(d, tab, 0, 0)
        held_tab = None
        x.XFlush(d)
        focused()
        niri('action', 'fullscreen-window')
        time.sleep(.4)
        fullscreen = geometry()[2:]
        capture('fullscreen')
        state = (1103515245 + 2 * 12345) | 1
        def random_value():
            global state
            state = (state * 214013 + 2531011) & 0xffffffff
            return (state >> 16) & 0x7fff
        for _ in range(4):
            random_value()
        order = list(range(4))
        for index in range(3, 0, -1):
            other = random_value() % (index + 1)
            order[index], order[other] = order[other], order[index]
        for piece in order:
            click(20, 40)
            click(115 + piece % 2 * 135, 51 + piece // 2 * 98)
        assert 'COMPLETED=1,2' in log.read_text(), log.read_text()
        key('Return')
        capture('completed')
        (output / f'{prefix}-runtime-debug.log').write_text(log.read_text())
        print(f'Niri/Xwayland: mapped and focused own game; compositor-only window captures; reference/timer paused; fullscreen {fullscreen}; monitor={args.monitor or "current"}; four mouse placements completed; no auto-solve', flush=True)
    finally:
        if held_tab is not None:
            t.XTestFakeKeyEvent(d, held_tab, 0, 0)
            x.XFlush(d)
        current = json.loads(niri('focused-window'))
        restore = current and current['id'] == window_id
        if child_pid is not None and Path(f'/proc/{child_pid}').exists():
            os.kill(child_pid, signal.SIGTERM)
        if game is not None and game.poll() is None:
            os.killpg(game.pid, signal.SIGTERM)
            game.wait(timeout=5)
        if restore and original:
            niri('action', 'focus-window', '--id', str(original['id']))
