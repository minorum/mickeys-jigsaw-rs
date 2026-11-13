from pathlib import Path
import os
import signal
import struct
import subprocess
import tempfile
import time
import ctypes as c

from PIL import Image

repo = Path(__file__).resolve().parents[1]
output = repo / 'validation'


def viewport(width, height):
    scale = min((width - 32) / 320, (height - 128) / 200)
    return (width - 320 * scale) / 2, 84 + (height - 128 - 200 * scale) / 2, scale


with tempfile.TemporaryDirectory(prefix='mickey-play-screen-') as temporary:
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
    server = subprocess.Popen(['Xvfb', ':93', '-screen', '0', '1920x1440x24'], stdout=(temp / 'xvfb.log').open('w'), stderr=subprocess.STDOUT)
    game = None
    try:
        time.sleep(1)
        assert server.poll() is None, (temp / 'xvfb.log').read_text()
        from x11_input import key, click, shot, t, x, d
        log = temp / 'game.log'
        game = subprocess.Popen(['gdb', '-q', '-batch', '-x', str(temp / 'input.gdb'), str(repo / 'target/debug/mickeys-jigsaw-puzzles')], cwd=temp, env={**os.environ, 'DISPLAY': ':93', 'ALSA_CONFIG_PATH': str(temp / 'alsa.conf')}, start_new_session=True, stdout=log.open('w'), stderr=subprocess.STDOUT)
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline and 'READY' not in log.read_text():
            assert game.poll() is None, log.read_text()
            time.sleep(.1)
        assert 'READY' in log.read_text(), log.read_text()
        t.XTestFakeMotionEvent(d, -1, 100, 100, 0)
        x.XFlush(d)
        time.sleep(.3)
        assert game.poll() is None, log.read_text()
        key('Escape')
        time.sleep(.3)
        key('Return')
        time.sleep(1)
        shot(str(output / 'play-screen-unsolved.png'))
        with Image.open(output / 'play-screen-unsolved.png') as actual:
            assert actual.getpixel((0, 0)) == (24, 40, 49), 'Expected playing header'
        key('h')
        shot(str(output / 'play-screen-without-guide.png'))
        with Image.open(output / 'play-screen-unsolved.png') as guided, Image.open(output / 'play-screen-without-guide.png') as unguided:
            assert guided.crop((156, 100, 930, 660)).tobytes() != unguided.crop((156, 100, 930, 660)).tobytes(), 'H must hide the guide'
        key('h')
        tab = x.XKeysymToKeycode(d, x.XStringToKeysym(b'Tab'))
        t.XTestFakeKeyEvent(d, tab, 1, 0)
        x.XFlush(d)
        time.sleep(.2)
        shot(str(output / 'play-screen-reference.png'))
        time.sleep(1.2)
        shot(str(temp / 'reference-later.png'))
        with Image.open(output / 'play-screen-reference.png') as before, Image.open(temp / 'reference-later.png') as after:
            assert before.tobytes() == after.tobytes(), 'Reference view/timer must remain paused'
        t.XTestFakeKeyEvent(d, tab, 0, 0)
        x.XFlush(d)
        time.sleep(.1)
        root = c.c_ulong()
        parent = c.c_ulong()
        children = c.POINTER(c.c_ulong)()
        count = c.c_uint()
        x.XDefaultRootWindow.argtypes = [c.c_void_p]
        x.XDefaultRootWindow.restype = c.c_ulong
        x.XQueryTree.argtypes = [c.c_void_p, c.c_ulong, c.POINTER(c.c_ulong), c.POINTER(c.c_ulong), c.POINTER(c.POINTER(c.c_ulong)), c.POINTER(c.c_uint)]
        assert x.XQueryTree(d, x.XDefaultRootWindow(d), c.byref(root), c.byref(parent), c.byref(children), c.byref(count))
        assert count.value == 1, count.value
        window = children[0]
        x.XFree.argtypes = [c.c_void_p]
        x.XFree(children)
        x.XResizeWindow.argtypes = [c.c_void_p, c.c_ulong, c.c_uint, c.c_uint]
        for width, height, name in [(480, 360, 'compact'), (1720, 1380, 'large'), (960, 720, 'normal')]:
            x.XResizeWindow(d, window, width, height)
            x.XFlush(d)
            time.sleep(.3)
            shot(str(output / f'play-screen-{name}.png'), width, height)
        t.XTestFakeMotionEvent(d, -1, 800, 56, 0)
        t.XTestFakeButtonEvent(d, 1, 1, 0)
        x.XFlush(d)
        time.sleep(.15)
        shot(str(temp / 'mouse-reference-before.png'))
        t.XTestFakeMotionEvent(d, -1, 600, 300, 0)
        x.XFlush(d)
        time.sleep(1.2)
        shot(str(temp / 'mouse-reference-outside.png'))
        with Image.open(temp / 'mouse-reference-before.png') as before, Image.open(temp / 'mouse-reference-outside.png') as outside:
            assert before.tobytes() == outside.tobytes(), 'Held reference and timer must survive pointer leaving its button'
        t.XTestFakeButtonEvent(d, 1, 0, 0)
        x.XFlush(d)
        time.sleep(.15)
        t.XTestFakeMotionEvent(d, -1, 600, 300, 0)
        t.XTestFakeButtonEvent(d, 1, 1, 0)
        x.XFlush(d)
        time.sleep(.1)
        t.XTestFakeMotionEvent(d, -1, 800, 56, 0)
        x.XFlush(d)
        time.sleep(.1)
        shot(str(temp / 'drag-over-reference.png'))
        with Image.open(temp / 'drag-over-reference.png') as actual:
            assert actual.getpixel((752, 44)) != (237, 190, 94), 'Dragging across reference must not activate it'
        t.XTestFakeButtonEvent(d, 1, 0, 0)
        x.XFlush(d)
        key('Escape')
        key('Down')
        key('Return')
        time.sleep(1)
        vx, vy, scale = viewport(960, 720)
        def game_click(px, py):
            click(round(vx + (px + .5) * scale), round(vy + (py + .5) * scale))
        state = (1103515245 + 2 * 12345) | 1
        def next_value():
            global state
            state = (state * 214013 + 2531011) & 0xffffffff
            return (state >> 16) & 0x7fff
        for _ in range(4):
            next_value()
        order = list(range(4))
        for index in range(3, 0, -1):
            other = next_value() % (index + 1)
            order[index], order[other] = order[other], order[index]
        for piece in order:
            game_click(20, 40)
            game_click(115 + piece % 2 * 135, 51 + piece // 2 * 98)
        assert 'COMPLETED=1,2' in log.read_text(), log.read_text()
        key('Return')
        time.sleep(.2)
        shot(str(output / 'play-screen-completed.png'))
        assert game.poll() is None, log.read_text()
        (output / 'play-screen-runtime-debug.log').write_text(log.read_text())
        print('P01 VGA: unsolved/reference/completed windows captured; guide toggle changes board; reference/timer paused; 480x360/960x720/1720x1380 layouts; 4 mouse placements complete through scaled viewport', flush=True)
    finally:
        if game is not None and game.poll() is None:
            os.killpg(game.pid, signal.SIGTERM)
            game.wait(timeout=5)
        server.terminate()
        server.wait(timeout=5)
