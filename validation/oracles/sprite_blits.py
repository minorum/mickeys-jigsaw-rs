from pathlib import Path
import hashlib
import struct
import sys

from unicorn import Uc, UC_ARCH_X86, UC_MODE_16
from unicorn.x86_const import *

image = Path(sys.argv[1]).read_bytes()
assert hashlib.sha256(image).hexdigest() == '02de74a740e4fb51108e78f2da655983399674b35191a53fb6391dad05772630'
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


def word(offset, value):
    u.mem_write(0x143c0 + offset, struct.pack('<H', value))


rows = ['asset\tsprite\tx\ty\tflags\tclip_x\tclip_y\tclip_w\tclip_h\tfnv64\n']
for path in sorted(Path(sys.argv[2]).glob('scene-P*.PZ8.raw')):
    data = path.read_bytes()
    cursor = 0
    while cursor < len(data):
        tag = data[cursor:cursor + 4]
        size = struct.unpack_from('<I', data, cursor + 4)[0]
        cursor += 8
        if tag == b'MCG0':
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
        stride, height, fmt, depth, sx, sy, width, sub_h = struct.unpack_from('<8H', record, 10)
        assert depth & 255 == 8 and width and sub_h
        raw = record[26:26 + stride * height]
        assert len(raw) == stride * height
        u.mem_write(0x90000, raw)
        context = bytearray(record[6:26])
        struct.pack_into('<HH', context, 0, 0, 0x9000)
        cases = [(23, 19, 0, (0, 0, 320, 200)),
                 (-width // 2, 20, 1, (2, 2, 270, 196)),
                 (311, 195, 2, (0, 0, 320, 200)),
                 (4, -sub_h // 2, 3, (2, 2, 270, 196))]
        if depth >> 8 or sprite == 0:
            cases += [(23, 19, mode | flip, (0, 0, 320, 200))
                      for mode in (0x10, 0x2a20) for flip in range(4)]
        for x, y, flags, clip in cases:
            u.mem_write(0x14430, bytes(context))
            for offset, value in ((0x20, 320), (0x88, stride), (0x92, 1),
                                  (0x6e, (flags >> 4) & 15), (0x4c, flags >> 8),
                                  (0x26, clip[0]), (0x28, clip[1]),
                                  (0x2a, clip[2]), (0x2c, clip[3]),
                                  (0xc, 0xa000), (0x870, flags & 3)):
                word(offset, value)
            u.mem_write(0x14c16, b'\2')
            u.mem_write(0xa0000, bytes([255]) * 64000)
            u.mem_write(0x8ff00, struct.pack('<H', 0x74))
            for reg, value in ((UC_X86_REG_CS, 0x1000), (UC_X86_REG_DS, 0x143c),
                               (UC_X86_REG_SS, 0x8000), (UC_X86_REG_SP, 0xff00),
                               (UC_X86_REG_AX, x & 65535), (UC_X86_REG_BX, y & 65535)):
                u.reg_write(reg, value)
            u.emu_start(0x11bb8, 0x10074, count=10000000)
            assert u.reg_read(UC_X86_REG_IP) == 0x74
            checksum = 0xcbf29ce484222325
            for byte in u.mem_read(0xa0000, 64000):
                checksum = ((checksum ^ byte) * 0x100000001b3) & 0xffffffffffffffff
            rows.append('\t'.join(map(str, (path.name[6:-4], sprite, x, y, flags, *clip))) + f'\t{checksum:016x}\n')
        sprite += 1
    print(f'{path.name}: {sprite} sprites drawn', flush=True)
Path(sys.argv[3]).write_text(''.join(rows))
print(f'{len(rows) - 1} native VGA full-frame blit hashes')
