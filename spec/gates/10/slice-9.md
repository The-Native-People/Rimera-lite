# Gate 10 Slice 9 — `async with`, Partial Entry, Suppression, and Suspended Cleanup

## Goal

Extend Gate 8's compiler-planned context-manager cleanup to asynchronous enter
and exit without creating a second exception or cleanup representation.

## Integrated work

- Resolve special `__aenter__` and `__aexit__` on the type in Python order,
  await their results, and bind `as` targets only after successful entry.
- Lower multiple managers with left-to-right entry and right-to-left exit,
  including partial-entry failure, break/continue/return, body exception,
  cancellation, and an exit operation that itself suspends or fails.
- Pass the exact managed exception triple, honor truth-based suppression, and
  preserve exception cause/context when exit replaces the pending completion.
- Reuse Gate 8 cleanup actions and pending completion slots across suspension;
  synchronous and asynchronous managers may compose in nested scopes.
- Root every entered manager, bound exit callable, awaitable, exception, and
  pending transfer until its cleanup is irrevocably complete.

## Performance invariants

Exit methods are captured once per entered manager. Suspension reuses the
existing cleanup record and does not allocate a new exception triple, schedule
a cleanup task, or dynamically select an executor backend.

## Completion proof

Public differentials cover lookup order, multiple/partial managers, suppression,
all control transfers, failing/suspending entry and exit, cancellation races,
nested sync/async managers, tracebacks, cycles, and low-heap atomicity.

## Closure evidence

`gate10_slice9_async_with.py` matches CPython 3.12 through the public native
pipeline for type-level `__aenter__`/`__aexit__` lookup despite instance shadows,
left-to-right multiple entry and right-to-left exit, partial-entry failure,
target-assignment failure, truth-based suppression, replacement/chaining,
return/break/continue, nested synchronous/asynchronous managers, body awaits,
and cancellation injected while cleanup itself subsequently suspends. Entry and
exit both exercise real await suspension and failure. The fixture also compares
exit-failure traceback frame names and completes 1,500 self-cyclic async-manager
lifetimes under the same 128 KiB managed-heap differential, proving cleanup roots
do not strand manager cycles. Debug MIR proves the special-method and generic
await/delegation path, and the resulting artifacts remain native-only.

The closure audit additionally proves descriptor binding order and captured-exit
stability across class mutation and entry suspension, custom suppression
`__bool__` (including failure/chaining and no truth conversion on normal exit),
exact missing-method TypeErrors, and cancellation injected during partial entry
or an already-running exit. These run in the same public differential at both
normal and 128 KiB heap limits.

## DONE BY CHATGPT
