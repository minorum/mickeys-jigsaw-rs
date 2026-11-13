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
for reg, value in [(UC_X86_REG_DS, 0x280d), (UC_X86_REG_SS, 0x280d)]:
    u.reg_write(reg, value)
def word(offset,value):
    u.mem_write(0x280d0+offset,struct.pack('<H',value))
def retfar(value=None,segment=None):
    sp=u.reg_read(UC_X86_REG_SP)
    ip,cs=struct.unpack('<HH',u.mem_read(0x280d0+sp,4))
    u.reg_write(UC_X86_REG_SP,sp+4)
    u.reg_write(UC_X86_REG_CS,cs)
    u.reg_write(UC_X86_REG_IP,ip)
    if value is not None:u.reg_write(UC_X86_REG_AX,value)
    if segment is not None:u.reg_write(UC_X86_REG_DX,segment)
captured=[]
def hook(machine,address,size,data):
    global captured
    if address==0x196c8:
        sp=u.reg_read(UC_X86_REG_SP)
        args=struct.unpack('<4H',u.mem_read(0x280d0+sp+4,8))
        if args[0]==0x3f0e:
            pointer=args[1]+args[2]*16
            captured.append(struct.unpack('<4H',u.mem_read(pointer+18,8)))
            if len(captured)==dim*dim:u.emu_stop()
            else:retfar()
        else:retfar()
    elif address in [0x1b069,0x1ae63,0x13d59,0x13dfd,0x1393a]:retfar()
    elif address==0x1384e:retfar(4096,0)
    elif address==0x138e2:retfar(0,0x7000)
u.hook_add(UC_HOOK_CODE,hook)
rows=['grid\tpiece\tx\ty\twidth\theight\n']
for dim in range(2,9):
    captured=[]
    for offset,value in [(0xe42,dim),(0xe44,dim-1),(0xe46,0),(0xdc6,0),(0x170,0),(0x172,0x7100)]:word(offset,value)
    u.reg_write(UC_X86_REG_CS,0x1e59)
    u.reg_write(UC_X86_REG_SP,0xff00)
    u.mem_write(0x37fd0,struct.pack('<HH',0,0x9000))
    u.emu_start(0x1e6ac,0x90000,count=100000)
    assert len(captured)==dim*dim,(dim,len(captured))
    assert captured[0][:2]==(2,2),captured[0]
    for index,rectangle in enumerate(captured):
        rows.append('\t'.join(map(str,(dim,index,*rectangle)))+'\n')
Path(sys.argv[2]).write_text(''.join(rows))
print('203 original piece capture rectangles; first origin (2,2) in all 7 grids')
