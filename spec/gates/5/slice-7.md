# Gate 5 Slice 7 — Cross-Feature Composition, GC, and Traceback Fidelity

## Goal

Prove the completed function, scope, and exception behavior composes across the
native synchronous core rather than passing isolated fixtures.

## Integrated work

- Combine decorators/defaults, recursive closures, class bodies, descriptors,
  comprehensions, generator expressions, expanded calls, pattern matching,
  exception groups, and nested cleanup.
- Stress callback mutation and reentrancy while bindings, cells, handled
  exceptions, traceback frames, and pending completions remain live.
- Force GC at allocation and invocation safepoints and prove exact persistence
  of live state plus collection of dead activations and exception graphs.
- Force `MemoryError` during function construction, argument preparation,
  exception creation, traceback growth, and cleanup; previously visible state
  must remain valid and partial publication must not occur.
- Scan public artifacts for CPython, generated-C, `setjmp`, and `longjmp`
  symbols.

## Completion proof

A compact composition corpus matches CPython 3.12.11 stdout, stderr, status,
traceback shape, and side-effect order under normal and constrained heaps.
