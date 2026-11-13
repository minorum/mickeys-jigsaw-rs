from pathlib import Path
import hashlib,struct,sys
image=Path(sys.argv[1]).read_bytes()
assert hashlib.sha256(image).hexdigest()=='565a5c5601ede858cfdc289e6dd1f3bbde7c2ec2399e52dad2dce3d1b165d465'
header=struct.unpack_from('<H',image,8)[0]*16
base=header+0x180d0
archives=[]
for index in range(2):
 offset,segment=struct.unpack_from('<HH',image,base+0x63fa+index*4)
 archives.append(image[header+segment*16+offset:].split(b'\0',1)[0].decode('ascii'))
assert archives==['PROG.ESS','DATA.ESS']
rows=['event\tarchive\tresource\tphrase\tcaption\n']
for event in range(15):
 bank,resource,phrase,offset,segment=struct.unpack_from('<5H',image,base+0x1dd0+event*10)
 assert bank<len(archives)
 caption=image[header+segment*16+offset:].split(b'\0',1)[0].decode('ascii') if offset or segment else ''
 rows.append(f'{event}\t{archives[bank]}\t{resource}\t{phrase}\t{caption.replace(chr(13), chr(92)+"r")}\n')
Path(sys.argv[2]).write_text(''.join(rows))
print('15 original feedback entries')
