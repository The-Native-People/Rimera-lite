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

## Acceptance evidence — 2026-09-04

- `gate5_slice7_cross_feature_composition_matches_cpython_312_under_gc_pressure`
  reuses the established Gate 4/Gate 5/Gate 6 composition corpus under a
  160,000-byte heap limit, covering class/descriptors, comprehensions, expanded
  calls, scopes/cells, defaults/metadata cycles, generators, cleanup, and
  callbacks through the public build path.
- `gate5_exception_graphs_survive_low_heap_collection_and_failed_metadata_is_atomic`
  remains green for rooted exception graphs, collection, and failed metadata
  publication.
- The final audit finds one authoritative binder, `CellObject`,
  `ExceptionObject`, `TracebackObject`, and compiler `CleanupAction` model; no
  Gate 5 ignored tests, `todo!`, `unimplemented!`, or host-unwind escape path is
  present.

## DONE BY CHATGPT
