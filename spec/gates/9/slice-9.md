# Gate 9 Slice 9 — Authoritative `sys.modules`, `__import__`, and Cache Mutation

## Goal

Expose the existing context-owned cache as Python-visible import state without
duplicating storage.

## Integrated work

- Provide a managed `sys` module whose `modules` mapping is the live
  authoritative cache used by every import operation.
- Publish the ordinary `__import__` builtin through generic calls with Python
  argument binding, return-value, `globals`/`locals`, `fromlist`, and `level`
  behavior.
- Honor supported insertion, replacement, and deletion of cache entries while
  validating module/non-module effects at the same points as CPython.
- Root cache keys/values and mutations atomically under allocation failure.

## Completion proof

Public differentials cover cache identity, direct `__import__`, fromlists,
levels, deletion/reimport, replacement, `None` entries, invalid arguments,
mutation during import, and low-heap failure atomicity.

## DONE BY CHATGPT
