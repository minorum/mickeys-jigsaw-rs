from pathlib import Path
import hashlib,struct,sys,json
from unicorn import Uc,UC_ARCH_X86,UC_MODE_16,UC_HOOK_INSN
from unicorn.x86_const import *
image=Path(sys.argv[1]).read_bytes()
assert hashlib.sha256(image).hexdigest()=='565a5c5601ede858cfdc289e6dd1f3bbde7c2ec2399e52dad2dce3d1b165d465'
header=struct.unpack_from('<H',image,8)[0]*16
u=Uc(UC_ARCH_X86,UC_MODE_16);u.mem_map(0,0x100000);u.mem_write(0x10000,image[header:])
for register,value in [(UC_X86_REG_CS,0x1239),(UC_X86_REG_SS,0x8000),(UC_X86_REG_SP,0xff00)]:u.reg_write(register,value)
u.mem_write(0x8ff00,struct.pack('<HH',0,0x9000))
writes=[]
def output(u,port,size,value,data):
 assert port==0x61
 writes.append(value)
u.hook_add(UC_HOOK_INSN,lambda *args:0,None,1,0,UC_X86_INS_IN)
u.hook_add(UC_HOOK_INSN,output,None,1,0,UC_X86_INS_OUT)
u.emu_start(0x13985,0x90000,count=2000000)
assert u.reg_read(UC_X86_REG_CS)==0x9000
assert writes==[0,2]*80+[0],writes
print(json.dumps({'speaker_cycles':80,'port_writes':len(writes),'restored_gate':writes[-1]}))
