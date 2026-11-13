from pathlib import Path
import struct,sys,hashlib
from unicorn import *
from unicorn.x86_const import *
IMAGE = b''
def load_original(path):
 global IMAGE
 IMAGE=Path(path).read_bytes()
 assert hashlib.sha256(IMAGE).hexdigest() == '565a5c5601ede858cfdc289e6dd1f3bbde7c2ec2399e52dad2dce3d1b165d465'
def trace(song):
 u=Uc(UC_ARCH_X86,UC_MODE_16);u.mem_map(0,0x200000)
 h=struct.unpack_from('<H',IMAGE,8)[0]*16;u.mem_write(0x10000,IMAGE[h:]);rt=struct.unpack_from('<H',IMAGE,24)[0]
 for i in range(struct.unpack_from('<H',IMAGE,6)[0]):
  off,seg=struct.unpack_from('<HH',IMAGE,rt+4*i);a=0x10000+seg*16+off;v=struct.unpack('<H',u.mem_read(a,2))[0];u.mem_write(a,struct.pack('<H',(v+0x1000)&65535))
 u.mem_write(0x275e0,song)
 for r,v in [(UC_X86_REG_CS,0x1239),(UC_X86_REG_DS,0x275e),(UC_X86_REG_ES,0),(UC_X86_REG_SS,0x8000),(UC_X86_REG_SP,0xff00)]:u.reg_write(r,v)
 divisor=0;low=None;gate=False;initial=True
 def out(u,p,size,v,data):
  nonlocal divisor,low,gate
  if p==0x42:
   if low is None:low=v
   else:divisor=(low|(v<<8)) or 65536;low=None
  if p==0x61:gate=(v&3)==3
 def hook(u,a,size,data):
  if a==0x13e89:
   u.reg_write(UC_X86_REG_SP,u.reg_read(UC_X86_REG_SP)+2);u.reg_write(UC_X86_REG_IP,u.reg_read(UC_X86_REG_IP)+5)
  if initial and a==0x13e5f:u.emu_stop()
 u.hook_add(UC_HOOK_INSN,out,None,1,0,UC_X86_INS_OUT)
 u.hook_add(UC_HOOK_INSN,lambda *args:0,None,1,0,UC_X86_INS_IN)
 u.hook_add(UC_HOOK_CODE,hook)
 u.emu_start(0x13e3a,0,count=100000);initial=False
 rows=[]
 for tick in range(100000):
  u.reg_write(UC_X86_REG_CS,0x1239);u.reg_write(UC_X86_REG_SP,0xff00);u.mem_write(0x8ff00,struct.pack('<HHH',0,0x9000,0x202))
  u.emu_start(0x13e69,0x90000,count=10000)
  rows.append((int(gate),divisor))
  if not u.mem_read(0x27f92,1)[0]:break
 else:raise RuntimeError('music did not stop')
 return rows
if __name__=='__main__':
 load_original(sys.argv[1])
 game=Path(sys.argv[2])
 lines=['bank\tsong\tticks\tfnv1a64\n']
 for stem in ['TITLE','SOUND0','SOUND1','SOUND2','SOUND3']:
  data=(game/(stem+'.ISS')).read_bytes()
  count=struct.unpack_from('<H',data)[0]
  pos=2+4*count
  for song in range(count):
   size=struct.unpack_from('<I',data,2+4*song)[0]
   rows=trace(data[pos:pos+size])
   digest=14695981039346656037
   for gate,divisor in rows:
    for byte in struct.pack('<BI',gate,divisor):
     digest=((digest^byte)*1099511628211)&0xffffffffffffffff
   lines.append(f'{stem}\t{song}\t{len(rows)}\t{digest:016x}\n')
   pos+=size
 Path(sys.argv[3]).write_text(''.join(lines))
 print(f'{len(lines)-1} songs')
