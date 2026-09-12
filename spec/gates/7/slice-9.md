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

## Implemented contract

- Reflective class namespace/name/base mutation invalidates the mutated type,
  descendants inheriting through it, and classes observing it through a
  metaclass MRO. The version tag remains the single future-cache invalidation
  owner; lookup itself remains the existing dynamic descriptor/MRO path.
- Legacy string-key namespace growth and string-backed class/function/generator
  metadata preflight retained managed growth before publication when a hard heap
  limit would be crossed. A failed `MemoryError` leaves the old value, hierarchy,
  namespace, and version tags visible; no mutate-then-rollback reentrancy window
  is published.
- `__bases__` validation failures surface as managed `TypeError` while preserving
  callback exceptions and the already-transactional descendant MRO plan.
- The composition fixture keeps descriptors, type relations, namespace views,
  code/closure cells, a suspended generator frame, PEP 695 metadata, a PEP 688
  provider lease, and reflective class mutation live together under allocation
  pressure.

## Acceptance evidence — 2026-09-04

- `gate7_slice9_reflection_mutation_composition_matches_cpython_312_under_gc_pressure`
  matches CPython 3.12.11 under a 192,000-byte managed heap limit.
- Runtime proofs `gate7_reflective_type_mutation_invalidates_descendants_and_is_low_heap_atomic`
  and `gate7_reflective_string_metadata_growth_is_low_heap_atomic` pin descendant
  invalidation and zero-headroom mutation atomicity.
- The complete Gate 7 public reflection filter is 20/20 green and the Gate 7
  runtime filter is 11/11 green after retaining the older Slice 3 128 KiB
  low-heap contract.

## DONE BY CHATGPT
