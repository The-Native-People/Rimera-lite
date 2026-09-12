# Gate 10 Slice 8 — Async Generators, `asend`, `athrow`, `aclose`, and Finalization

## Goal

Implement the complete Python asynchronous-generator object protocol on the
owned generator suspension and async execution foundations.

## Integrated work

- Create lazy async-generator objects for `async def` bodies containing `yield`;
  prohibit non-empty return and model their distinct running/closed state.
- Implement `__aiter__`, `__anext__`, `asend`, `athrow`, and `aclose`, including
  first-send rules, overlapping-operation errors, yielded wrappers,
  `StopAsyncIteration`, `GeneratorExit`, and ignored-close behavior.
- Resume awaited operations inside the generator without exposing an internal
  yielded token as a Python item or submitting a task per generated value.
- Preserve exception state, `finally`, async cleanup, retained locals, closure
  cells, frames, and traceback metadata over yield and await suspension.
- Provide exactly-once abandonment/shutdown finalization hooks while leaving
  public event-loop async-generator hook APIs to the standard-library gate.

## Performance invariants

Reuse one generator frame and one operation object at a time. Yield/await
transitions avoid per-poll adapter allocation and do not create nested executor
tasks for internal awaits.

## Completion proof

Public differentials cover every operation, overlap/reuse failures, await/yield
interleaving, exception injection, cancellation, close, abandonment, cleanup,
reflection, cycles, constrained heaps, and long-stream allocation budgets.

## Closure evidence

`gate10_slice8_async_generators.py` matches CPython 3.12 through the public native
pipeline for lazy async-generator construction and reflection, `__aiter__`,
`__anext__`, `asend`, `athrow`, and `aclose`, first-send consumption, operation
reuse, overlapping owners, `ag_running`/`ag_await`, yielded values around an
internal await, injected failure/cancellation-style `BaseException`, traceback
state, ignored `GeneratorExit`, and `finally` cleanup whose close path itself
awaits. The same fixture completes 5,000 generated values under a 96 KiB managed
heap, so completed operation wrappers are not retained across the long stream.
`gate10_slice8_finalization.py` proves unreachable cyclic and still-live shutdown
async generators run their finalization exactly once. A non-empty async-generator
`return` is rejected before artifact publication. Focused Slice 8 native tests,
85 runtime unit tests plus the public runtime ABI test, 55 compiler-lib tests,
warning-denied runtime/compiler Clippy, rustfmt, native-only artifact checks, and
`git diff --check` are green.

## DONE BY CHATGPT
