Currently Rimeara uses Clang to make the object code
into Native machine code. This allows Rimera to run anywhere easily.

[Rimera IR] -> [Cranelit] -> [Mach Objects] -Linker-> [Native]

This we recommend using for the linker
1) Zig cc (best clang wrapper)
2) Gcc (OpenSource Hero)
3) MSVC (Shitty microsoft)

pyproject.toml
```toml
[tools.rimera]
linker="zig cc"
custom_path="" # custom command to run to link
```

