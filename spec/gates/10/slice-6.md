# Gate 10 Slice 6 — Generic `__await__`, Delegation, Injected Failures, and Cleanup

## Goal

Await arbitrary supported Python awaitables through normal object protocols and
preserve exact completion and cleanup behavior across delegation.

## Integrated work

- Resolve `__await__` through ordinary special-method lookup, call it once per
  await expression, require an iterator result, and reject invalid results with
  CPython-shaped managed errors.
- Drive the returned iterator using the existing iteration/send/throw/close
  machinery while distinguishing coroutine awaiting from `yield from`.
- Route values, `StopIteration.value`, exceptions, explicit close, and observed
  cancellation through one verified delegation state machine.
- Preserve cause/context/suppression, traceback frames, pending returns, and
  `finally`/context cleanup exactly once when an injection replaces completion.
- Trace awaitable, iterator, yielded token, awaiting parent, exception, and
  retained locals through cycles and constrained collection.

## Performance invariants

Cache only lookup facts already legal under the object-model invalidation
rules. Delegation reuses the suspension frame and performs no adapter allocation
per step; direct native coroutine awaiting retains the faster Slice 3 path.

## Completion proof

Public differentials cover native and user awaitables, malformed `__await__`,
nested delegation, return values, send/throw/close, cancellation replacement,
cleanup ordering, mutation invalidation, tracebacks, cycles, and low heaps.

## Closure evidence

`gate10_slice6_awaitables.py` is compiled natively and compared byte-for-byte with
CPython 3.12. It covers one-shot `__await__` resolution, yielded tokens and send
values, `StopIteration.value`, malformed/missing hooks, injected Throw, Close and
`finally` cleanup, live `__await__` mutation, and a self-cycle retained through
await. The same differential passes under a 64 KiB managed heap. MIR evidence
contains the generic `await_iter` plus shared delegation path while direct native
coroutines retain the Slice 3 fast path. Warning-denied runtime/compiler Clippy
and formatting are green.

## DONE BY CHATGPT
