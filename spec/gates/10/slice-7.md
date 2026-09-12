# Gate 10 Slice 7 — Async Iteration, `async for`, and Async Comprehensions

## Goal

Implement Python 3.12 asynchronous iteration entirely through generic
`__aiter__`/`__anext__` awaitable protocols and compiler-planned cleanup.

## Integrated work

- Implement builtin `aiter`/`anext` behavior and special-method lookup with
  exact missing, invalid iterator, invalid awaitable, default, and
  `StopAsyncIteration` behavior.
- Lower `async for` into explicit acquisition, awaited next, target binding,
  exhaustion, break/continue/return, exception, and cleanup blocks.
- Implement async list/set/dict comprehensions and generator expressions with
  Python 3.12 evaluation order, scopes, closure cells, filters, and mixed
  synchronous/asynchronous clauses.
- Preserve iterator and awaitable roots across suspension, and correctly close
  retained async-generator sources when the owning construct requires it.
- Maintain source spans and managed tracebacks for failures in acquisition,
  awaiting, assignment targets, filters, bodies, and cleanup.

## Performance invariants

The loop reuses one compiler-owned iteration state and resume frame. It creates
no executor task per element and adds no allocation beyond objects required by
the user-visible iterator/awaitable protocol.

## Completion proof

Public differentials cover builtin helpers, custom iterators, exhaustion,
defaults, malformed hooks, control transfers, nested/mixed comprehensions,
scope leakage, mutations, failures, cancellation, collection, and large-loop
allocation/throughput budgets.

## Closure evidence

The public async-iteration fixture matches CPython 3.12 for `async for`,
break/continue/else, `aiter`, one- and two-argument `anext`, malformed
`__aiter__`/`__anext__`, injected failure through a suspended `__anext__` with
await/loop cleanup, and a 1000-element loop; the same fixture passes under a
64 KiB managed heap. Async list/set/dict comprehensions prove filters, scope
isolation, and mixed sync/async clauses. Async generator expressions execute on
the single Slice 8 async-generator substrate and are consumed through
`__anext__` without spawning an executor task per element. Native artifacts stay
on the verified MIR/Cranelift path and match CPython output/error behavior.

## DONE BY CHATGPT
