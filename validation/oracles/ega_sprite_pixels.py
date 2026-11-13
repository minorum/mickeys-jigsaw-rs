from pathlib import Path
import hashlib
import struct
import sys

from unicorn import Uc, UC_ARCH_X86, UC_MODE_16
from unicorn.x86_const import *


def load(path, digest):
    image = Path(path).read_bytes()
    assert hashlib.sha256(image).hexdigest() == digest
    machine = Uc(UC_ARCH_X86, UC_MODE_16)
    machine.mem_map(0, 0x200000)
    header = struct.unpack_from('<H', image, 8)[0] * 16
    machine.mem_write(0x10000, image[header:])
    for index in range(struct.unpack_from('<H', image, 6)[0]):
        entry = struct.unpack_from('<H', image, 24)[0] + index * 4
        offset, segment = struct.unpack_from('<HH', image, entry)
        address = 0x10000 + segment * 16 + offset
        value = struct.unpack('<H', machine.mem_read(address, 2))[0]
        machine.mem_write(address, struct.pack('<H', (value + 0x1000) & 65535))
    return machine


main = load(sys.argv[1], '565a5c5601ede858cfdc289e6dd1f3bbde7c2ec2399e52dad2dce3d1b165d465')
ega = load(sys.argv[2], '6ec038bddd09d17aa331f2f869005d9ed4e2057495adc08628ff7007576674f0')
rows = ['asset\tsprite\tstride\theight\tfnv64\n']
reads = 0
for path in sorted(Path(sys.argv[3]).glob('scene-P*.PZ4.raw')):
    data = path.read_bytes()
    cursor = 0
    while cursor < len(data):
        tag = data[cursor:cursor + 4]
        size = struct.unpack_from('<I', data, cursor + 4)[0]
        cursor += 8
        if tag == b'TND0':
            chunk = data[cursor:cursor + size]
            break
        cursor += size
    else:
        raise AssertionError(path)
    cursor = sprite = 0
    while cursor < len(chunk):
        kind = struct.unpack_from('<H', chunk, cursor)[0]
        size = struct.unpack_from('<I' if kind & 0x8000 else '<H', chunk, cursor + 2)[0]
        record = bytearray(chunk[cursor:cursor + size])
        assert len(record) == size and size >= 4
        cursor += size
        if kind != 0x8061:
            continue
        stride, height = struct.unpack_from('<HH', record, 10)
        assert stride % 2 == 0 and record[16] == 4
        raw = record[26:26 + stride * height // 2]
        assert len(raw) == stride * height // 2
        struct.pack_into('<HH', record, 6, 26, 0x5000)
        main.mem_write(0x50000, bytes(record))
        main.mem_write(0x90000, bytes(65536))
        main.mem_write(0x8ff00, struct.pack('<6H', 0, 0xff00, 0, 0x5000, 0, 0x9000))
        for reg, value in ((UC_X86_REG_CS, 0x1239), (UC_X86_REG_DS, 0x280d),
                           (UC_X86_REG_SS, 0x8000), (UC_X86_REG_SP, 0xff00)):
            main.reg_write(reg, value)
        main.emu_start(0x1305d, 0xff000, count=10000000)
        assert main.reg_read(UC_X86_REG_CS) == 0xff00
        converted_size = struct.unpack('<I', main.mem_read(0x90002, 4))[0]
        assert 26 < converted_size <= 65536
        converted = bytes(main.mem_read(0x90000, converted_size))
        colors = {color for byte in raw for color in (byte >> 4, byte & 15)}
        assert struct.unpack_from('<HHH', converted, 10) == ((stride + 7) & ~7, height, len(colors - {0}))
        ega.mem_write(0x90000, converted)
        ega.mem_write(0x16e42, converted[6:26])
        checksum = 0xcbf29ce484222325
        for position in range(stride * height):
            x, y = position % stride, position // stride
            ega.mem_write(0x70100, struct.pack('<4H', 0, 0x7000, x, y))
            for reg, value in ((UC_X86_REG_CS, 0x1000), (UC_X86_REG_DS, 0x16df),
                               (UC_X86_REG_ES, 0x7000), (UC_X86_REG_DI, 0x108),
                               (UC_X86_REG_SS, 0x8000), (UC_X86_REG_SP, 0xff00)):
                ega.reg_write(reg, value)
            ega.emu_start(0x12e1e, 0x10078, count=100000)
            assert ega.reg_read(UC_X86_REG_IP) == 0x78
            result = struct.unpack('<h', ega.mem_read(0x70000, 2))[0]
            byte = raw[position // 2]
            color = byte >> 4 if position % 2 == 0 else byte & 15
            assert result == (-1 if color == 0 else color), (path.name, sprite, x, y, result, color)
            for byte in struct.pack('<h', result):
                checksum = ((checksum ^ byte) * 0x100000001b3) & 0xffffffffffffffff
            reads += 1
        rows.append(f'{path.name[6:-4]}\t{sprite}\t{stride}\t{height}\t{checksum:016x}\n')
        sprite += 1
    print(f'{path.name}: {sprite} converted sprites checked', flush=True)
Path(sys.argv[4]).write_text(''.join(rows))
print(f'{len(rows) - 1} native EGA converted sprites; {reads} pixel reads')
