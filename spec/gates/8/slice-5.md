# Gate 8 Slice 5 — Exception Triples, Suppression, Replacement, and Chaining

## Goal

Complete exceptional context-manager exit semantics using Gate 5 exception and
Gate 7 traceback metadata.

## Integrated work

- Pass the active exception type, identical value, and traceback to `__exit__`
  for body, target, nested-entry, and inner-cleanup failures.
- Suppress the pending exception only when the exit result is truthy through the
  generic truth protocol; normal flow then resumes at the correct continuation.
- If truth testing or `__exit__` raises, replace the pending exception while
  retaining the previous one as context with correct cause/suppression state.
- Compose multiple exit methods that suppress, replace, or raise, preserving
  right-to-left order, traceback attachment, handler state, and final identity.
- Root exception/traceback/manager/result graphs across callbacks and reclaim
  cycles after cleanup finishes.

## Completion proof

Public CPython differentials cover exact triples, suppression truthiness,
replacement/chaining, multiple exits, reraises, groups, tracebacks, forced GC,
and allocation failure during exceptional cleanup.

## Completion evidence

- `gate8_exceptional_cleanup.py` proves identical exception values and
  tracebacks, generic truth callbacks, false/true suppression, truth and exit
  replacement, reraising, right-to-left nested chaining, exception groups, and
  cleanup-time `MemoryError` against CPython 3.12.11 under a constrained heap.
- The public test is
  `gate8_slice5_exception_triples_suppression_replacement_and_chaining_match_cpython_312`.
