from pathlib import Path
import hashlib
import struct
import sys

from unicorn import Uc, UC_ARCH_X86, UC_MODE_16, UC_HOOK_CODE
from unicorn.x86_const import *

image = Path(sys.argv[1]).read_bytes()
assert hashlib.sha256(image).hexdigest() == '565a5c5601ede858cfdc289e6dd1f3bbde7c2ec2399e52dad2dce3d1b165d465'
u = Uc(UC_ARCH_X86, UC_MODE_16)
u.mem_map(0, 0x100000)
header = struct.unpack_from('<H', image, 8)[0] * 16
u.mem_write(0x10000, image[header:])
relocation = struct.unpack_from('<H', image, 24)[0]
for index in range(struct.unpack_from('<H', image, 6)[0]):
    offset, segment = struct.unpack_from('<HH', image, relocation + index * 4)
    address = 0x10000 + segment * 16 + offset
    value = struct.unpack('<H', u.mem_read(address, 2))[0]
    u.mem_write(address, struct.pack('<H', (value + 0x1000) & 65535))
u.reg_write(UC_X86_REG_DS, 0x6000)
u.reg_write(UC_X86_REG_SS, 0x8000)


def word(offset, value=None):
    if value is not None:
        u.mem_write(0x60000 + offset, struct.pack('<H', value))
    return struct.unpack('<H', u.mem_read(0x60000 + offset, 2))[0]


def retfar(value=None):
    sp = u.reg_read(UC_X86_REG_SP)
    ip, cs = struct.unpack('<HH', u.mem_read(0x80000 + sp, 4))
    u.reg_write(UC_X86_REG_SP, sp + 4)
    u.reg_write(UC_X86_REG_CS, cs)
    u.reg_write(UC_X86_REG_IP, ip)
    if value is not None:
        u.reg_write(UC_X86_REG_AX, value)


frames = []
explicit = 0


def hook(machine, address, size, data):
    if address == 0x21bf0:
        frame = word(0x1384)
        frames.append(frame)
        word(0x980, explicit if frame == 2 else 0)
        retfar(1)
    elif address in (0x21d1b, 0x21c71, 0x1c128):
        retfar()


u.hook_add(UC_HOOK_CODE, hook)
rows = ['repetitions\tdefault_ticks\texplicit_ticks\tframes\tframe_ticks\n']
for repetitions in (0, 1, 2, 3, 4, 6):
    for delay in (2, 3, 4, 11):
        for explicit in (0, 9):
            frames.clear()
            ticks = []
            for offset, value in ((0xdc0, 0), (0x848, 2), (0x972, 0), (0x974, 0),
                                  (0x978, repetitions), (0x982, delay), (0x984, 0),
                                  (0x97e, 0), (0x1376, 0), (0x137e, 1),
                                  (0x1380, 4), (0x1384, 1), (0x858, 0), (0xc558, 0)):
                word(offset, value)
            while word(0x858) == 0:
                assert len(frames) < 30
                word(0x984, 0)
                u.reg_write(UC_X86_REG_CS, 0x21bf)
                u.reg_write(UC_X86_REG_SP, 0xff00)
                u.mem_write(0x8ff00, struct.pack('<HH', 0, 0x9000))
                u.emu_start(0x21ddd, 0x90000, count=100000)
                assert u.reg_read(UC_X86_REG_CS) == 0x9000
                ticks.append(word(0x97a))
            assert frames == [1, 2, 3] * max(repetitions, 1)
            assert ticks == [delay, explicit + 1 if explicit else delay, delay] * max(repetitions, 1)
            rows.append(f'{repetitions}\t{delay}\t{explicit}\t' + ','.join(map(str, frames)) + '\t' + ','.join(map(str, ticks)) + '\n')
Path(sys.argv[2]).write_text(''.join(rows))
print('48 original sequence playback traces; zero and one each play once; explicit delay overrides defaults')
