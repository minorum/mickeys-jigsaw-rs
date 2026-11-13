from pathlib import Path
import argparse
import hashlib
import struct

from unicorn import Uc, UC_ARCH_X86, UC_MODE_16, UC_HOOK_CODE
from unicorn.x86_const import *


def load(path, digest):
    image = Path(path).read_bytes()
    assert hashlib.sha256(image).hexdigest() == digest
    machine = Uc(UC_ARCH_X86, UC_MODE_16)
    machine.mem_map(0, 0x200000)
    header = struct.unpack_from('<H', image, 8)[0] * 16
    machine.mem_write(0x10000, image[header:])
    for index in range(struct.unpack_from('<H', image, 6)[0]):
        entry = struct.unpack_from('<H', image, 24)[0] + index * 4
        offset, segment = struct.unpack_from('<HH', image, entry)
        address = 0x10000 + segment * 16 + offset
        value = struct.unpack('<H', machine.mem_read(address, 2))[0]
        machine.mem_write(address, struct.pack('<H', (value + 0x1000) & 65535))
    return machine


parser = argparse.ArgumentParser()
for name in ('main', 'driver', 'corpus', 'output'):
    parser.add_argument(name)
parser.add_argument('--asset')
parser.add_argument('--frame', type=int)
parser.add_argument('--pixels')
args = parser.parse_args()
if args.pixels and (args.asset is None or args.frame is None):
    parser.error('--pixels requires --asset and --frame')
main_path, driver_path, corpus_path, output_path = args.main, args.driver, args.corpus, args.output
ega_mode = hashlib.sha256(Path(driver_path).read_bytes()).hexdigest() == '6ec038bddd09d17aa331f2f869005d9ed4e2057495adc08628ff7007576674f0'
if ega_mode:
    import ega_sprite_blits as ega_model
    ega_model.initialize(main_path, driver_path)
    ega_model.calibrate()
    gpu = ega_model.ega
else:
    gpu = load(driver_path, '02de74a740e4fb51108e78f2da655983399674b35191a53fb6391dad05772630')
gpu_segment = 0x16df if ega_mode else 0x143c
gpu_base = gpu_segment * 16
gpu_return = 0x78 if ega_mode else 0x74
handlers = struct.unpack('<64H', gpu.mem_read(gpu_base + (0xb0 if ega_mode else 0xce), 128))
main = None
sprites_by_address = {}
draws = 0


def dispatch(machine, address, size, data):
    global draws
    sp = machine.reg_read(UC_X86_REG_SP)
    position = 0x60000 + sp + 4
    while True:
        opcode = struct.unpack('<H', machine.mem_read(position, 2))[0]
        if opcode == 2:
            break
        length, operation = opcode & 255, opcode >> 8
        assert length >= 2 and length % 2 == 0
        command = bytes(machine.mem_read(position, length))
        assert operation in (6, 7, 9, 0x10, 0x11, 0x12, 0x13, 0x14, 0x1c, 0x20), hex(opcode)
        if operation not in (6, 9):
            if operation == 0x13:
                offset, segment = struct.unpack_from('<HH', command, 2)
                record = bytearray(sprites_by_address[segment * 16 + offset])
                assert len(record) < 65536
                struct.pack_into('<HH', record, 6, 26, 0x9000)
                gpu.mem_write(0x90000, bytes(record))
                command = struct.pack('<3H', opcode, 6, 0x9000)
            gpu.mem_write(0x70000, command)
            for reg, value in ((UC_X86_REG_CS, 0x1000), (UC_X86_REG_DS, gpu_segment),
                               (UC_X86_REG_ES, 0x7000), (UC_X86_REG_DI, length),
                               (UC_X86_REG_SS, 0x8000), (UC_X86_REG_SP, 0xff00)):
                gpu.reg_write(reg, value)
            gpu.emu_start(0x10000 + handlers[operation], 0x10000 + gpu_return, count=10000000)
            assert gpu.reg_read(UC_X86_REG_IP) == gpu_return, hex(opcode)
            if operation == 0x20:
                draws += 1
        position += length
    ip, cs = struct.unpack('<HH', machine.mem_read(0x60000 + sp, 4))
    machine.reg_write(UC_X86_REG_SP, sp + 4)
    machine.reg_write(UC_X86_REG_CS, cs)
    machine.reg_write(UC_X86_REG_IP, ip)


rows = ['asset\tframe\tdraws\tfnv64\n']
for path in sorted(Path(corpus_path).glob('scene-P*.PZ4.raw' if ega_mode else 'scene-P*.PZ8.raw')):
    if args.asset is not None and path.name[6:-4] != args.asset:
        continue
    data = path.read_bytes()
    position = 0
    while position < len(data):
        tag = data[position:position + 4]
        size = struct.unpack_from('<I', data, position + 4)[0]
        position += 8
        if tag == (b'TND0' if ega_mode else b'MCG0'):
            chunk = data[position:position + size]
            break
        position += size
    else:
        raise AssertionError(path)
    records, sprites, headers = [], [], []
    position = 0
    while position < len(chunk):
        kind = struct.unpack_from('<H', chunk, position)[0]
        size = struct.unpack_from('<I' if kind & 0x8000 else '<H', chunk, position + 2)[0]
        record = bytearray(chunk[position:position + size])
        assert len(record) == size and size >= 4
        position += size
        if kind == 0x7ff0:
            headers.append(len(records))
        if kind == 0x7ff1:
            offset = 12
            for _ in range(struct.unpack_from('<H', record, 10)[0]):
                command, length = struct.unpack_from('<HH', record, offset)
                if command in (0x60, 0x80, 0x90):
                    resource = struct.unpack_from('<H', record, offset + 8)[0]
                    struct.pack_into('<H', record, offset + 8, max(0, resource - 1))
                offset += length
        if kind in (0x7ff0, 0x7ff1):
            records.append(record)
        elif kind == 0x8061:
            sprites.append(record)
    film = records[headers[1]]
    chosen = set()
    offset = 16
    for _ in range(struct.unpack_from('<H', film, 6)[0]):
        command, length = struct.unpack_from('<HH', film, offset)
        if command in (0x10, 0x11):
            chosen.add(struct.unpack_from('<H', film, offset + 4)[0])
        offset += length
    roots = {headers[1] + 1}
    for sequence in chosen:
        end = headers[sequence + 1] if sequence + 1 < len(headers) else len(records)
        roots.update(range(headers[sequence] + 1, end))
    if args.frame is not None:
        assert args.frame in roots
        roots = {args.frame}
    main = load(main_path, '565a5c5601ede858cfdc289e6dd1f3bbde7c2ec2399e52dad2dce3d1b165d465')
    main.mem_write(0x60000, bytes(main.mem_read(0x280d0, 65536)))
    for offset, value in ((0x1440, 320), (0x1442, 200), (0x16c, len(sprites)),
                          (0x9a0, 0), (0x9ae, 15 if ega_mode else 255)):
        main.mem_write(0x60000 + offset, struct.pack('<H', value))
    sprites_by_address.clear()
    pointers = []
    address = 0x90000
    for table, items in ((0x6c7c4, records), (0x6aeb6, sprites)):
        for index, record in enumerate(items):
            assert address + len(record) < 0xff000
            main.mem_write(address, bytes(record))
            main.mem_write(table + index * 4, struct.pack('<HH', 0, address // 16))
            if table == 0x6c7c4:
                pointers.append(address // 16)
            else:
                sprites_by_address[address + 6] = ega_model.convert(record) if ega_mode else bytes(record)
            address += (len(record) + 15) & ~15
    main.hook_add(UC_HOOK_CODE, dispatch, begin=0x196c8, end=0x196c8)
    for frame in sorted(roots):
        draws = 0
        if ega_mode:
            ega_model.reset(0)
        else:
            gpu.mem_write(0xa0000, bytes(64000))
        for offset, value in ((0xc, 0xa000), (0x14, 320), (0x16, 200), (0x20, 40 if ega_mode else 320),
                              (0x26, 2), (0x28, 2), (0x2a, 270), (0x2c, 196)):
            gpu.mem_write(gpu_base + offset, struct.pack('<H', value))
        if ega_mode:
            gpu.mem_write(gpu_base + 0x5b3, struct.pack('<HH', 2, 2))
        else:
            gpu.mem_write(gpu_base + 0x856, b'\2')
        main.mem_write(0x6ff00, struct.pack('<5H', 0, 0xff00, 0, pointers[frame], 0))
        for reg, value in ((UC_X86_REG_CS, 0x196c), (UC_X86_REG_DS, 0x6000),
                           (UC_X86_REG_SS, 0x6000), (UC_X86_REG_SP, 0xff00)):
            main.reg_write(reg, value)
        main.emu_start(0x1ae63, 0xff000, count=10000000)
        assert main.reg_read(UC_X86_REG_CS) == 0xff00, (path.name, frame)
        checksum = 0xcbf29ce484222325
        result_pixels = ega_model.pixels if ega_mode else gpu.mem_read(0xa0000, 64000)
        if args.pixels:
            Path(args.pixels).write_bytes(bytes(result_pixels))
        for byte in result_pixels:
            checksum = ((checksum ^ byte) * 0x100000001b3) & 0xffffffffffffffff
        rows.append(f'{path.name[6:-4]}\t{frame}\t{draws}\t{checksum:016x}\n')
    print(f'{path.name}: {len(roots)} display frames rendered', flush=True)
Path(output_path).write_text(''.join(rows))
print(f'{len(rows) - 1} native {"EGA" if ega_mode else "VGA"} scene framebuffer hashes')
