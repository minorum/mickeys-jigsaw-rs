from pathlib import Path
import argparse
import os
import signal
import struct
import subprocess
import tempfile
import time

from PIL import Image

parser = argparse.ArgumentParser()
parser.add_argument('native_pixels', type=Path)
parser.add_argument('native_scene', type=Path)
args = parser.parse_args()
repo = Path(__file__).resolve().parents[1]
output = repo / 'validation'
data = args.native_scene.read_bytes()
position = 0
while position < len(data):
    tag = data[position:position + 4]
    size = struct.unpack_from('<I', data, position + 4)[0]
    position += 8
    if tag == b'PALE':
        palette = data[position:position + 48]
        break
    position += size
else:
    raise AssertionError('Missing native palette')
native = args.native_pixels.read_bytes()
assert len(native) == 64000
expected = Image.frombytes('P', (320, 200), native)
expected.putpalette(palette)
expected = expected.convert('RGB').crop((10, 10, 260, 180)).resize((750, 510), Image.Resampling.NEAREST)
parts = [list(map(int, line.split('\t')[1:])) for line in (output / 'preset-bounds.tsv').read_text().splitlines()[1:] if line.startswith('P03.PZ4\t')]


def order(count, preset):
    state = (3 * 1103515245 + (1 if preset else 2) * 12345) | 1

    def next_value():
        nonlocal state
        state = (state * 214013 + 2531011) & 0xffffffff
        return (state >> 16) & 0x7fff

    if not preset:
        for _ in range(4):
            next_value()
    result = list(range(count))
    for index in range(count - 1, 0, -1):
        other = next_value() % (index + 1)
        result[index], result[other] = result[other], result[index]
    return result


with tempfile.TemporaryDirectory(prefix='mickey-background-pattern-') as temporary:
    temp = Path(temporary)
    for line in (output / 'assets.sha256').read_text().splitlines():
        name = line.split('  ', 1)[1]
        (temp / name).symlink_to(repo / name)
    (temp / 'mjp_config.pre').write_bytes(struct.pack('<14H', 2, 1, 0, 0, 1, 6, 1, 3, 0, 0, 0, 8, 1, 0))
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
    server = subprocess.Popen(['Xvfb', ':93', '-screen', '0', '1024x768x24'], stdout=(temp / 'xvfb.log').open('w'), stderr=subprocess.STDOUT)
    game = None
    try:
        time.sleep(1)
        assert server.poll() is None, (temp / 'xvfb.log').read_text()
        from x11_input import key, click_game as click, shot_game as shot, t, x, d
        log = temp / 'game.log'
        game = subprocess.Popen(['gdb', '-q', '-batch', '-x', str(temp / 'input.gdb'), str(repo / 'target/debug/mickeys-jigsaw-puzzles')], cwd=temp, env={**os.environ, 'DISPLAY': ':93', 'ALSA_CONFIG_PATH': str(temp / 'alsa.conf')}, start_new_session=True, stdout=log.open('w'), stderr=subprocess.STDOUT)
        time.sleep(2)
        assert game.poll() is None, log.read_text()
        key('Escape')
        key('Right')
        key('Right')
        key('Down')
        for preset in (False, True):
            if preset:
                key('Down')
            key('Return')
            targets = [(115, 51), (250, 51), (115, 149), (250, 149)]
            if preset:
                targets = []
                for part in parts:
                    px, py, width, height = part[5:]
                    left, top = max(2, px), max(2, py)
                    width, height = min(272, px + width) - left, min(198, py + height) - top
                    assert width > 0 and height > 0
                    targets.append((48 + left - 2 + width // 2, top + height // 2))
            for piece in order(len(targets), preset):
                click(20, 40)
                click(*targets[piece])
            completions = [line for line in log.read_text().splitlines() if line.startswith('COMPLETED=')]
            assert completions == (['COMPLETED=3,2', 'COMPLETED=3,1'] if preset else ['COMPLETED=3,2']), completions
            key('Return')
            t.XTestFakeMotionEvent(d, -1, 3, 3, 0)
            x.XFlush(d)
            time.sleep(.2)
            name = 'preset' if preset else 'grid'
            screenshot = output / f'background-pattern-{name}.png'
            shot(str(screenshot))
            with Image.open(screenshot) as actual:
                assert actual.crop((99, 30, 849, 540)).tobytes() == expected.tobytes(), f'{name}: native scene pixels differ'
            assert game.poll() is None, log.read_text()
            key('Escape')
            print(f'P03 EGA {name}: {len(targets)} mouse placements; completion observed; 42500 displayed scene pixels match native output', flush=True)
        (output / 'background-pattern-runtime-debug.log').write_text(log.read_text())
    finally:
        if game is not None and game.poll() is None:
            os.killpg(game.pid, signal.SIGTERM)
            game.wait(timeout=5)
        server.terminate()
        server.wait(timeout=5)
