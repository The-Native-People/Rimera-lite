# Gate 7 Slice 9 — Mutation Invalidation, Composition, GC, and Failure Atomicity

## Goal

Prove reflective reads and writes compose with the full synchronous object model
and never expose stale caches or corrupt managed state.

## Integrated work

- Invalidate class, descendant, descriptor, method, type-relation, `dir`, and
  method-table observations after reflective namespace/base mutations.
- Combine namespace views, function/code/cell metadata, exception frames,
  suspended generators, type parameters, metaclass hooks, descriptors, and
  Python buffer providers in callback-heavy fixtures.
- Stress reentrant reflection while lookups, mutations, frame views, buffer
  exports, exception graphs, and generator states are live.
- Force GC and `MemoryError` at reflective allocation/call/mutation safepoints;
  failed changes remain atomic and previously published views stay valid under
  their documented semantics.
- Run public artifacts through forbidden CPython/generated-C/`setjmp`/`longjmp`
  symbol scans and audit reachable ABI exports.

## Completion proof

A compact composition corpus matches CPython 3.12.11 values, identities,
ordering, side effects, errors, and tracebacks under normal and constrained
heaps, including cache-invalidation regressions.
