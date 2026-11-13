from pathlib import Path
import hashlib, struct, sys, json
from unicorn import Uc, UC_ARCH_X86, UC_MODE_16, UC_HOOK_CODE
from unicorn.x86_const import *

image = Path(sys.argv[1]).read_bytes()
assert hashlib.sha256(image).hexdigest() == '565a5c5601ede858cfdc289e6dd1f3bbde7c2ec2399e52dad2dce3d1b165d465'
u = Uc(UC_ARCH_X86, UC_MODE_16)
u.mem_map(0, 0x100000)
header = struct.unpack_from('<H', image, 8)[0] * 16
u.mem_write(0x10000, image[header:])
rt = struct.unpack_from('<H', image, 24)[0]
for i in range(struct.unpack_from('<H', image, 6)[0]):
    off, seg = struct.unpack_from('<HH', image, rt + 4*i)
    address = 0x10000 + seg*16 + off
    value = struct.unpack('<H', u.mem_read(address, 2))[0]
    u.mem_write(address, struct.pack('<H', (value + 0x1000) & 65535))
for reg, value in [(UC_X86_REG_DS, 0x6000), (UC_X86_REG_SS, 0x8000)]:
    u.reg_write(reg, value)
u.mem_write(0x6c7c4, struct.pack('<HH', 0, 0x7000))
calls = []
intercept_nested = False

def hook(machine, address, size, data):
    if address == 0x196c8 or address == 0x1a73a or (intercept_nested and address == 0x1abb0):
        if address != 0x196c8:
            calls.append('subframe' if address == 0x1abb0 else 'rectangle')
        sp = machine.reg_read(UC_X86_REG_SP)
        ip, cs = struct.unpack('<HH', machine.mem_read(0x80000 + sp, 4))
        machine.reg_write(UC_X86_REG_SP, sp + 4)
        machine.reg_write(UC_X86_REG_CS, cs)
        machine.reg_write(UC_X86_REG_IP, ip)

u.hook_add(UC_HOOK_CODE, hook)

def run(top, command):
    global intercept_nested
    intercept_nested = top
    calls.clear()
    words = [0x7ff1, 28, 0, 0, 0, 1, command, 16, 0, 0, 0, 0, 4, 4]
    u.mem_write(0x70000, struct.pack('<14H', *words))
    args = [0, 0x7000, 65534] if top else [0, 0, 0, 0, 0, 0]
    u.mem_write(0x8ff00, struct.pack('<'+'H'*(2+len(args)), 0, 0x9000, *args))
    u.reg_write(UC_X86_REG_SP, 0xff00)
    u.reg_write(UC_X86_REG_CS, 0x196c)
    u.emu_start(0x1ae63 if top else 0x1abb0, 0x90000, count=10000)
    assert u.reg_read(UC_X86_REG_CS) == 0x9000
    return list(calls)

result = {'top_a0': run(True, 0xa0), 'top_subframe': run(True, 0x40),
          'nested_a0': run(False, 0xa0), 'nested_rectangle': run(False, 0x50)}
assert result == {'top_a0': [], 'top_subframe': ['subframe'],
                  'nested_a0': [], 'nested_rectangle': ['rectangle']}, result
print(json.dumps(result, indent=2))
