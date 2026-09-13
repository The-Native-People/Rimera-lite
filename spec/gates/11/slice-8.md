# Gate 11 Slice 8 — final audit and gate closure

Gate 11 closes only for the explicitly documented capability-enabled subset.
It is not a general Python 3.12 dynamic-code compatibility claim.

## Final audit

The focused Gate 11 suite runs four public native tests covering all seven
fixtures from Slices 1–7. Supported semantic cases compare stdout and exit
status with CPython 3.12.11 at normal or constrained managed heaps. Product
resource ceilings use exact Rimera diagnostics and recovery assertions.

Every capability-enabled artifact is scanned for CPython, generated-C,
compatibility-interpreter, `setjmp`, and `longjmp` symbols. A capability-free
synchronous artifact is separately scanned to prove
`rimera_dynamic_compiler_install` and `compile_native` are absent. The existing
capability-denial case proves no output artifact is published.

Runtime tests independently cover exact-key reuse, distinct code-object
identity, post-GC native-owner destruction, recompilation after reclamation,
source/native-unit pre-publication limits, and weak-key association fixed-point
behavior. Compiler tests continue to verify all modes, immutable metadata,
optimization, syntax diagnostics, and verified MIR.

## Closed surface and remaining boundaries

Gate 11 closes native `compile`, `eval`, and `exec` for the sources, flags,
namespaces, closures, cache ownership, resource limits, and already-supported
cross-feature composition listed by Slices 1–7. AST input, nonzero `PyCF_*`
flags, top-level await flags, arbitrary stdlib imports, unsupported syntax,
and general Python 3.12 compatibility remain outside this promotion. Later
gates must extend the same compiler, IR, runtime, exception, object, and module
models rather than introduce another dynamic executor.
