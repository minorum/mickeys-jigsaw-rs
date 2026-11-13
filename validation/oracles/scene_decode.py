from pathlib import Path
import hashlib
import sys

from credits import decode

executable, asset_dir, corpus_dir, destination = map(Path, sys.argv[1:])
rows = ['asset\tbytes\tfnv64\tsha256\n']
total = 0
for path in sorted(corpus_dir.glob('scene-P*.raw')):
    asset = path.name[6:-4]
    result = bytes(decode(executable, asset_dir / asset))
    assert result == path.read_bytes(), asset
    declared = int.from_bytes((asset_dir / asset).read_bytes()[16:20], 'little')
    assert len(result) == declared, asset
    checksum = 0xcbf29ce484222325
    for byte in result:
        checksum = ((checksum ^ byte) * 0x100000001b3) & 0xffffffffffffffff
    rows.append(f'{asset}\t{len(result)}\t{checksum:016x}\t{hashlib.sha256(result).hexdigest()}\n')
    total += len(result)
    print(f'{asset}: complete native DA output equals raster corpus', flush=True)
assert len(rows) == 28
destination.write_text(''.join(rows))
print(f'27 complete scene streams; {total} native decoded bytes match raster corpus')
