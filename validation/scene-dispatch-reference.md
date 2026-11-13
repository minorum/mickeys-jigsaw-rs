# Scene dispatch reference

The independently unpacked original executable has SHA-256
`565a5c5601ede858cfdc289e6dd1f3bbde7c2ec2399e52dad2dce3d1b165d465`.
Original top-level dispatcher `FUN_196c_17a3` handles sound 0020, delay 0030 and
subframe 0040. Its nested renderer `FUN_196c_14f0` handles 0040, 0050, 0060,
0080 and 0090. Both advance over other commands by their declared record size.
Neither renders command 00a0, found in P03 EGA frame 189. The Rust parser's
existing skip behavior is therefore consistent with these original dispatchers.

`oracles/scene_dispatch.py` executes the actual relocated x86 instructions with
injected frame records. It intercepts the low-level graphics dispatcher and
records calls at the nested renderer and rectangle drawing boundary. For both
dispatchers, 00a0 generates no drawing call; positive controls 0040 (top level)
and 0050 (nested) do generate their expected calls. All cases return normally.
This tests dispatch behavior, not the pixel output of those intercepted callees.

Reproduce:

```sh
python3 validation/oracles/scene_dispatch.py /path/to/unpacked.exe
```

Subsequent machine-code testing found a child-flag mismatch in recursive bounds
measurement despite the structural similarity noted during static review. It is
now repaired and covered by all 64 three-level flip combinations. See
`frame-bounds-reference.md`. Full sprite pixel and hotspot parity remains unproven.
