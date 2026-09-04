# Gate 8 Slice 7 — Composition, GC, Failure Atomicity, and Artifact Proof

## Goal

Prove context managers compose with the complete synchronous core under
callbacks, exceptions, suspension, collection, and allocation failure.

## Integrated work

- Combine classes/metaclasses/descriptors, recursive targets, expanded calls,
  comprehensions, pattern matching, exception groups, reflection, custom buffer
  providers, generators, delegation, and nested cleanup inside manager paths.
- Stress reentrancy and class/namespace mutation while captured exit methods,
  entered values, pending completions, and exception triples remain live.
- Force GC at lookup, entry, target, body, exit, truth, suspension, and unwind
  safepoints; live state survives and completed/failed manager graphs collect.
- Force `MemoryError` throughout partial entry and cleanup; prior visible state
  remains valid and each successful entry receives exactly one required exit.
- Run all fixtures through the public build API and scan final artifacts for
  CPython, generated C, `setjmp`, and `longjmp`.

## Completion proof

A compact composition corpus matches CPython 3.12.11 stdout, stderr, status,
tracebacks, identities, and side-effect logs under normal and constrained heaps,
with audited MIR roots and ABI reachability.
