# Gate 3 Slice 13 — Reverse Iteration Gaps

## Goal
Make `reversed()` work for all Gate 3 builtin sequences/collections that CPython supports through direct builtin behavior or the generic `__reversed__` / `__len__` + `__getitem__` fallback.

## Completed implementation
- Fix `reversed(bytes)`: the current exact-bytes path creates a reverse-sequence iterator whose `__next__` implementation does not handle bytes.
- Support `reversed(dict)` and yield keys in reverse insertion order.
- Support `reversed(dict.keys())`.
- Support `reversed(dict.values())`.
- Support `reversed(dict.items())`.
- Ensure reverse dict-view iteration observes the same mutation/version rules as forward iteration.
- Recheck bytearray, range, list, tuple, and string reverse paths after consolidation.
- Preserve custom `__reversed__` priority.
- Preserve the sequence fallback through `__len__` and `__getitem__` for user objects.
- Propagate user exceptions from `__reversed__`, `__len__`, and `__getitem__` unchanged.

## Completion criteria
`reversed()` produces CPython-shaped values/order for bytes, dicts, all three dictionary views, ordinary sequences, ranges, and supported protocol objects.

## Completion evidence
- Native reverse-sequence iteration now handles bytes and bytearray directly, while dicts and all three live dictionary-view kinds yield reverse insertion order without materializing a replacement collection.
- Reverse dict/view iterators carry the same collection version contract as forward dictionary iteration and raise structured `RuntimeError` when size-changing mutation is observed.
- `range` reversal uses an arithmetic `Range` iterator with arbitrary-precision current/stop/step values, so enormous ranges reverse in O(1) space and do not depend on `Py_ssize_t` length conversion.
- Custom `__reversed__` remains first priority, and the generic `__len__` + `__getitem__` fallback remains intact for user objects; exceptions from all three hooks propagate unchanged.
- The public `gate3_reversed_and_range_semantics_match_cpython_312` differential covers bytes, bytearray, dict, keys/values/items views, mutation failure, ordinary sequences, custom `__reversed__`, fallback protocol objects, and hook exceptions against `/opt/homebrew/bin/python3.12`.

## DONE BY CHATGPT
