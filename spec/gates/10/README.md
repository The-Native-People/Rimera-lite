# Gate 10 — Async, Await, and Asynchronous Protocols

Gate 10 implements Python 3.12 asynchronous language semantics after the module
system is stable. It reuses the existing call binder, exception completion,
generator suspension, descriptor dispatch, and context-manager cleanup. It may
not add an async interpreter or a second object/call/exception model.

The gate proves protocol behavior independently of `asyncio`; the standard
library event loop belongs to Gate 14. Tests may use a small native protocol
driver to resume awaitables deterministically.

## Progress ledger

- [ ] Slice 1 — ownership, async syntax/HIR, suspension contract, and oracle matrix
- [ ] Slice 2 — coroutine objects, calls, `await`, and lifecycle diagnostics
- [ ] Slice 3 — `__await__`, delegation, cancellation injection, and cleanup
- [ ] Slice 4 — async iteration, `async for`, and async comprehensions
- [ ] Slice 5 — async generators, `asend`, `athrow`, `aclose`, and finalization hooks
- [ ] Slice 6 — `async with`, partial entry, suppression, and suspended cleanup
- [ ] Slice 7 — task-style composition, context propagation, GC, and atomicity
- [ ] Slice 8 — final audit, documentation, and gate closure

## Slice acceptance

1. Inventory Python 3.12 async forms and define MIR suspension/resume/injection
   contracts shared with, but distinct in kind from, synchronous generators.
2. Make `async def` calls lazy coroutine construction; prove awaiting, return,
   reuse errors, unawaited lifecycle behavior, identity, frames, and tracing.
3. Drive generic awaitables through `__await__`, preserve exception/cause state,
   and execute `finally` exactly once under injected cancellation or close.
4. Implement `aiter`/`anext`, `StopAsyncIteration`, `async for` cleanup, and
   scope/evaluation behavior for async comprehensions.
5. Implement the full asynchronous-generator protocol and its interaction with
   exceptions, delegation boundaries, shutdown, and retained state.
6. Extend Gate 8 cleanup actions to asynchronous enter/exit without duplicating
   lookup, exception triples, or pending-completion representation.
7. Compose nested coroutines, async iterators/generators/managers, callbacks,
   cancellation, constrained heaps, and dead-cycle reclamation.
8. Audit deferred diagnostics and public differentials before promoting async.

Schedulers, sockets, and `asyncio` are later library/platform work; this gate
completes the language and object protocols they require.
