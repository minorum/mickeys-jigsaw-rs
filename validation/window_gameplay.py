from pathlib import Path
import argparse
import os
import signal
import struct
import subprocess
import tempfile
import time

repo = Path(__file__).resolve().parents[1]
output = repo / 'validation'
parser = argparse.ArgumentParser()
parser.add_argument('--preset', action='store_true')
preset = parser.parse_args().preset
prefix = 'window-preset' if preset else 'window-matrix'
bounds = {}
if preset:
    for line in (output / 'preset-bounds.tsv').read_text().splitlines()[1:]:
        asset, *values = line.split('\t')
        bounds.setdefault(asset, []).append(list(map(int, values)))


def tray_order(puzzle, count=4):
    state = (puzzle * 1103515245 + (1 if preset else 2) * 12345) | 1
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


with tempfile.TemporaryDirectory(prefix='mickey-window-matrix-') as temporary:
    temp = Path(temporary)
    for line in (output / 'assets.sha256').read_text().splitlines():
        name = line.split('  ', 1)[1]
        (temp / name).symlink_to(repo / name)
    (temp / 'mjp_config.pre').write_bytes(struct.pack('<14H', 1, 1, 0, 0, 1, 6, 1, 3, 1, 0, 1, 8, 1, 0))
    (temp / 'alsa.conf').write_text('pcm.!default { type plug slave.pcm { type null } }\n')
    (temp / 'input.gdb').write_text('''set pagination off
set confirm off
break mickeys_jigsaw_puzzles::puzzle::PuzzleSession::on_puzzle_completed
commands
silent
printf "COMPLETED=%ld\\n", self.puzzle_num
continue
end
run --setup
''')
    server = subprocess.Popen(['Xvfb', ':93', '-screen', '0', '1024x768x24'], stdout=(temp / 'xvfb.log').open('w'), stderr=subprocess.STDOUT)
    game = None
    try:
        time.sleep(1)
        assert server.poll() is None, (temp / 'xvfb.log').read_text()
        from x11_input import key, click_game as click, shot_game as shot
        log_path = temp / 'game.log'
        game = subprocess.Popen(['gdb', '-q', '-batch', '-x', str(temp / 'input.gdb'), str(repo / 'target/debug/mickeys-jigsaw-puzzles')], cwd=temp, env={**os.environ, 'DISPLAY': ':93', 'ALSA_CONFIG_PATH': str(temp / 'alsa.conf')}, start_new_session=True, stdout=log_path.open('w'), stderr=subprocess.STDOUT)
        time.sleep(2)
        assert game.poll() is None, log_path.read_text()
        key('Escape')
        key('Down')
        if preset:
            key('Down')
        expected = []
        placed = 0
        for mode in ['vga', 'ega']:
            if mode == 'ega':
                key('F4')
                key('Right')
                key('Escape')
                key('Right')
            for puzzle in range(1, 16):
                key('Return')
                targets = [(115 + (piece % 2) * 135, 51 + (piece // 2) * 98) for piece in range(4)]
                if preset:
                    asset = f'P{puzzle:02}.PZ4' if mode == 'ega' else f'P{puzzle:02}.PZ8'
                    parts = bounds.get(asset, bounds[f'P{puzzle:02}.PZ8'])
                    targets = []
                    for part in parts:
                        x, y, w, h = part[5:]
                        left, top = max(2, x), max(2, y)
                        width, height = min(272, x + w) - left, min(198, y + h) - top
                        assert width > 0 and height > 0
                        targets.append((48 + left - 2 + width // 2, top + height // 2))
                for move_index, piece in enumerate(tray_order(puzzle, len(targets))):
                    click(20, 40)
                    click(*targets[piece])
                    placed += 1
                    if puzzle == 1 and move_index == 0:
                        shot(str(output / f'{prefix}-{mode}-partial-p01.png'))
                    if preset and mode == 'ega' and puzzle == 11 and move_index == 0:
                        from PIL import Image
                        shown = output / 'window-preset-ega-tray-shown.png'
                        hidden = output / 'window-preset-ega-tray-hidden.png'
                        time.sleep(.3)
                        shot(str(shown))
                        key('space')
                        shot(str(hidden))
                        with Image.open(shown) as before, Image.open(hidden) as after:
                            assert before.crop((144, 60, 954, 420)).tobytes() == after.crop((75, 60, 885, 420)).tobytes(), 'Tray toggle changed native board pixels'
                            glyphs = sum(before.getpixel((x, y)) == (0, 0, 0)
                                         for y in range(450, 480) for x in range(15, 945))
                            assert glyphs > 900, f'Blank pickup caption: {glyphs} black pixels'
                            print(f'EGA P11: tray toggle preserved board pixels; caption has {glyphs} black glyph pixels', flush=True)
                        key('space')
                time.sleep(.1)
                expected.append(puzzle)
                actual = [int(line.split('=')[1]) for line in log_path.read_text().splitlines() if line.startswith('COMPLETED=')]
                assert actual == expected, (mode, puzzle, actual, expected, log_path.read_text()[-2000:])
                assert game.poll() is None
                if puzzle in [1, 11, 15]:
                    shot(str(output / f'{prefix}-{mode}-p{puzzle:02}.png'))
                key('Escape')
                if puzzle != 15:
                    key('Right')
                print(f'{mode} P{puzzle:02}: {len(targets)} pieces placed; completion observed; returned to browser', flush=True)
        debug_log = 'window-preset-debug.log' if preset else 'window-gameplay-debug.log'
        (output / debug_log).write_text(log_path.read_text())
        print(f'30 real-window games completed; {placed} pointer-placed pieces; no auto-solve', flush=True)
    finally:
        if game is not None and game.poll() is None:
            os.killpg(game.pid, signal.SIGTERM)
            game.wait(timeout=5)
        server.terminate()
        server.wait(timeout=5)
