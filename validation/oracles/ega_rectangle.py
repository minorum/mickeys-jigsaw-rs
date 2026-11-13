from pathlib import Path
import hashlib, struct, sys
from unicorn import Uc, UC_ARCH_X86, UC_MODE_16, UC_HOOK_MEM_WRITE, UC_HOOK_INSN
from unicorn.x86_const import *

image = Path(sys.argv[1]).read_bytes()
assert hashlib.sha256(image).hexdigest() == '6ec038bddd09d17aa331f2f869005d9ed4e2057495adc08628ff7007576674f0'
u = Uc(UC_ARCH_X86, UC_MODE_16)
u.mem_map(0, 0x200000)
header = struct.unpack_from('<H', image, 8)[0]*16
u.mem_write(0x10000, image[header:])
rt = struct.unpack_from('<H', image, 24)[0]
for i in range(struct.unpack_from('<H', image, 6)[0]):
    off, seg = struct.unpack_from('<HH', image, rt+4*i)
    address = 0x10000+seg*16+off
    value = struct.unpack('<H', u.mem_read(address,2))[0]
    u.mem_write(address,struct.pack('<H',(value+0x1000)&65535))
for reg,value in [(UC_X86_REG_CS,0x1000),(UC_X86_REG_DS,0x16df),(UC_X86_REG_SS,0x8000),(UC_X86_REG_SP,0xff00)]:
    u.reg_write(reg,value)
for off,value in [(0xc,0xa000),(0x5b3,2),(0x5b7,3),(0x5b9,5),(0x5bb,8),(0x5bd,3),(0x4e,1),(0x20,40)]:
    u.mem_write(0x16df0+off,struct.pack('<H',value))
for y in range(200):
    u.mem_write(0x16df0+0x703+y*2,struct.pack('<H',y*40))
u.mem_write(0x11d02,struct.pack('<H',0x090e))
u.mem_write(0x8ff00,struct.pack('<H',0x8000))
pixels=[255]*64000
index=0
mask=255
writes=0

def output(machine,port,size,value,data):
    global index,mask
    if port==0x3ce:
        index=value&255
        if size==2 and index==8:mask=value>>8
    elif port==0x3cf and index==8:
        mask=value&255

def memory(machine,access,address,size,value,data):
    global writes
    if 0xa0000<=address<0xa0000+8000:
        assert size==1
        byte=address-0xa0000
        for bit in range(8):
            if mask & (128>>bit):pixels[byte*8+bit]=value&15
        writes+=1

u.hook_add(UC_HOOK_INSN,output,None,1,0,UC_X86_INS_OUT)
u.hook_add(UC_HOOK_MEM_WRITE,memory)
u.emu_start(0x11f1d,0x18000,count=100000)
assert u.reg_read(UC_X86_REG_IP)==0x8000
assert sum(p!=255 for p in pixels)==36
rows=['x\ty\tcolor\n']
for y in range(10):
    for x in range(16):rows.append(f'{x}\t{y}\t{pixels[y*320+x]}\n')
Path(sys.argv[2]).write_text(''.join(rows))
print(f'Original EGA rectangle: {writes} masked byte writes; 36 colored pixels')
