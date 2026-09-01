# Gate 4 Slice 23 — Cross-Feature Ordering, GC, and Failure Atomicity

## Goal

Prove the slices compose correctly rather than passing only isolated fixtures.

## Implementation and proof matrix

- Expanded calls inside comprehensions and f-strings.
- Nested/starred unpacking in loops, comprehensions, and `match` bodies.
- Walrus expressions in boolean chains, guards, and comprehension filters.
- Attribute/item augmented assignment and deletion during iterator callbacks.
- Slices and class patterns invoking user descriptors and dunders.
- Exceptions crossing comprehension activations, formatting calls, matching,
  and partial target stores.
- Forced collection at every allocation/invocation safepoint; dead temporary
  iterators, tentative bindings, expansion buffers, and comprehension results
  become collectible.
- Heap-limit failures leave mappings, target state, pattern state, and runtime
  exception state valid.
- Every public artifact excludes CPython, generated C, `setjmp`, and `longjmp`.

## Completion criteria

Run a combined CPython 3.12.11 differential corpus through the public build API
and pin stdout, stderr, exit status, traceback shape, and side-effect logs.
