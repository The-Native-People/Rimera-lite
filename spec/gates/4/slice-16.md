# Gate 4 Slice 16 — List Comprehensions

## Goal

Compile list comprehensions through the shared comprehension function and
native list operations.

## Implementation

- Allocate the result list inside the implicit comprehension activation.
- Execute nested clauses and filters in source order.
- Append each element through the native list protocol while rooting the list,
  iterators, targets, closure cells, and element.
- Return the list on normal exhaustion and propagate target/iterator/filter/
  element failures with traceback frames.
- Preserve independent activations under recursion and nested comprehensions.

## Public fixtures

- Single/nested loops, multiple filters, destructuring targets, closure reads,
  walrus bindings, side effects, and exceptions.
- Large growth under forced GC and configured heap limits.
- Scope non-leakage and outermost-iterable evaluation order.

## Completion evidence

- `<listcomp>` activations allocate their result list inside the hidden native
  function and append through `rimera_list_append`; the sink is a fallible MIR
  safepoint whose precise roots include the result and produced element plus
  loop-carried iterator state as required by liveness.
- `gate4_list_comprehensions.py` and the former list-comprehension capability
  fixture match CPython 3.12 for single/nested loops, filters, recursive and
  starred destructuring, closure reads, nested activations, side effects, and
  iterator/target/filter failure propagation.
- `gate4_list_comprehension_growth.py` proves 4096-element growth normally and
  deterministic `MemoryError: managed heap limit exceeded` under a configured
  budget. Runtime ABI proof separately pins retained-vector managed-byte
  refresh and heap-limit enforcement after in-place append.

## DONE BY CHATGPT
