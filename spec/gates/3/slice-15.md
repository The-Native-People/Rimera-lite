# Gate 3 Slice 15 — Slice Object Surface

## Goal
Finish the normal Python-visible managed `slice` object surface on top of the already-implemented construction, hashing, comparison, and slicing semantics.

## Completed implementation
- Expose `slice.start`.
- Expose `slice.stop`.
- Expose `slice.step`.
- Implement `slice.indices(length)`.
- `indices()` must accept integer-like length through `__index__`.
- Reject negative lengths with the same exception behavior as CPython.
- Reuse the authoritative slice-normalization logic rather than creating a second normalization algorithm.
- Preserve arbitrary-size integer handling where the Python API supports it.
- Preserve hash/equality/repr semantics after exposing attributes.
- Ensure slice objects remain immutable.

## Completion criteria
Normal source can inspect a `slice` object's fields and call `.indices()` with CPython-compatible normalized results and failures.

## Completion evidence
- Managed slice objects expose immutable `.start`, `.stop`, and `.step` attributes through ordinary attribute lookup, preserving explicit `None` values.
- `.indices(length)` is a bound managed method and accepts arbitrary integer-like lengths through `__index__`.
- The method reuses the authoritative arbitrary-precision slice normalization path used by range slicing rather than maintaining a second normalization algorithm.
- Explicit `None` bounds are normalized as omitted bounds, including negative-step cases; zero steps raise structured `ValueError` and negative lengths raise structured `ValueError` with CPython-shaped behavior.
- Arbitrary-size lengths remain arbitrary precision in the returned `(start, stop, step)` tuple.
- Existing slice hash/equality/repr behavior is unchanged and attribute assignment remains rejected.
- `gate3_slice_and_memoryview_remaining_semantics_match_cpython_312` compares fields, huge normalization, `__index__` lengths, negative-length/zero-step failures, and immutability byte-for-byte with `/opt/homebrew/bin/python3.12`.

## DONE BY CHATGPT
