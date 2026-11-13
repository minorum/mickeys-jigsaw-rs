from pathlib import Path
import hashlib
import struct
import sys

from unicorn import Uc, UC_ARCH_X86, UC_MODE_16, UC_HOOK_CODE, UC_HOOK_MEM_WRITE
from unicorn.x86_const import *

rows = ['variant\tcount\tfnv64\tfirst32\tlast16\n']
for variant, path, digest in (
    ('VGA', sys.argv[1], '02de74a740e4fb51108e78f2da655983399674b35191a53fb6391dad05772630'),
    ('EGA', sys.argv[2], '6ec038bddd09d17aa331f2f869005d9ed4e2057495adc08628ff7007576674f0'),
):
    image = Path(path).read_bytes()
    assert hashlib.sha256(image).hexdigest() == digest
    u = Uc(UC_ARCH_X86, UC_MODE_16)
    u.mem_map(0, 0x200000)
    header = struct.unpack_from('<H', image, 8)[0] * 16
    u.mem_write(0x10000, image[header:])
    relocation = struct.unpack_from('<H', image, 24)[0]
    for index in range(struct.unpack_from('<H', image, 6)[0]):
        offset, segment = struct.unpack_from('<HH', image, relocation + index * 4)
        address = 0x10000 + segment * 16 + offset
        value = struct.unpack('<H', u.mem_read(address, 2))[0]
        u.mem_write(address, struct.pack('<H', (value + 0x1000) & 65535))
    ega = variant == 'EGA'
    data_segment = 0x16df if ega else 0x143c
    for reg, value in ((UC_X86_REG_CS, 0x1000), (UC_X86_REG_DS, data_segment),
                       (UC_X86_REG_SS, 0x8000), (UC_X86_REG_SP, 0xff00)):
        u.reg_write(reg, value)
    for offset, value in ((4, 0xa000), (8, 0x9000), (0x14, 320), (0x16, 200)):
        u.mem_write(data_segment * 16 + offset, struct.pack('<H', value))
    order = []

    def copy_cell(machine, address, size, data):
        if address == 0x10893:
            x, y = machine.reg_read(UC_X86_REG_BP), machine.reg_read(UC_X86_REG_DI)
            assert x < 160 and y < 100
            offset = y * 320 + x
            order.extend((offset, offset + 32000, offset + 160, offset + 32160))
            sp = machine.reg_read(UC_X86_REG_SP)
            ip = struct.unpack('<H', machine.mem_read(0x80000 + sp, 2))[0]
            machine.reg_write(UC_X86_REG_SP, sp + 2)
            machine.reg_write(UC_X86_REG_IP, ip)

    def write_pixel(machine, access, address, size, value, data):
        if 0xa0000 <= address < 0xa0000 + 64000:
            assert size == 1
            order.append(address - 0xa0000)

    if ega:
        u.hook_add(UC_HOOK_CODE, copy_cell)
    else:
        u.hook_add(UC_HOOK_MEM_WRITE, write_pixel)
    u.emu_start(0x107fb if ega else 0x1063e,
                0x10476 if ega else 0x10396, count=10000000)
    assert u.reg_read(UC_X86_REG_IP) == (0x476 if ega else 0x396)
    assert len(order) == 64000 and len(set(order)) == 64000
    checksum = 0xcbf29ce484222325
    for offset in order:
        for byte in struct.pack('<H', offset):
            checksum = ((checksum ^ byte) * 0x100000001b3) & 0xffffffffffffffff
    rows.append(f'{variant}\t{len(order)}\t{checksum:016x}\t' + ','.join(map(str, order[:32])) + '\t' + ','.join(map(str, order[-16:])) + '\n')
    print(f'{variant}: all 64000 pixels exactly once; order FNV64={checksum:016x}')
Path(sys.argv[3]).write_text(''.join(rows))
