"""Run the original DA decoder. Requires Unicorn and the unpacked executable."""

import argparse
import hashlib
import struct
from pathlib import Path

from unicorn import Uc, UC_ARCH_X86, UC_HOOK_CODE, UC_MODE_16
from unicorn.x86_const import (
    UC_X86_REG_AX,
    UC_X86_REG_CS,
    UC_X86_REG_DS,
    UC_X86_REG_IP,
    UC_X86_REG_SP,
    UC_X86_REG_SS,
)


def decode(executable, asset):
    program = executable.read_bytes()
    assert hashlib.sha256(program).hexdigest() == (
        "565a5c5601ede858cfdc289e6dd1f3bbde7c2ec2399e52dad2dce3d1b165d465"
    )
    header_size = struct.unpack_from("<H", program, 8)[0] * 16
    machine = Uc(UC_ARCH_X86, UC_MODE_16)
    machine.mem_map(0, 0x200000)
    machine.mem_write(0x10000, program[header_size:])
    relocations = struct.unpack_from("<H", program, 6)[0]
    table = struct.unpack_from("<H", program, 24)[0]
    for index in range(relocations):
        offset, segment = struct.unpack_from("<HH", program, table + 4 * index)
        address = 0x10000 + 16 * segment + offset
        value = struct.unpack("<H", machine.mem_read(address, 2))[0]
        machine.mem_write(address, struct.pack("<H", (value + 0x1000) & 0xFFFF))

    def write_word(offset, value):
        machine.mem_write(0x280D0 + offset, struct.pack("<H", value & 0xFFFF))

    def read_word(offset):
        return struct.unpack("<H", machine.mem_read(0x280D0 + offset, 2))[0]

    for pointers, segment in (
        ([0xC052, 0xC056, 0xC05E, 0xC0E4, 0xC0EA, 0xC0EE, 0x8E9E, 0xAEA6], 0x5000),
        ([0xC0F2, 0xC0F6, 0xC0FE, 0xC336, 0x85BA, 0x868A, 0x8E92, 0x8E96], 0x6000),
    ):
        for offset, pointer in enumerate(pointers):
            write_word(pointer, offset * 0x100)
            write_word(pointer + 2, segment)
    write_word(0xAEAC, 0x800)
    write_word(0xAEAE, 0x5000)
    data = asset.read_bytes()
    assert data[:4] == b"DA\0\0"
    assert int.from_bytes(data[10:12], "little") == 1
    expected = int.from_bytes(data[16:20], "little")
    write_word(0x84B2, expected)
    write_word(0x84B4, expected >> 16)
    write_word(0x83C4, 0)
    write_word(0xC5C0, int.from_bytes(data[8:10], "little"))
    position = 20
    output = bytearray()
    eof_calls = 0
    close_errors = []
    machine.reg_write(UC_X86_REG_DS, 0x280D)
    machine.reg_write(UC_X86_REG_SS, 0x8000)
    machine.reg_write(UC_X86_REG_SP, 0xFF00)
    machine.reg_write(UC_X86_REG_CS, 0x265F)
    machine.reg_write(UC_X86_REG_IP, 0x258)
    machine.mem_write(0x8FF00, struct.pack("<HH", 0, 0xF000))

    def stack_address():
        return machine.reg_read(UC_X86_REG_SP) + 16 * machine.reg_read(UC_X86_REG_SS)

    def return_far(value):
        ip, cs = struct.unpack("<HH", machine.mem_read(stack_address(), 4))
        machine.reg_write(UC_X86_REG_SP, machine.reg_read(UC_X86_REG_SP) + 4)
        machine.reg_write(UC_X86_REG_AX, value & 0xFFFF)
        machine.reg_write(UC_X86_REG_CS, cs)
        machine.reg_write(UC_X86_REG_IP, ip)

    def hook(_machine, address, _size, _context):
        nonlocal position, eof_calls
        if address == 0x267B6:
            if position < len(data):
                value = data[position]
                position += 1
            else:
                value = 0xFFFF
                eof_calls += 1
            return_far(value)
        elif address == 0x27066:
            offset, count = struct.unpack("<HH", machine.mem_read(stack_address() + 4, 4))
            buffer = read_word(0x8E9A) + 16 * read_word(0x8E9C)
            output.extend(machine.mem_read(buffer + offset, count))
            return_far(0)
        elif address == 0x26787:
            error = struct.unpack("<H", machine.mem_read(stack_address() + 4, 2))[0]
            close_errors.append(error)
            write_word(0x83C4, 0xFFFF)
            return_far(error)
        elif address == 0xF0000:
            machine.emu_stop()

    machine.hook_add(UC_HOOK_CODE, hook)
    machine.emu_start(0x26848, 0xF0000, count=40_000_000)
    assert machine.reg_read(UC_X86_REG_CS) == 0xF000
    digest = hashlib.sha256(output).hexdigest()
    forms = [index for index in range(len(output)) if output[index:index + 4] == b"FORM"]
    print(f"{asset.name}: input={position}/{len(data)} eof_calls={eof_calls}")
    print(f"output={len(output)} declared={expected} close_errors={close_errors}")
    print(f"sha256={digest}")
    print(f"FORM offsets={forms}")
    return output


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("unpacked_executable", type=Path)
    parser.add_argument("asset", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    result = decode(args.unpacked_executable, args.asset)
    if args.output:
        args.output.write_bytes(result)
