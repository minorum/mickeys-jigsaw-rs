from pathlib import Path
import hashlib, struct, sys
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
for reg, value in [(UC_X86_REG_DS, 0x280d), (UC_X86_REG_SS, 0x8000)]:
    u.reg_write(reg, value)
clips=[]
def word(offset,value):u.mem_write(0x280d0+offset,struct.pack('<H',value))
def hook(machine,address,size,data):
    if address not in [0x196c8,0x1badd,0x1b980,0x1bc2f,0x1bec0,0x1b793]:return
    sp=u.reg_read(UC_X86_REG_SP)
    if address==0x196c8:
        command,pointer=struct.unpack('<HH',u.mem_read(0x80000+sp+4,4))
        if command==0x306:clips.append(struct.unpack('<4h',u.mem_read(0x280d0+pointer,8)))
    ip,cs=struct.unpack('<HH',u.mem_read(0x80000+sp,4))
    u.reg_write(UC_X86_REG_SP,sp+4)
    u.reg_write(UC_X86_REG_CS,cs)
    u.reg_write(UC_X86_REG_IP,ip)
u.hook_add(UC_HOOK_CODE,hook)
rows=['state\tx\ty\twidth\theight\n']
for state,expected in [(2,(2,2,270,196)),(3,(34,2,270,196)),(5,(0,0,320,200)),(6,(2,2,270,196))]:
    clips.clear()
    word(0x848,state);word(0x1440,320);word(0x1442,200)
    u.reg_write(UC_X86_REG_CS,0x1b43);u.reg_write(UC_X86_REG_SP,0xff00)
    u.mem_write(0x8ff00,struct.pack('<HH',0,0x9000))
    u.emu_start(0x1c00d,0x90000,count=10000)
    assert u.reg_read(UC_X86_REG_CS)==0x9000
    assert len(clips)==2 and clips[-1]==expected,(state,clips)
    rows.append('\t'.join(map(str,(state,*clips[-1])))+'\n')
Path(sys.argv[2]).write_text(''.join(rows))
print('4 original state viewport interiors verified')
