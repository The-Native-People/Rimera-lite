To make Rimera fast, like LuaJIT and etc we need to take AOT compilation seriously.

1) Use expensive anayalsis aot, allowing the final application to run optimized.
2) No startup times.

# Current benchmark findings

Benchmark fixture:

```text
tests/fixtures/basic/runtime_benchmark.py
```

The benchmark only uses functionality currently supported by both Rimera and CPython:

```text
integer arithmetic
for + range loops
function calls
list indexing + mutation
dict lookup + mutation
branching
basic user classes
method calls
attribute reads/writes
```

No imports, stdlib modules, generators, async, or unsupported Python functionality are used.

Both runtimes produce the exact same result:

```text
iterations 200000
integer 9599725
function 23948380
list 5002150020
dict 12500750000
object 5000099993
checksum 22536548118
```

## Runtime benchmark

The Rimera binary was compiled first in release mode. Compiler/build time is not included in the runtime benchmark.

Measured on the current development machine:

```text
CPython 3.12 median: 0.086431s
Rimera release median: 13.268742s
```

Current Rimera is roughly:

```text
153x slower than CPython 3.12
```

This benchmark is intentionally useful as a baseline, not a performance claim.

Rimera currently lowers Python semantics to native code, but most operations still travel through generic managed `RValue` and runtime protocol paths. Native compilation alone does not make these operations fast.

The main optimization target is therefore removing unnecessary runtime work through AOT specialization:

```text
unboxed numeric locals
specialized integer operations
specialized loops
function inlining
attribute shape/layout specialization
bounds check elimination
allocation elimination
fewer generic runtime calls
```

## Startup benchmark

Startup is already significantly better than CPython.

Using the tiny `hello.py` fixture and benchmarking the already-built Rimera executable:

```text
CPython 3.12 hello median: 16.215 ms
Rimera hello median:       1.704 ms
```

Minimum measured runs:

```text
CPython 3.12: 15.632 ms
Rimera:        1.615 ms
```

Rimera currently starts roughly:

```text
9.5x faster than CPython 3.12
```

This means startup is not currently the main performance problem.

The important result is:

```text
Startup            -> already very good
Generic execution  -> currently very slow
AOT specialization -> primary performance target
```
