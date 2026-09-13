# Gate 11 — Dynamic Compilation, `compile`, `eval`, and `exec`

Gate 11 adds capability-governed runtime compilation through Rimera's existing
parser, semantic analysis, verified MIR, and Cranelift path. It must not execute
Python bytecode, interpret AST nodes, shell out to CPython, or create a weaker
dynamic semantics path.

## Progress ledger

- [x] Slice 1 — modes, code-object contract, capabilities, and oracle matrix
- [x] Slice 2 — `compile` sources, flags, diagnostics, and code metadata
- [x] Slice 3 — `eval` globals/locals, builtins injection, and closure reads
- [x] Slice 4 — `exec` namespaces, writes, declarations, and class-body composition
- [x] Slice 5 — dynamic cache keys, linking/loading, lifetime, and reclamation
- [x] Slice 6 — nested dynamic code, modules, reflection, async, and exceptions
- [x] Slice 7 — adversarial inputs, resource limits, atomicity, and artifact proof
- [x] Slice 8 — final audit, documentation, and gate closure

## Slice acceptance

1. Define `exec`/`eval`/`single` modes, code-object ownership, accepted flags,
   filename/line metadata, and an explicit build/runtime capability policy.
2. Compile strings and supported source objects with Python-shaped syntax
   failures and immutable managed code metadata.
3. Match namespace selection, builtins insertion, return value, name lookup,
   closure restrictions, and error behavior for `eval`.
4. Match writes and deletions through supplied mappings for `exec`, including
   declarations and interactions with class/module namespace rules.
5. Load generated native code into the owning context with deterministic cache
   keys, exact roots, unload/reclamation policy, and no stale callable pointers.
6. Compose dynamic code with imports, frames, tracebacks, generators/coroutines,
   descriptors, callbacks, and nested dynamic compilation.
7. Enforce size/depth/capability limits before publication and prove failure
   leaves namespaces, caches, and output artifacts coherent.
8. Run the complete dynamic corpus and scan for forbidden interpreter paths.

Static builds that do not grant dynamic compilation must omit this machinery.

All eight slices are closed. Gate 11 is `Implemented — conformance audit
pending` for its documented capability-enabled subset. Gate 12 Slices 1–3 are
closed and Slice 4 is the active compatibility dependency.
