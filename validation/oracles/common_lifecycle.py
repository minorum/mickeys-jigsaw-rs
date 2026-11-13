from pathlib import Path
import hashlib
import struct
import sys

from unicorn import Uc, UC_ARCH_X86, UC_MODE_16, UC_HOOK_CODE
from unicorn.x86_const import *

image = Path(sys.argv[1]).read_bytes()
assert hashlib.sha256(image).hexdigest() == '565a5c5601ede858cfdc289e6dd1f3bbde7c2ec2399e52dad2dce3d1b165d465'
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
events = []
load_fails = 0


def retfar(result=0):
    sp = u.reg_read(UC_X86_REG_SP)
    address = u.reg_read(UC_X86_REG_SS) * 16 + sp
    ip, cs = struct.unpack('<HH', u.mem_read(address, 4))
    u.reg_write(UC_X86_REG_SP, sp + 4)
    u.reg_write(UC_X86_REG_AX, result)
    u.reg_write(UC_X86_REG_CS, cs)
    u.reg_write(UC_X86_REG_IP, ip)


def hook(machine, address, size, data):
    if address in (0x1b020, 0x1b037, 0x1b1d6, 0x13e24):
        retfar()
    elif address == 0x23a5e:
        events.append('stop')
        retfar()
    elif address == 0x22b47:
        sp = machine.reg_read(UC_X86_REG_SP)
        bank, resource, mode = struct.unpack('<3H', machine.mem_read(0x280d0 + sp + 4, 6))
        assert mode == 1
        events.append(f'load:{bank}:{resource}')
        retfar(load_fails)
    elif address == 0x22d55:
        sp = machine.reg_read(UC_X86_REG_SP)
        arguments = struct.unpack('<6H', machine.mem_read(0x280d0 + sp + 4, 12))
        assert arguments[:2] == (1, 3)
        events.append(f'play:{arguments[5]}')
        retfar()
    elif address == 0x22b21:
        events.append('wait')
        retfar()


u.hook_add(UC_HOOK_CODE, hook)
rows = ['event\thardware\tspeech\tload_fails\toperations\n']
for event in range(15):
    bank, resource, phrase = struct.unpack('<3H', u.mem_read(0x280d0 + 0x1dd0 + event * 10, 6))
    for hardware in (0, 1):
        for speech in (0, 1):
            for load_fails in (0, 1):
                for offset, value in ((0x864, hardware), (0xdb6, speech), (0xdc4, 0)):
                    u.mem_write(0x280d0 + offset, struct.pack('<H', value))
                events.clear()
                for reg, value in ((UC_X86_REG_DS, 0x280d), (UC_X86_REG_SS, 0x280d),
                                   (UC_X86_REG_CS, 0x22a0), (UC_X86_REG_SP, 0xff00)):
                    u.reg_write(reg, value)
                u.mem_write(0x37fd0, struct.pack('<3H', 0, 0x9000, event))
                u.emu_start(0x2323a, 0x90000, count=100000)
                assert u.reg_read(UC_X86_REG_CS) == 0x9000
                expected = []
                if hardware and speech:
                    expected = ['stop', f'load:{bank}:{resource}']
                    if not load_fails:
                        expected += [f'play:{phrase}'] + ([] if event == 6 else ['wait'])
                assert events == expected, (event, hardware, speech, load_fails, events, expected)
                rows.append(f'{event}\t{hardware}\t{speech}\t{load_fails}\t{",".join(events) or "none"}\n')
Path(sys.argv[2]).write_text(''.join(rows))
print(f'{len(rows) - 1} native common-feedback lifecycle cases; captions disabled; I/O and playback boundaries intercepted')
