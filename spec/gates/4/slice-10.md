# Gate 4 Slice 10 — Complete Slicing and Extended Subscripts

## Goal

Finish source-level slice construction and extended subscript tuples for every
Gate 3 sequence and memoryview implementation.

## Implementation

- Preserve omitted start/stop/step distinctly from explicit `None` until slice
  construction, then materialize native `slice` values.
- Support `value[a:b:c]`, multidimensional comma-separated indices, and mixed
  index/slice tuples.
- Invoke `__index__` for bounds and indices through the generic protocol.
- Route get/set/delete through item protocols with explicit exception edges.
- Preserve receiver, index tuple, and right-hand side single evaluation.
- Pin normalized positive/negative steps, clipping, zero-step failures, range
  slice preservation, and mutable slice assignment/deletion.

## Proof

- CPython differentials cover strings, bytes, bytearray, list, tuple, range,
  memoryview, and user `__getitem__`/`__setitem__`/`__delitem__` objects.
- MIR and GC tests prove exact roots for all extended-index components.
