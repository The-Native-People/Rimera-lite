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

## Completion evidence

- The existing Gate 3 slice runtime is now proven through the complete Gate 4
  source surface: omitted bounds stay `Option::None` until native `SliceNew`,
  explicit `None` remains an ordinary bound value, and comma-separated/mixed
  subscripts materialize one tuple passed unchanged to generic item protocols.
- Generic `__index__` conversion continues to own scalar indices and slice
  bounds; source-level get/set/delete preserve receiver/index/RHS evaluation
  exactly once and use the existing fallible item operations.
- `gate4_slicing_and_extended_subscripts_match_cpython_312` is byte-for-byte
  green for list, tuple, string, bytes, bytearray, range, multidimensional
  memoryview scalar access/mutation, user get/set/delete objects, positive and
  negative steps, mutable slice assignment/deletion, and zero-step failure.
- `gate4_extended_subscript_roots_receiver_tuple_components_and_rhs` pins the
  native slice construction, tuple-index component roots, final index tuple,
  receiver, RHS, and the explicit exception successor on `ItemSet`.
- `gate4_extended_subscript_callback_keeps_index_tuple_slice_and_rhs_alive`
  forces collection inside user `__setitem__` and verifies the tuple, nested
  slice, and RHS are all still live inside the callback.

## DONE BY CHATGPT
