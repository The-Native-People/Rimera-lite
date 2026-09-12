# Gate 10 Slice 3 — Coroutine Objects, Lazy Calls, Direct `await`, and Lifecycle

## Goal

Make native `async def` calls create Python-compatible lazy coroutine objects
and make direct coroutine awaiting execute through owned suspension frames.

## Integrated work

- Add one GC-traced coroutine object over the existing managed function, code,
  frame, closure-cell, argument-binder, and exception models.
- Bind arguments when the async function is called, defer body execution until
  first resume, and preserve identity, frame/code metadata, return values, and
  traceback locations.
- Implement direct `await` of native coroutine objects, nested coroutine return
  propagation, exception propagation, completed-coroutine reuse errors, close,
  and unawaited-coroutine lifecycle diagnostics.
- Root frames, arguments, closures, awaiting parents, exceptions, and tracebacks
  across allocation and collection; reclaim unreachable coroutine cycles.
- Keep coroutine state distinct from synchronous generators while sharing the
  proven completion, cleanup, and suspended-root infrastructure.

## Performance invariants

Allocate the coroutine and its reusable frame storage once. A direct await
resumes the child without submitting a second executor task, building an
intermediate future chain, or allocating on each resume.

## Completion proof

Public CPython differentials cover laziness, binding errors, nested awaits,
return and failure propagation, reuse, close, warnings, reflection, traceback,
cycles, and constrained heaps through a native protocol driver.

## Closure evidence

`gate10_slice3_native_coroutine_protocol_matches_cpython_and_warns_when_unawaited`
passes against CPython 3.12.11 for lazy calls, nested direct awaits, exception
propagation, reuse/close behavior, coroutine reflection, traceback shape, and
unawaited warnings. Its 16 KiB managed-heap run also matches CPython while the
artifact remains native-only. Direct nested awaits stay inside the existing
suspended activation and do not submit executor tasks.

## DONE BY CHATGPT
