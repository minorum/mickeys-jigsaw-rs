from pathlib import Path
import ctypes as c
import os
import signal
import struct
import subprocess
import tempfile
import time

repo = Path(__file__).resolve().parents[1]
output = repo / 'validation'
widths = [33, 34, 34, 34, 34, 34, 34, 33]
heights = [25, 24, 25, 24, 25, 24, 25, 24]
targets = [(sum(widths[:column]) + widths[column] // 2,
            2 + sum(heights[:row]) + heights[row] // 2)
           for row in range(8) for column in range(8)]
state = (1103515245 + 8 * 12345) | 1


def random_value():
    global state
    state = (state * 214013 + 2531011) & 0xffffffff
    return (state >> 16) & 0x7fff


for _ in range(112):
    random_value()
order = list(range(64))
for index in range(63, 0, -1):
    other = random_value() % (index + 1)
    order[index], order[other] = order[other], order[index]

with tempfile.TemporaryDirectory(prefix='mickey-mixed-input-') as temporary:
    temp = Path(temporary)
    for line in (output / 'assets.sha256').read_text().splitlines():
        name = line.split('  ', 1)[1]
        (temp / name).symlink_to(repo / name)
    (temp / 'mjp_config.pre').write_bytes(struct.pack('<14H', 1, 1, 0, 0, 1, 6, 1, 3, 1, 0, 0, 8, 1, 0))
    (temp / 'alsa.conf').write_text('pcm.!default { type plug slave.pcm { type null } }\n')
    (temp / 'input.gdb').write_text('''set pagination off
set confirm off
tbreak mickeys_jigsaw_puzzles::GameApp::update
commands
silent
printf "READY\\n"
continue
end
break mickeys_jigsaw_puzzles::puzzle::PuzzleSession::on_puzzle_completed
commands
silent
printf "COMPLETED=%ld,%ld\\n", self.puzzle_num, self.grid_dim
continue
end
run --setup
''')
    server = subprocess.Popen(['Xvfb', ':93', '-screen', '0', '1024x768x24'], stdout=(temp / 'xvfb.log').open('w'), stderr=subprocess.STDOUT)
    game = None
    try:
        time.sleep(1)
        assert server.poll() is None, (temp / 'xvfb.log').read_text()
        from x11_input import key, click, game_point, shot, t, x, d, root
        log = temp / 'game.log'
        game = subprocess.Popen(['gdb', '-q', '-batch', '-x', str(temp / 'input.gdb'), str(repo / 'target/debug/mickeys-jigsaw-puzzles')], cwd=temp, env={**os.environ, 'DISPLAY': ':93', 'ALSA_CONFIG_PATH': str(temp / 'alsa.conf')}, start_new_session=True, stdout=log.open('w'), stderr=subprocess.STDOUT)
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline and 'READY' not in log.read_text():
            assert game.poll() is None, log.read_text()
            time.sleep(.1)
        assert 'READY' in log.read_text(), log.read_text()
        time.sleep(.2)
        key('Escape')
        for _ in range(5):
            key('Up')
        owner = c.c_ulong()
        parent = c.c_ulong()
        children = c.POINTER(c.c_ulong)()
        count = c.c_uint()
        x.XQueryTree.argtypes = [c.c_void_p, c.c_ulong, c.POINTER(c.c_ulong), c.POINTER(c.c_ulong), c.POINTER(c.POINTER(c.c_ulong)), c.POINTER(c.c_uint)]
        assert x.XQueryTree(d, root, c.byref(owner), c.byref(parent), c.byref(children), c.byref(count))
        assert count.value == 1, count.value
        window = children[0]
        x.XFree.argtypes = [c.c_void_p]
        x.XFree(children)
        x.XResizeWindow.argtypes = [c.c_void_p, c.c_ulong, c.c_uint, c.c_uint]
        size = (960, 720)

        def point(px, py):
            return game_point(px, py, *size)

        def move(px, py):
            t.XTestFakeMotionEvent(d, -1, *point(px, py), 0)
            x.XFlush(d)
            time.sleep(.05)

        for mode_index, mode in enumerate(('vga', 'ega')):
            if mode_index:
                key('F4')
                key('Right')
                key('Escape')
            key('r')
            time.sleep(.3)
            for step, piece in enumerate(order):
                px, py = targets[piece]
                if step % 4 == 0:
                    move(20, 40)
                    t.XTestFakeButtonEvent(d, 1, 1, 0)
                    x.XFlush(d)
                    time.sleep(.06)
                    move(48 + px, py)
                    t.XTestFakeButtonEvent(d, 1, 0, 0)
                    x.XFlush(d)
                    time.sleep(.06)
                elif step % 4 == 1:
                    key('Return')
                    size = (480, 360) if size == (960, 720) else (960, 720)
                    x.XResizeWindow(d, window, *size)
                    x.XFlush(d)
                    time.sleep(.15)
                    click(*point(48 + px, py))
                elif step % 4 == 2:
                    click(*point(20, 40))
                    key('space')
                    click(*point(25 + px, py))
                    key('space')
                else:
                    click(*point(20, 40))
                    click(*point(48 + px, py))
                assert game.poll() is None, log.read_text()
            completed = [line for line in log.read_text().splitlines() if line.startswith('COMPLETED=')]
            assert completed == ['COMPLETED=1,8'] * (mode_index + 1), (mode, completed, log.read_text())
            key('Return')
            shot(str(output / f'mixed-input-{mode}.png'))
            key('Escape')
            print(f'{mode}: 64-piece curved puzzle completed; 16 drag-release, 16 keyboard-pick/resize/mouse-place, 16 hidden-tray placements, 16 click-carry; no auto-solve', flush=True)
        (output / 'mixed-input-runtime-debug.log').write_text(log.read_text())
    finally:
        if game is not None and game.poll() is None:
            os.killpg(game.pid, signal.SIGTERM)
            game.wait(timeout=5)
        server.terminate()
        server.wait(timeout=5)
