from pathlib import Path
import os
import signal
import struct
import subprocess
import tempfile
import time

repo = Path(__file__).resolve().parents[1]
output = repo / 'validation'
grid_rects = [list(map(int, row.split('\t')))[1:] for row in (output / 'piece-capture.tsv').read_text().splitlines()[1:] if row.startswith('6\t')]
parts = {}
for row in (output / 'preset-bounds.tsv').read_text().splitlines()[1:]:
    asset, *values = row.split('\t')
    if asset.startswith('P01.'):
        parts.setdefault(asset, []).append(list(map(int, values)))


def order(dim, count):
    state = (1103515245 + dim * 12345) | 1
    def random():
        nonlocal state
        state = (state * 214013 + 2531011) & 0xffffffff
        return (state >> 16) & 0x7fff
    if dim != 1:
        for _ in range(2 * dim * (dim - 1)):
            random()
    result = list(range(count))
    for index in range(count - 1, 0, -1):
        other = random() % (index + 1)
        result[index], result[other] = result[other], result[index]
    return [result.pop(4)] + result


with tempfile.TemporaryDirectory(prefix='mickey-tray-edge-') as temporary:
    temp = Path(temporary)
    for line in (output / 'assets.sha256').read_text().splitlines():
        name = line.split('  ', 1)[1]
        (temp / name).symlink_to(repo / name)
    (temp / 'mjp_config.pre').write_bytes(struct.pack('<14H', 1, 1, 0, 0, 1, 6, 1, 3, 0, 0, 0, 8, 1, 0))
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
        from x11_input import key, click_game as click, shot_game as shot
        log_path = temp / 'game.log'
        game = subprocess.Popen(['gdb', '-q', '-batch', '-x', str(temp / 'input.gdb'), str(repo / 'target/debug/mickeys-jigsaw-puzzles')], cwd=temp, env={**os.environ, 'DISPLAY': ':93', 'ALSA_CONFIG_PATH': str(temp / 'alsa.conf')}, start_new_session=True, stdout=log_path.open('w'), stderr=subprocess.STDOUT)
        time.sleep(2)
        assert game.poll() is None, log_path.read_text()
        key('Escape')
        selected_dim = 3
        expected = []
        placed = 0
        for mode in ('vga', 'ega'):
            if mode == 'ega':
                key('F4')
                key('Right')
                key('Escape')
            for dim in (6, 1):
                while selected_dim != dim:
                    key('Up' if selected_dim < dim else 'Down')
                    selected_dim += 1 if selected_dim < dim else -1
                key('Return')
                if dim == 6:
                    targets = [(48 + x - 2 + w // 2, y + h // 2) for _, x, y, w, h in grid_rects]
                else:
                    targets = []
                    for part in parts['P01.PZ4' if mode == 'ega' else 'P01.PZ8']:
                        x, y, w, h = part[5:]
                        left, top = max(2, x), max(2, y)
                        targets.append((48 + left - 2 + (min(272, x + w) - left) // 2, top + (min(198, y + h) - top) // 2))
                for move, piece in enumerate(order(dim, len(targets))):
                    click(20, 185 if move == 0 else 40)
                    click(*targets[piece])
                    placed += 1
                    if move == 0:
                        shot(str(output / f'tray-edge-{mode}-{dim}.png'))
                expected.append((1, dim))
                actual = [tuple(map(int, row.split('=')[1].split(','))) for row in log_path.read_text().splitlines() if row.startswith('COMPLETED=')]
                assert actual == expected, (mode, dim, actual, expected)
                assert game.poll() is None, log_path.read_text()
                key('Escape')
                print(f'{mode} P01 mode={dim}: bottom padding selected the fifth visible piece; all {len(targets)} pieces placed', flush=True)
        (output / 'tray-edge-gameplay-debug.log').write_text(log_path.read_text())
        print(f'Four real-window games completed; {placed} mouse placements; no auto-solve', flush=True)
    finally:
        if game is not None and game.poll() is None:
            os.killpg(game.pid, signal.SIGTERM)
            game.wait(timeout=5)
        server.terminate()
        server.wait(timeout=5)
