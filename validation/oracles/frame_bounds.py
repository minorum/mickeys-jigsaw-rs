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
for index in range(3):
    u.mem_write(0x6c7c4 + index*4, struct.pack('<HH', index*0x100, 0x7000))
u.mem_write(0x61440, struct.pack('<HH', 320, 200))
rows = ['outer\tmiddle\tinner\tx\ty\tw\th\n']
def frame(offset, words):
    u.mem_write(0x70000 + offset, struct.pack('<'+'H'*len(words), *[w & 65535 for w in words]))
frame(0x200, [0x7ff1,28,0,0,0,1,0x50,16,7,11,5,0,13,17])
for outer in range(4):
    for middle in range(4):
        for inner in range(4):
            frame(0, [0x7ff1,24,0,0,0,1,0x40,12,5,9,1,middle])
            frame(0x100, [0x7ff1,24,0,0,0,1,0x40,12,-3,4,2,inner])
            args = [0x800,0x7000,0,13,19,outer,0]
            u.mem_write(0x8ff00, struct.pack('<9H',0,0x9000,*args))
            u.reg_write(UC_X86_REG_SP,0xff00)
            u.reg_write(UC_X86_REG_CS,0x196c)
            u.emu_start(0x1a1aa,0x90000,count=100000)
            assert u.reg_read(UC_X86_REG_CS)==0x9000
            bounds=struct.unpack('<4h',u.mem_read(0x70800,8))
            rows.append('\t'.join(map(str,(outer,middle,inner,*bounds)))+'\n')
Path(sys.argv[2]).write_text(''.join(rows))
print('64 original nested reflection bounds')
