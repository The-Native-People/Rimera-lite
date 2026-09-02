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

## Completion evidence

- `gate4_composition.py` combines expanded calls inside comprehensions/f-strings,
  recursive and starred loop/comprehension/match-body unpacking, walrus filters
  and guards, attribute/item in-place mutation and deletion during iterator
  callbacks, user slice `__getitem__`, class patterns, and descriptor access.
  Its stdout/side-effect log is byte-for-byte equal to CPython 3.12.11.
- Composition exposed and fixed a protocol-order defect: plain user instances
  now receive `__getitem__` before native slice specialization, so user
  `obj[1:7:2]` observes the managed `slice` object exactly as CPython does.
- `gate4_composition_failures.py` proves exceptions cross comprehension,
  formatting, match-descriptor, slice-dunder, and partial-target boundaries
  without corrupting previously visible state. The iterator ABI now preserves
  an already-raised managed exception instead of replacing it with `TypeError`.
- `gate4_composition_traceback.py` pins CPython 3.12 frame positions and final
  exception identity. Hidden list/set/dict comprehension implementation
  activations are traceback-transparent, matching Python 3.12's inlined
  comprehension behavior; generator expressions retain their real generator
  frame boundary.
- `gate4_composition_gc.py` is green against CPython under a 32 KiB managed
  heap limit while expanded-call, formatting, descriptor, structural-pattern,
  walrus, comprehension, and unpacking paths allocate through fallible
  safepoints.
- `gate4_heap_limit_failure_preserves_cross_feature_state` forces a real
  `MemoryError` during comprehension growth and proves mapping contents,
  observable partial assignment state, failed pattern-capture state, and the
  handled runtime exception state remain valid afterward.
- Every fixture runs through the public build API and the shared
  `assert_native_only_artifact` forbidden-symbol scan, excluding CPython,
  generated C, `setjmp`, and `longjmp`.
- Slice-boundary verification passed `cargo fmt --check`, warning-denied
  workspace clippy, the full workspace suite (3 ABI + 8 CLI + 37 compiler lib
  + 143 native-pipeline + 57 runtime tests), doc tests, and `git diff --check`.

## DONE BY CHATGPT
