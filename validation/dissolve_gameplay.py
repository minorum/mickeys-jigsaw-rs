from pathlib import Path
import os
import signal
import struct
import subprocess
import tempfile
import time

from PIL import Image

repo = Path(__file__).resolve().parents[1]
output = repo / 'validation'

with tempfile.TemporaryDirectory(prefix='mickey-dissolve-') as temporary:
    temp = Path(temporary)
    for line in (output / 'assets.sha256').read_text().splitlines():
        name = line.split('  ', 1)[1]
        (temp / name).symlink_to(repo / name)
    (temp / 'mjp_config.pre').write_bytes(struct.pack('<14H', 1, 1, 0, 0, 1, 6, 1, 3, 0, 0, 0, 8, 1, 0))
    (temp / 'alsa.conf').write_text('pcm.!default { type plug slave.pcm { type null } }\n')
    (temp / 'input.gdb').write_text('''set pagination off
set confirm off
break mickeys_jigsaw_puzzles::graphics::render_scene_dissolve
commands
silent
printf "DISSOLVE=%ld,%ld,%f\\n", previous_frame, frame, progress
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
            return [line[9:].split(',') for line in log_path.read_text().splitlines() if line.startswith('DISSOLVE=')]

        key('Escape')
        for _ in range(4):
            key('Right')
        key('Return')
        key('F2')
        key('Return')
        for mode, center, frame in (('vga', (66, 40), 184), ('ega', (67, 44), 182)):
            if mode == 'ega':
                key('F4')
                key('Right')
                key('Escape')
            paths = {phase: output / f'dissolve-{mode}-{phase}.png' for phase in ('before', 'mid', 'after', 'idle', 'complete')}
            shot(str(paths['before']))
            start = len(trace())
            began = time.monotonic()
            click(*center)
            shot(str(paths['mid']))
            time.sleep(.18)
            shot(str(paths['after']))
            assert time.monotonic() - began < .5, 'Snapshots exceeded the first frame interval'
            rows = trace()[start:]
            assert rows and all((int(row[0]), int(row[1])) == (3, frame) for row in rows), rows
            assert any(0 < float(row[2]) < 1 for row in rows), rows
            assert any(float(row[2]) >= 1 for row in rows), rows
            key('Return')
            count = len(trace())
            time.sleep(.2)
            assert len(trace()) == count, 'Skip left a dissolve active'
            shot(str(paths['idle']))
            images = {phase: Image.open(paths[phase]) for phase in ('before', 'mid', 'after')}
            old_count = new_count = 0
            for y in range(2, 198):
                for x in range(25, 295):
                    point = (x * 3 + 1, y * 3 + 1)
                    before, middle, after = (images[phase].getpixel(point) for phase in ('before', 'mid', 'after'))
                    assert middle in (before, after), (mode, x, y, before, middle, after)
                    if before != after:
                        old_count += middle == before
                        new_count += middle == after
            for image in images.values():
                image.close()
            assert old_count > 0 and new_count > 0, (mode, old_count, new_count)
            with Image.open(paths['before']) as before, Image.open(paths['idle']) as idle:
                assert before.crop((75, 6, 885, 594)).tobytes() == idle.crop((75, 6, 885, 594)).tobytes()
            start = len(trace())
            click(*center)
            time.sleep(.8)
            rows = trace()[start:]
            assert any(int(row[1]) == frame + 1 and float(row[2]) >= 1 for row in rows), rows
            assert all(int(row[1]) in (frame, frame + 1) for row in rows), rows
            shot(str(paths['complete']))
            with Image.open(paths['before']) as before, Image.open(paths['complete']) as complete:
                assert before.crop((75, 6, 885, 594)).tobytes() == complete.crop((75, 6, 885, 594)).tobytes()
            assert game.poll() is None, log_path.read_text()
            print(f'{mode} P05 projector: mixed {old_count} old / {new_count} new changed pixels; both targets reached; skip and natural completion returned to idle', flush=True)
        (output / 'dissolve-gameplay-debug.log').write_text(log_path.read_text())
        print('Both live P05 dissolves preserve source pixels and remain skippable', flush=True)
    finally:
        if game is not None and game.poll() is None:
            os.killpg(game.pid, signal.SIGTERM)
            game.wait(timeout=5)
        server.terminate()
        server.wait(timeout=5)
