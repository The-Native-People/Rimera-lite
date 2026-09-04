# Gate 7 Slice 6 — Generator Identity, State, and Suspension Metadata

## Goal

Publish the completed Gate 6 generator's Python-visible ownership and state
without exposing or allowing mutation of its native resume machinery.

## Integrated work

- Expose generator name/qualified name, code/frame ownership, running state,
  yield-from delegate, and supported suspension metadata through ordinary
  managed attributes.
- Keep never-started, running, suspended, delegated, closed, completed, and
  failed states coherent across `next`, `send`, `throw`, and `close`.
- Define frame/local observability while suspended and after completion,
  including identity, clearing, delegate transitions, and retained references.
- Prevent writes to read-only state and prevent reflective access from bypassing
  reentrancy, lifecycle, exception, or cleanup rules.
- Trace visible generator/frame/delegate graphs and reclaim terminal state when
  no reflective reference keeps it alive.

## Completion proof

- `gate7_slice6_generator_identity_state_and_suspension_metadata_match_cpython_312`
  differentially proves generator `__name__`, `__qualname__`, `gi_code`,
  `gi_frame`, `gi_running`, `gi_suspended`, and `gi_yieldfrom` across
  never-started, running, suspended, delegated, closed, completed, and failed
  states, including writable name metadata and read-only lifecycle fields.
- `gate7_slice6_retained_generator_frames_survive_gc_and_terminal_detach` proves
  suspended frame/local identity and liveness through constrained-heap GC,
  terminal generator-to-frame detachment, and independently retained frame/local
  survival after the generator is released.
- `gate7_slice6_generator_frame_publication_is_low_heap_atomic` proves failed
  generator/frame construction leaves the call output untouched, publishes no
  partial managed graph, and raises managed `MemoryError`.
- Verified generator yield code publishes only Python-visible suspension-line
  metadata through `rimera_generator_frame_line_set`; native resume addresses,
  state-machine layout, and reentrancy remain opaque and unmodifiable.
- Generator and frame payloads are boxed inside the managed-object union while
  their owned payload bytes remain explicitly accounted. This prevents new
  reflection payloads from inflating every unrelated managed object and keeps
  the established 8 KiB startup/GC contract green.
- Final acceptance is green: Gate 7 focused proof is 20/20, the complete
  workspace is 289/289 with doc-tests green, formatting and warning-denied
  clippy pass, and `scripts/verify_release.sh` passes at 502,936 bytes with no
  forbidden legacy symbols.

## DONE BY CHATGPT
