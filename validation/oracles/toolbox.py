from pathlib import Path
import struct, sys
from credits import decode

body = decode(Path(sys.argv[1]), Path(sys.argv[2]))
count = struct.unpack_from('<H', body)[0]
assert count == 3
pos = 2 + count * 4
rows = ['variant\tbytes\tsprites\tbpp\tpalette_bytes\tfnv1a64\n']
for variant in range(count):
    size = struct.unpack_from('<I', body, 2 + variant*4)[0]
    chunk = body[pos:pos+size]
    assert len(chunk) == size
    pos += size
    cursor = 0
    sprites = 0
    depths = set()
    palette_bytes = None
    while cursor < len(chunk):
        assert cursor+8 <= len(chunk)
        tag = chunk[cursor:cursor+4]
        length = struct.unpack_from('<I',chunk,cursor+4)[0]
        data = chunk[cursor+8:cursor+8+length]
        assert len(data) == length
        cursor += 8+length
        if tag == b'PALE':palette_bytes = length
        elif tag == b'TBOX':
            offset = 0
            while offset < len(data):
                kind, size = struct.unpack_from('<HI',data,offset)
                assert kind == 0x8061 and size >= 26 and offset+size <= len(data)
                depths.add(struct.unpack_from('<H',data,offset+16)[0] & 255)
                sprites += 1
                offset += size
    assert len(depths) == 1
    depth = depths.pop()
    assert palette_bytes >= 3 * (1 << depth)
    digest = 14695981039346656037
    for byte in chunk:digest = ((digest ^ byte) * 1099511628211) & 0xffffffffffffffff
    rows.append(f'{variant}\t{len(chunk)}\t{sprites}\t{depth}\t{palette_bytes}\t{digest:016x}\n')
assert pos == len(body)
Path(sys.argv[3]).write_text(''.join(rows))
print('3 original toolbox variants inventoried and hashed')
