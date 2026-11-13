from pathlib import Path
import struct,sys
from unicorn import Uc,UC_ARCH_X86,UC_MODE_16,UC_HOOK_CODE
from unicorn.x86_const import *
code = b''
def load_original(path):
 global code
 import hashlib
 image=Path(path).read_bytes()
 assert hashlib.sha256(image).hexdigest() == '565a5c5601ede858cfdc289e6dd1f3bbde7c2ec2399e52dad2dce3d1b165d465'
 header=struct.unpack_from('<H',image,8)[0]*16
 code=image[header+0x13630:header+0x13630+0x2800]
def decode(bank,phrase=0):
 u=Uc(UC_ARCH_X86,UC_MODE_16);u.mem_map(0,0x200000);u.mem_write(0x20000,code);u.mem_write(0x50000,bank);u.mem_write(0x23000,bytes(range(256)))
 bp=0xa;pc=struct.unpack_from('<I',bank,4+phrase*4)[0]
 def w(off,n):u.mem_write(0x20000+bp+off,struct.pack('<H',n))
 def b(off,n):u.mem_write(0x20000+bp+off,bytes([n]))
 w(0x2f,0xdc7);w(0x2b,pc&15);w(0x2d,0x5000+(pc>>4));w(0x39,0);w(0x3b,0x5000);w(0x36,0x3000);b(0x35,1)
 for reg,v in [(UC_X86_REG_CS,0x2000),(UC_X86_REG_BP,bp),(UC_X86_REG_ES,0x8000),(UC_X86_REG_SS,0x9000)]:u.reg_write(reg,v)
 done=False
 def hook(u,address,size,data):
  nonlocal done
  if address==0x21172:done=True;u.emu_stop()
 u.hook_add(UC_HOOK_CODE,hook)
 out=bytearray()
 for _ in range(3000):
  u.reg_write(UC_X86_REG_DI,0);u.reg_write(UC_X86_REG_SP,0xfffc);u.mem_write(0x9fffc,struct.pack('<H',0x3100))
  u.emu_start(0x20db4,0x23100,count=2000000)
  n=u.reg_read(UC_X86_REG_DI);out+=u.mem_read(0x80000,n)
  if done:break
 else:raise RuntimeError('phrase did not terminate')
 return out
if __name__=='__main__':
 load_original(sys.argv[1])
 game=Path(sys.argv[2])
 banks={p.stem:p.read_bytes() for p in sorted(game.glob('*.NNV'))}
 for stem in ['PROG','DATA']:
  data=(game/(stem+'.ESS')).read_bytes()
  count=struct.unpack_from('<H',data)[0]
  pos=2+4*count
  for i in range(count):
   size=struct.unpack_from('<I',data,2+4*i)[0]
   banks[f'{stem}_{i}']=data[pos:pos+size]
   pos+=size
 rows=['bank\tphrase\tsamples\tfnv1a64\n']
 for stem,bank in banks.items():
  for phrase in range(struct.unpack_from('<I',bank)[0]):
   pcm=decode(bank,phrase)
   digest=14695981039346656037
   for sample in pcm:
    digest=((digest^sample)*1099511628211)&0xffffffffffffffff
   rows.append(f'{stem}\t{phrase}\t{len(pcm)}\t{digest:016x}\n')
 Path(sys.argv[3]).write_text(''.join(rows))
 print(f'{len(banks)} banks, {len(rows)-1} phrases')
