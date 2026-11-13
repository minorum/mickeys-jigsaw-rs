from pathlib import Path
import os
import signal
import struct
import subprocess
import tempfile
import time

repo = Path(__file__).resolve().parents[1]
output = repo / 'validation'
tick = 65536 / 1193182

with tempfile.TemporaryDirectory(prefix='mickey-sequences-') as temporary:
    temp = Path(temporary)
    for line in (output / 'assets.sha256').read_text().splitlines():
        name = line.split('  ', 1)[1]
        (temp / name).symlink_to(repo / name)
    (temp / 'mjp_config.pre').write_bytes(struct.pack('<14H', 1, 1, 0, 0, 1, 6, 1, 3, 1, 0, 1, 8, 1, 0))
    (temp / 'alsa.conf').write_text('pcm.!default { type plug slave.pcm { type null } }\n')
    (temp / 'input.gdb').write_text('''set pagination off
set confirm off
python import time
break mickeys_jigsaw_puzzles::puzzle::PuzzleSession::trigger_frame_side_effects
commands
silent
python print("FRAME=%d,%d,%.6f" % (int(gdb.parse_and_eval("self.puzzle_num")), int(gdb.parse_and_eval("frame_idx")), time.monotonic()), flush=True)
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

        def trace():
            return [line[6:].split(',') for line in log_path.read_text().splitlines() if line.startswith('FRAME=')]

        def wait_count(count, timeout):
            deadline = time.monotonic() + timeout
            while len(trace()) < count and time.monotonic() < deadline:
                assert game.poll() is None, log_path.read_text()
                time.sleep(.05)
            assert len(trace()) >= count, log_path.read_text()

        key('Escape')
        selected = 1
        for mode in ('vga', 'ega'):
            if mode == 'ega':
                key('F4')
                key('Right')
                key('Escape')
            for puzzle, frames, ticks, center in (
                (2, list(range(189, 197)) * 3, [2] * 24, (197, 140) if mode == 'vga' else (200, 142)),
                (11, list(range(154, 160)), [11, 11, 11, 11, 20, 11], (158, 166)),
            ):
                while selected != puzzle:
                    key('Right')
                    selected = selected % 15 + 1
                key('Return')
                key('F2')
                key('Return')
                start = len(trace())
                click(*center)
                wait_count(start + len(frames), 12)
                time.sleep(ticks[-1] * tick + .2)
                actual = trace()[start:]
                assert [int(row[1]) for row in actual] == frames, (mode, puzzle, actual)
                seconds = float(actual[-1][2]) - float(actual[0][2])
                expected = sum(ticks[:-1]) * tick
                assert abs(seconds - expected) < expected * .25, (seconds, expected)
                for previous, current, delay in zip(actual, actual[1:], ticks):
                    elapsed = float(current[2]) - float(previous[2])
                    assert abs(elapsed - delay * tick) < max(.05, delay * tick * .25), (mode, puzzle, previous, current, delay)
                start = len(trace())
                click(*center)
                wait_count(start + 1, 2)
                assert int(trace()[start][1]) == frames[0]
                shot(str(output / f'sequence-{mode}-p{puzzle:02}.png'))
                key('Return')
                key('Escape')
                print(f'{mode} P{puzzle:02}: {len(frames)} exact frames; elapsed={seconds:.3f}s expected={expected:.3f}s; finished, restarted and skipped', flush=True)
        (output / 'sequence-gameplay-debug.log').write_text(log_path.read_text())
        print('Four real-window animation runs: native repetitions and speeds, restart and skip verified', flush=True)
    finally:
        if game is not None and game.poll() is None:
            os.killpg(game.pid, signal.SIGTERM)
            game.wait(timeout=5)
        server.terminate()
        server.wait(timeout=5)
