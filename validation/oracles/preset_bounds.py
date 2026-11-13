from pathlib import Path
import hashlib
import struct
import sys

from unicorn import Uc, UC_ARCH_X86, UC_MODE_16
from unicorn.x86_const import *


def words(data, offset, count=1):
    return struct.unpack_from('<' + 'H' * count, data, offset)


image = Path(sys.argv[1]).read_bytes()
assert hashlib.sha256(image).hexdigest() == '565a5c5601ede858cfdc289e6dd1f3bbde7c2ec2399e52dad2dce3d1b165d465'
rows = ['asset\tpiece\tframe\tdx\tdy\tflags\tx\ty\twidth\theight\n']
assets = sorted(Path(sys.argv[2]).glob('scene-P*.raw'))
assert assets
for path in assets:
    data = path.read_bytes()
    position = 0
    chunk = None
    while position < len(data):
        tag = data[position:position + 4]
        size = struct.unpack_from('<I', data, position + 4)[0]
        position += 8
        if tag in (b'MCG0', b'TND0', b'EGA0', b'CGA0'):
            chunk = data[position:position + size]
            break
        position += size
    assert chunk is not None, path
    records, sprites = [], []
    position = 0
    while position < len(chunk):
        kind = words(chunk, position)[0]
        size = (struct.unpack_from('<I', chunk, position + 2)[0]
                if kind & 0x8000 else words(chunk, position + 2)[0])
        assert size >= 4 and position + size <= len(chunk), path
        record = bytearray(chunk[position:position + size])
        if kind == 0x7ff1:
            command_offset = 12
            for _ in range(words(record, 10)[0]):
                command_kind, command_size = words(record, command_offset, 2)
                if command_kind in (0x60, 0x80, 0x90):
                    resource = words(record, command_offset + 8)[0]
                    struct.pack_into('<H', record, command_offset + 8, max(resource - 1, 0))
                command_offset += command_size
        if kind in (0x7ff0, 0x7ff1):
            records.append(record)
        elif kind == 0x8061:
            sprites.append(record)
        position += size
    u = Uc(UC_ARCH_X86, UC_MODE_16)
    u.mem_map(0, 0x100000)
    header = words(image, 8)[0] * 16
    u.mem_write(0x10000, image[header:])
    relocation = words(image, 24)[0]
    for index in range(words(image, 6)[0]):
        offset, segment = words(image, relocation + 4 * index, 2)
        address = 0x10000 + segment * 16 + offset
        value = struct.unpack('<H', u.mem_read(address, 2))[0]
        u.mem_write(address, struct.pack('<H', (value + 0x1000) & 65535))
    u.reg_write(UC_X86_REG_DS, 0x6000)
    u.reg_write(UC_X86_REG_SS, 0x8000)
    u.mem_write(0x61440, struct.pack('<HH', 320, 200))
    u.mem_write(0x6016c, struct.pack('<H', len(sprites)))
    address = 0x90000
    for table, items in [(0x6c7c4, records), (0x6aeb6, sprites)]:
        for index, record in enumerate(items):
            assert address + len(record) < 0xff000, path
            u.mem_write(address, bytes(record))
            u.mem_write(table + index * 4, struct.pack('<HH', 0, address // 16))
            address += (len(record) + 15) & ~15
    shape = records[4]
    assert words(shape, 0)[0] == 0x7ff1
    offset, piece = 12, 0
    for _ in range(words(shape, 10)[0]):
        kind, size = words(shape, offset, 2)
        if kind == 0x40:
            command = shape[offset:offset + size]
            u.mem_write(0x70800, bytes(command))
            args = [0x900, 0x7000, 0x800, 0x7000, 3]
            u.mem_write(0x8ff00, struct.pack('<7H', 0, 0xff00, *args))
            u.reg_write(UC_X86_REG_SP, 0xff00)
            u.reg_write(UC_X86_REG_CS, 0x196c)
            u.emu_start(0x1a4c5, 0xff000, count=1000000)
            assert u.reg_read(UC_X86_REG_CS) == 0xff00, path
            bounds = struct.unpack('<4h', u.mem_read(0x70900, 8))
            dx, dy, frame, flags = struct.unpack_from('<hhHH', command, 4)
            rows.append('\t'.join(map(str, (path.name[6:-4], piece, frame, dx, dy, flags, *bounds))) + '\n')
            piece += 1
        offset += size
    assert piece > 0 and offset == len(shape), path
Path(sys.argv[3]).write_text(''.join(rows))
print(f'{len(rows) - 1} original preset-piece bounds across {len(assets)} source variants')
