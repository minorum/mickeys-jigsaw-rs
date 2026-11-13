from pathlib import Path
import hashlib
import struct
import sys

from unicorn import Uc, UC_ARCH_X86, UC_MODE_16, UC_HOOK_MEM_READ, UC_HOOK_MEM_WRITE, UC_HOOK_INSN
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


main = ega = None
gc, sequencer = [0] * 9, [0] * 5
gc_index = seq_index = 0
latched = -1
pixels = bytearray()
write_count = 0


def output(machine, port, size, value, data):
    global gc_index, seq_index
    assert port in (0x3c4, 0x3c5, 0x3ce, 0x3cf), hex(port)
    if port == 0x3ce:
        gc_index = value & 255
        assert gc_index < len(gc)
        if size == 2:
            gc[gc_index] = value >> 8
    elif port == 0x3cf:
        assert size == 1
        gc[gc_index] = value
    elif port == 0x3c4:
        seq_index = value & 255
        assert seq_index < len(sequencer)
        if size == 2:
            sequencer[seq_index] = value >> 8
    else:
        assert size == 1
        sequencer[seq_index] = value


def read(machine, access, address, size, value, data):
    global latched
    assert size == 1 and 0xa0000 <= address < 0xa0000 + 8000
    latched = address
    start = (address - 0xa0000) * 8
    result = 0
    for bit in range(8):
        color = pixels[start + bit]
        match = ((color ^ gc[2]) & gc[7]) == 0 if gc[5] & 8 else color & (1 << gc[4]) != 0
        if match:
            result |= 128 >> bit
    machine.mem_write(address, bytes([result]))


def write(machine, access, address, size, value, data):
    global write_count, latched
    assert gc[5] & 3 == 2 and gc[3] & 0x18 == 0 and sequencer[2] == 15
    assert size in (1, 2) and 0xa0000 <= address < address + size <= 0xa0000 + 8000
    assert gc[8] == 255 or (size == 1 and latched == address)
    for offset in range(size):
        color = (value >> (offset * 8)) & 15
        start = (address + offset - 0xa0000) * 8
        for bit in range(8):
            if gc[8] & (128 >> bit):
                pixels[start + bit] = color
    write_count += size
    latched = -1


def initialize(main_path, driver_path):
    global main, ega
    main = load(main_path, '565a5c5601ede858cfdc289e6dd1f3bbde7c2ec2399e52dad2dce3d1b165d465')
    ega = load(driver_path, '6ec038bddd09d17aa331f2f869005d9ed4e2057495adc08628ff7007576674f0')
    ega.hook_add(UC_HOOK_INSN, output, None, 1, 0, UC_X86_INS_OUT)
    ega.hook_add(UC_HOOK_MEM_READ, read, begin=0xa0000, end=0xaffff)
    ega.hook_add(UC_HOOK_MEM_WRITE, write, begin=0xa0000, end=0xaffff)
    for y in range(200):
        ega.mem_write(0x16df0 + 0x703 + y * 2, struct.pack('<H', y * 40))
    ega.mem_write(0x8ff00, struct.pack('<H', 0x78))
    for reg, value in ((UC_X86_REG_CS, 0x1000), (UC_X86_REG_SS, 0x8000),
                       (UC_X86_REG_SP, 0xff00)):
        ega.reg_write(reg, value)
    ega.emu_start(0x133b2, 0x10078, count=100000)
    assert ega.reg_read(UC_X86_REG_IP) == 0x78
    assert bytes(ega.mem_read(0x132b2, 256)) == bytes(int(f'{value:08b}'[::-1], 2) for value in range(256))

def convert(record):
    record = bytearray(record)
    struct.pack_into('<HH', record, 6, 26, 0x5000)
    main.mem_write(0x50000, bytes(record))
    main.mem_write(0x90000, bytes(65536))
    main.mem_write(0x8ff00, struct.pack('<6H', 0, 0xff00, 0, 0x5000, 0, 0x9000))
    for reg, value in ((UC_X86_REG_CS, 0x1239), (UC_X86_REG_DS, 0x280d),
                       (UC_X86_REG_SS, 0x8000), (UC_X86_REG_SP, 0xff00)):
        main.reg_write(reg, value)
    main.emu_start(0x1305d, 0xff000, count=10000000)
    assert main.reg_read(UC_X86_REG_CS) == 0xff00
    size = struct.unpack('<I', main.mem_read(0x90002, 4))[0]
    assert 26 < size <= 65536
    return bytes(main.mem_read(0x90000, size))


def reset(color):
    global pixels, latched
    pixels = bytearray([color]) * 64000
    latched = -1
    gc[:] = [0] * len(gc)
    gc[7], gc[8] = 15, 255
    sequencer[:] = [0] * len(sequencer)
    ega.mem_write(0xa0000, bytes([255 if color else 0]) * 8000)


def draw(converted, x, y, flags, clip):
    reset(15)
    ega.mem_write(0x90000, converted)
    ega.mem_write(0x16e42, converted[6:26])
    for offset, value in ((0x20, 40), (0x74, 1), (0x50, (flags >> 4) & 15),
                          (0x4c, flags >> 8), (0x26, clip[0]), (0x28, clip[1]),
                          (0x2a, clip[2]), (0x2c, clip[3]), (0xc, 0xa000),
                          (0x5b3, 2), (0x5b5, 2), (0x5cb, flags & 3)):
        ega.mem_write(0x16df0 + offset, struct.pack('<H', value))
    ega.mem_write(0x8ff00, struct.pack('<H', 0x78))
    for reg, value in ((UC_X86_REG_CS, 0x1000), (UC_X86_REG_DS, 0x16df),
                       (UC_X86_REG_SS, 0x8000), (UC_X86_REG_SP, 0xff00),
                       (UC_X86_REG_AX, x & 65535), (UC_X86_REG_BX, y & 65535)):
        ega.reg_write(reg, value)
    ega.emu_start(0x12104, 0x10078, count=10000000)
    assert ega.reg_read(UC_X86_REG_IP) == 0x78
    return pixels


def calibrate():
    calibration = bytearray(34)
    struct.pack_into('<HI', calibration, 0, 0x8061, len(calibration))
    struct.pack_into('<8H', calibration, 10, 8, 2, 0, 4, 0, 0, 8, 2)
    calibration[26:] = bytes.fromhex('1230456789abcdef')
    converted = convert(calibration)
    for flags in range(4):
        actual = draw(converted, 3, 4, flags, (0, 0, 320, 200))
        expected = bytearray([15]) * 64000
        colors = [color for byte in calibration[26:] for color in (byte >> 4, byte & 15)]
        for y in range(2):
            for x in range(8):
                color = colors[(1 - y if flags & 2 else y) * 8 + (7 - x if flags & 1 else x)]
                if color:
                    expected[(y + 4) * 320 + x + 3] = color
        assert actual == expected, ('calibration', flags, [(i % 320, i // 320, a, b) for i, (a, b) in enumerate(zip(actual, expected)) if a != b][:20])
    print('Masked-write model calibrated against known pixels in all four orientations', flush=True)


if __name__ == '__main__':
    initialize(sys.argv[1], sys.argv[2])
    calibrate()
    rows = ['asset\tsprite\tx\ty\tflags\tclip_x\tclip_y\tclip_w\tclip_h\tfnv64\n']
    for path in sorted(Path(sys.argv[3]).glob('scene-P*.PZ4.raw')):
        data = path.read_bytes()
        cursor = 0
        while cursor < len(data):
            tag = data[cursor:cursor + 4]
            size = struct.unpack_from('<I', data, cursor + 4)[0]
            cursor += 8
            if tag == b'TND0':
                chunk = data[cursor:cursor + size]
                break
            cursor += size
        else:
            raise AssertionError(path)
        cursor = sprite = 0
        while cursor < len(chunk):
            kind = struct.unpack_from('<H', chunk, cursor)[0]
            size = struct.unpack_from('<I' if kind & 0x8000 else '<H', chunk, cursor + 2)[0]
            record = chunk[cursor:cursor + size]
            assert len(record) == size and size >= 4
            cursor += size
            if kind != 0x8061:
                continue
            converted = convert(record)
            width, height = struct.unpack_from('<HH', record, 22)
            cases = [(23, 19, 0, (0, 0, 320, 200)),
                     (-width // 2, 20, 1, (2, 2, 270, 196)),
                     (311, 195, 2, (0, 0, 320, 200)),
                     (4, -height // 2, 3, (2, 2, 270, 196))]
            if sprite == 0:
                cases += [(23, 19, mode | flip, (0, 0, 320, 200))
                          for mode in (0x10, 0x720) for flip in range(4)]
            for x, y, flags, clip in cases:
                checksum = 0xcbf29ce484222325
                for byte in draw(converted, x, y, flags, clip):
                    checksum = ((checksum ^ byte) * 0x100000001b3) & 0xffffffffffffffff
                rows.append('\t'.join(map(str, (path.name[6:-4], sprite, x, y, flags, *clip))) + f'\t{checksum:016x}\n')
            sprite += 1
        print(f'{path.name}: {sprite} converted sprites drawn', flush=True)
    Path(sys.argv[4]).write_text(''.join(rows))
    print(f'{len(rows) - 1} native EGA full-frame blit hashes; {write_count} masked byte writes')
