# Gate 3 Slice 3 — Structured Exceptions for Gate 3 Builtins

## Goal
Stop converting builtin/runtime failures into generic `TypeError` or `RuntimeError` based on message strings. Gate 3 operations must raise the concrete Python exception class they own.

## Remaining implementation
Create or reuse a structured runtime error path that carries the intended builtin exception type together with its message. Gate 3 operations should select the exception at the point where the semantic failure is detected rather than relying on FFI text heuristics.

At minimum cover:
- `slice step cannot be zero` → `ValueError`.
- Invalid sequence index type → `TypeError`.
- Out-of-range sequence indexing → `IndexError`.
- Missing dictionary subscription key → `KeyError`.
- Missing `dict.pop(key)` without default → `KeyError`.
- Missing `set.remove(value)` → `KeyError`.
- Bytearray resizing while exported → `BufferError`.
- Writable/unhashable memoryview hashing → correct `ValueError`/`TypeError` behavior according to CPython case.
- Invalid memoryview cast shape/format → correct `TypeError` or `ValueError`.
- `chr()` outside the Unicode range → `ValueError`.
- Invalid `int()` literal → `ValueError`.
- Invalid `int()` base → `ValueError`.
- Converting positive/negative infinity to integer → `OverflowError`.
- Converting NaN to integer → `ValueError`.
- Invalid `float()` text → `ValueError`.
- Invalid `complex()` text → `ValueError`.
- Bytes/bytearray element outside `0..255` → `ValueError`.
- `pow(base, exp, 0)` → `ValueError`.
- Negative-exponent modular `pow()` with no inverse → `ValueError`.
- `list.index()` with no matching value → `ValueError`.
- `list.remove()` with no matching value → `ValueError`.
- `list.pop()` with an invalid index → `IndexError`.
- Invalid format specifications → `ValueError` where CPython uses `ValueError`.

## Architectural requirements
- Do not add compiler-specific exception hacks for individual builtin calls.
- Preserve an already-raised user exception from callbacks such as `__hash__`, `__eq__`, `__index__`, `__format__`, `__iter__`, or `__next__`.
- Avoid using substring matching as the authoritative semantic classifier.
- Full exception hierarchy completeness remains Gate 5; only concrete exception classes required by Gate 3 behavior must be available here.

## Completion criteria
Representative Gate 3 failures can be caught by their correct Python exception class through normal compiled Python `try/except` code.

## DONE BY CHATGPT
