# Original resource index handling

Static inspection used Ghidra 12.2 and the unpacked original executable with
SHA-256 `565a5c5601ede858cfdc289e6dd1f3bbde7c2ec2399e52dad2dce3d1b165d465`.

The generic selected-resource loader `FUN_2747_0095` (linear `0x27505`, original
relative segment `1747:0095`) uses a direct zero-based resource index. After
opening the file, it initializes its result to 9, reads the resource count,
and proceeds only if `index < count`. It sums sizes before the selected index,
stores that entry's size, and skips exactly that accumulated length. An index
equal to or greater than the count closes the file and returns 9. There is no
clamp, modulo, or last-resource fallback in this routine.

This alone does **not** prove the behavior of an out-of-range ISS song index.
Its observed direct callers are image/toolbox loaders `FUN_1701_0004` and
`FUN_1cd2_0631`. Scene audio preloading in `FUN_1cd2_0a74` instead opens and reads
the entire named audio file through `FUN_17ac_0018`, `FUN_17ac_090f`, and
`FUN_17ac_00ea`.

`SOUND3.ISS` begins `02 00 3c 01 00 00 56 02 00 00`: its container has exactly
two resources, with lengths 316 and 598. Therefore resource index 2 is absent
from this supplied file.

The actual ISS selected-song loader was subsequently traced independently:
`FUN_22a0_0454` (linear `0x22e54`) checks the direct zero-based song index against
the container count. It sums preceding sizes and copies the selected song, up
to `0x960` bytes, before calling `FUN_1239_1aaa`. An out-of-range index does
nothing; there is no clamp or renumbering. Thus the original also skips P11's
absent SOUND3 song 2. See `iss-oracle.md` for the separate synthesis proof.
