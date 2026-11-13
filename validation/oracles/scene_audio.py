from pathlib import Path
import struct,hashlib,json,sys
from unicorn import Uc, UC_ARCH_X86, UC_MODE_16, UC_HOOK_CODE
from unicorn.x86_const import *
image=Path(sys.argv[1]).read_bytes()
assert hashlib.sha256(image).hexdigest()=='565a5c5601ede858cfdc289e6dd1f3bbde7c2ec2399e52dad2dce3d1b165d465'
u=Uc(UC_ARCH_X86,UC_MODE_16);u.mem_map(0,0x200000)
header=struct.unpack_from('<H',image,8)[0]*16
u.mem_write(0x10000,image[header:])
rt=struct.unpack_from('<H',image,24)[0]
for i in range(struct.unpack_from('<H',image,6)[0]):
 off,seg=struct.unpack_from('<HH',image,rt+4*i);a=0x10000+seg*16+off
 value=struct.unpack('<H',u.mem_read(a,2))[0]
 u.mem_write(a,struct.pack('<H',(value+0x1000)&65535))
base=0x60000
for reg,value in [(UC_X86_REG_DS,0x6000),(UC_X86_REG_SS,0x8000)]:u.reg_write(reg,value)
def word(off,value):u.mem_write(base+off,struct.pack('<H',value))
def read(off):return struct.unpack('<H',u.mem_read(base+off,2))[0]
def retfar():
 sp=u.reg_read(UC_X86_REG_SP);ss=u.reg_read(UC_X86_REG_SS)*16
 ip,cs=struct.unpack('<HH',u.mem_read(ss+sp,4))
 u.reg_write(UC_X86_REG_SP,sp+4);u.reg_write(UC_X86_REG_CS,cs);u.reg_write(UC_X86_REG_IP,ip)
events=[];sample_busy=0
def hook(u,address,size,data):
 if address==0x231f1:
  events.append('play');retfar()
 elif address==0x22ee2:events.append('stop')
 elif address in [0x196c8,0x13e24,0x23a5e]:retfar()
 elif address==0x2379e:
  u.reg_write(UC_X86_REG_AX,sample_busy);retfar()
u.hook_add(UC_HOOK_CODE,hook)
def step(label,busy,sample):
 global sample_busy
 sample_busy=sample;word(0x9b2,busy)
 u.reg_write(UC_X86_REG_CS,0x21bf);u.reg_write(UC_X86_REG_SP,0xff00)
 u.mem_write(0x8ff00,struct.pack('<HH',0,0x9000))
 start=len(events);u.emu_start(0x21c71,0x90000,count=100000)
 assert u.reg_read(UC_X86_REG_CS)==0x9000
 return dict(step=label,events=events[start:],mode=read(0x85c),queued=read(0x910))
word(0x6c4e,0x6000);word(0x1e66,65535);word(0x910,1);word(0x85c,1)
rows=[step('initial',0,0),step('speaker_busy',1,0),step('sample_busy',0,1),step('finished',0,0)]
word(0x85c,0);rows.append(step('ordinary_replacement',0,0))
assert [r['events'] for r in rows]==[['stop','play'],[],[],['play'],['stop','play']],rows
assert [r['mode'] for r in rows]==[2,2,2,2,0],rows
assert [r['queued'] for r in rows]==[1,1,1,1,0],rows
print(json.dumps(rows,indent=2))
word(0x984,3);word(0x986,1)
u.mem_write(0x13638,struct.pack('<4H',0x984,0x6000,0,0x9000))
countdowns=[]
for _ in range(4):
 u.reg_write(UC_X86_REG_CS,0x1239);u.reg_write(UC_X86_REG_SP,0xff00)
 u.mem_write(0x8ff00,struct.pack('<HHH',0,0x9000,0x202))
 u.emu_start(0x136a2,0x90000,count=1000)
 countdowns.append((read(0x984),read(0x986)))
assert countdowns==[(2,0),(1,0),(0,0),(0,0)],countdowns
print('IRQ8_COUNTDOWNS='+json.dumps(countdowns))
vga_colors=struct.unpack('<16H',u.mem_read(0x280d0+0xd2e,32))
ega_colors=struct.unpack('<16H',u.mem_read(0x280d0+0xd4e,32))
assert vga_colors==(16,24,40,56,72,88,104,120,136,152,168,184,200,216,232,248)
assert ega_colors==(1,5,10,2,7,12,13,14,14,3,8,6,11,4,9,15)
print('BUBBLE_PIXEL_INDICES='+json.dumps({'VGA':[vga_colors[15],vga_colors[0]],'EGA':[ega_colors[15],ega_colors[0]]}))
