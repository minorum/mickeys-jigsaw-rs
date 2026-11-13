from pathlib import Path
import hashlib
import struct
import sys

from unicorn import Uc, UC_ARCH_X86, UC_MODE_16
from unicorn.x86_const import *

image = Path(sys.argv[1]).read_bytes()
digest = hashlib.sha256(image).hexdigest()
assert digest in ('02de74a740e4fb51108e78f2da655983399674b35191a53fb6391dad05772630',
                  '84bc097a9a38cdfc951b8c7f1d5011cb359e4722724a7e8948696f70fd6d2f08')
tandy = digest == '84bc097a9a38cdfc951b8c7f1d5011cb359e4722724a7e8948696f70fd6d2f08'
data_segment = 0x166e if tandy else 0x143c
u = Uc(UC_ARCH_X86, UC_MODE_16)
u.mem_map(0, 0x200000)
header = struct.unpack_from('<H', image, 8)[0] * 16
u.mem_write(0x10000, image[header:])
for index in range(struct.unpack_from('<H', image, 6)[0]):
    entry = struct.unpack_from('<H', image, 24)[0] + index * 4
    offset, segment = struct.unpack_from('<HH', image, entry)
    address = 0x10000 + segment * 16 + offset
    value = struct.unpack('<H', u.mem_read(address, 2))[0]
    u.mem_write(address, struct.pack('<H', (value + 0x1000) & 65535))
rows = ['asset\tsprite\tsamples\tfnv64\n']
samples = 0
for path in sorted(Path(sys.argv[2]).glob('scene-P*.PZ4.raw' if tandy else 'scene-P*.PZ8.raw')):
    data = path.read_bytes()
    cursor = 0
    while cursor < len(data):
        tag = data[cursor:cursor + 4]
        size = struct.unpack_from('<I', data, cursor + 4)[0]
        cursor += 8
        if tag == (b'TND0' if tandy else b'MCG0'):
            chunk = data[cursor:cursor + size]
            break
        cursor += size
    else:
        raise AssertionError(path)
    cursor = sprite = 0
    while cursor < len(chunk):
        kind = struct.unpack_from('<H', chunk, cursor)[0]
        size = struct.unpack_from('<I' if kind & 0x8000 else '<H', chunk, cursor + 2)[0]
        record = chunk[cursor:cursor + size]
        assert len(record) == size and size >= 4
        cursor += size
        if kind != 0x8061:
            continue
        stride, height, fmt, depth = struct.unpack_from('<4H', record, 10)
        assert depth & 255 == (4 if tandy else 8)
        assert not tandy or stride % 2 == 0
        raw = record[26:26 + stride * height // (2 if tandy else 1)]
        assert len(raw) == stride * height // (2 if tandy else 1)
        pixels = bytes(color for byte in raw for color in (byte >> 4, byte & 15)) if tandy else raw
        u.mem_write(0x90000, raw)
        context = bytearray(record[6:26])
        struct.pack_into('<HH', context, 0, 0, 0x9000)
        u.mem_write(data_segment * 16 + (0x52 if tandy else 0x70), bytes(context))
        u.mem_write(data_segment * 16 + (0x6a if tandy else 0x88), struct.pack('<H', stride // (2 if tandy else 1)))
        positions = sorted({pixels.index(color) for color in set(pixels)})
        checksum = 0xcbf29ce484222325
        for position in positions:
            x, y = position % stride, position // stride
            u.mem_write(0x70100, struct.pack('<4H', 0, 0x7000, x, y))
            for reg, value in ((UC_X86_REG_CS, 0x1000), (UC_X86_REG_DS, data_segment),
                               (UC_X86_REG_ES, 0x7000), (UC_X86_REG_DI, 0x108),
                               (UC_X86_REG_SS, 0x8000), (UC_X86_REG_SP, 0xff00)):
                u.reg_write(reg, value)
            u.emu_start(0x13061 if tandy else 0x11fa5, 0x10084 if tandy else 0x10074, count=10000)
            assert u.reg_read(UC_X86_REG_IP) == (0x84 if tandy else 0x74)
            result = struct.unpack('<h', u.mem_read(0x70000, 2))[0]
            assert result == (-1 if pixels[position] == (0 if tandy else depth >> 8) else pixels[position])
            for byte in struct.pack('<IH', position, result & 65535):
                checksum = ((checksum ^ byte) * 0x100000001b3) & 0xffffffffffffffff
        rows.append(f'{path.name[6:-4]}\t{sprite}\t{len(positions)}\t{checksum:016x}\n')
        samples += len(positions)
        sprite += 1
Path(sys.argv[3]).write_text(''.join(rows))
print(f'{len(rows) - 1} native {"TANDY" if tandy else "VGA"} sprites; {samples} pixel reads; every stored color per sprite')
