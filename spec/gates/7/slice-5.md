# Gate 7 Slice 5 — Exception, Traceback, and Frame Inspection

## Goal

Expose Gate 5 exception graphs and native traceback frames as safe managed
Python-visible metadata.

## Integrated work

- Complete inspection and supported mutation of exception `args`,
  `__traceback__`, `__cause__`, `__context__`, and `__suppress_context__` while
  preserving normalization and cycle safety.
- Expose traceback links, line/source positions, and frame references with
  stable identity and no native pointer leakage.
- Represent supported frame code/global/local/back-link metadata with explicit
  snapshot/live behavior consistent with the Slice 1 contract.
- Preserve frame order and exception identity through calls, reraises,
  exception groups, cleanup, and suspended generators.
- Ensure retained tracebacks keep required frames/locals alive and releasing
  them permits cycles to be collected.

## Completion proof

- `gate7_slice5_exception_traceback_and_frame_metadata_match_cpython_312`
  differentially proves caught/reraised/grouped exception metadata, exact
  traceback/frame ordering and line ownership, stable `tb_frame` identity,
  `f_code`/`f_globals`/`f_locals`/`f_back`/`f_lineno`, supported exception and
  `tb_next` mutation, and read-only frame/traceback failures.
- `gate7_slice5_retained_traceback_frames_survive_gc_pressure` proves retained
  traceback/frame/local graphs remain valid through constrained-heap collection
  and that dead cycles are reclaimable.
- `gate7_slice5_traceback_frame_allocation_is_low_heap_atomic` proves metadata
  allocation failure preserves the original exception graph, leaves no partial
  traceback publication, and raises managed `MemoryError`.
- Compiler exception edges now append each newly crossed Python frame exactly
  once. Bare reraising and cleanup propagation preserve the existing traceback
  instead of duplicating a frame.
- Module traceback capture uses the dedicated lightweight
  `rimera_traceback_append_module` ABI while function/generator capture uses
  `rimera_traceback_append`; both publish managed frame metadata without raw
  machine pointers, while the split preserves release reachability.
- Final acceptance is green: Gate 7 focused proof is 20/20, the complete
  workspace is 289/289 with doc-tests green, formatting and warning-denied
  clippy pass, the 8 KiB managed-heap regression remains green, and
  `scripts/verify_release.sh` passes at 502,936 bytes with no forbidden legacy
  symbols.

## DONE BY CHATGPT
