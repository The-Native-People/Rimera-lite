# Gate 10 Slice 5 — Timers, Wakeups, Cancellation Delivery, and Buffer Ownership

## Goal

Provide wake-driven time and completion primitives that asynchronous protocols
can consume safely without pre-claiming the `asyncio` networking API.

## Integrated work

- Define backend-neutral monotonic-deadline, timer registration, wake, cancel,
  completion, and shutdown records with explicit thread and ownership rules.
- Map timers and native operation completions directly to backend wakers; never
  scan all tasks, poll wall-clock time, or wake an unrelated Python task.
- Coalesce duplicate wakes without losing a wake that races with transition to
  pending. Define cancellation request, observation, acknowledgement, and
  exactly-once terminal completion as separate states.
- Deliver observed cancellation as a managed exception injection at a valid
  suspension point. This is an internal language/runtime signal and does not
  yet claim the full `asyncio.CancelledError` or Task API.
- Root or pin managed/native buffers and their owner objects until operation
  completion or acknowledged cancellation. Document every unavoidable copy and
  forbid callbacks from retaining expired borrows.

## Performance invariants

Idle tasks consume no CPU. Timer registration and task completion avoid global
scans, the common wake path allocates nothing, and buffer lifetime safety does
not require copying on every poll.

## Completion proof

Deterministic-clock tests, wake-race model tests, cancellation-race tests,
allocation counters, buffer lease tests, shutdown stress, and public native
fixtures prove no lost wake, use-after-free, duplicate completion, or busy loop.

## Closure evidence

The backend-neutral facade suite proves deterministic timer fire/cancel/late-fire,
stale-generation isolation, wake-during-poll coalescing, cancellation
request/observe/acknowledge separation, shutdown during resume, and no background
progress or hidden queue. Runtime tests prove managed Throw delivery, caught and
uncaught cancellation, low-heap MemoryError delivery, bytearray resize pinning,
release on completion/cancellation/drop/shutdown, and atomic failed lease
creation. The PEP 688 provider fixture survives collection with its provider,
exported view, and shared lease rooted, then invokes `__release_buffer__` exactly
once. The synchronous public artifact retains zero async/backend symbols.

## DONE BY CHATGPT
