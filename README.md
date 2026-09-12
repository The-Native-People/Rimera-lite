# Rimera (lite)
### Rimera's goal is not to reimplement compiled python, we just want a performance first alternative that supports python
PS; in my humble 16 year old opinon, agents write most of our code, so why change the syntax? why change whats already there? Instead let's build on these primitives with a better foundation.

Rimera is a clean native Python compiler. Its implemented path is:

```text
Python source -> RustPython syntax adapter -> HIR -> verified SSA MIR
 -> LIR -> Cranelift Mach-O object -> Rust runtime archive -> executable
```

It does not generate C, embed CPython, or run Python bytecode. The current
macOS arm64 milestone supports values, locals, arbitrary-size integers,
strings, arithmetic, comparisons, `print`, `if`/`else`, and `while`.

Build the compiler and runtime, then compile a source file:

```shell
cargo build -p rimera-runtime -p rimera-cli
target/debug/rimera-lite build tests/fixtures/basic/core.py -o dist/core
./dist/core
```

For a compile-and-run shortcut, pass a source entry followed by `--run`.
Rimera writes this temporary executable under that source directory's
`.rimera/bin/` cache and forwards the program's exit status:

```shell
cargo run tests/fixtures/basic/core.py --run
# equivalently: target/debug/rimera-lite tests/fixtures/basic/core.py --run
```

`cargo run -- tests/fixtures/basic/core.py --run` is also accepted when you
prefer Cargo's explicit argument separator.
An explicit artifact path remains available when you want to keep the binary:

```shell
target/debug/rimera-lite build tests/fixtures/basic/core.py -o dist/core --run
```

Use `--debug` to print the native build trace and write a readable `.ir.py`
representation under `.rimera/ir/`:

```shell
target/debug/rimera-lite build tests/fixtures/basic/core.py -o dist/core --debug
```

Use `--heap-limit-bytes` to embed an optional managed-memory budget. Zero or an
omitted option leaves the context unlimited:

```shell
target/debug/rimera-lite build tests/fixtures/basic/core.py -o dist/core --heap-limit-bytes 1048576
```

To make a real, machine-calibrated Python source fixture for manually checking
the build loader, generate the approximately 20-second compile stress input:

```shell
python3 scripts/generate_compile_stress.py --target-seconds 20
target/debug/rimera-lite build tests/fixtures/stress/compile_stress.py -o /tmp/rimera-stress --debug
```

The generated stress source is intentionally ignored by Git; it is not part of
the normal test suite.

Rimera stores native intermediates under `<project>/.rimera/objects/`. Its
supported environment configuration is documented in
[the configuration spec](spec/configuration.md).

Read [the specification](spec/architecture.md), the
[native ABI](spec/abi-v1.md), and the
[compatibility ledger](spec/compatibility.md), and the
[milestone evidence](spec/foundation.md) before changing
architecture.

`Trash/` retains the previous implementation for reference only.

ps; why call it Rimera lite? Rimera makes an effort to debloat everything. Its not saying Rimera is cheap, we want rimera to create small miracles.
